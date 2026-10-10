//! The Claude account: a Claude subscription's login, added as ErisAE adds it, from the
//! `.credentials.json` Claude Code writes, or on macOS, where Claude Code keeps its login in the
//! Keychain instead, from there. ErisReader keeps its own copy and refreshes it before it expires.
//! Claude refresh tokens are single-use, so after ErisReader refreshes its copy, the Claude Code
//! session it came from signs in again, and the other way around.
use crate::bridge::Bridge;
use crate::storage::{now, write_json};
use anyhow::{Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;

/// Claude Code's OAuth client, the only one its logins refresh through.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// What a login needs to make model calls.
const INFERENCE_SCOPE: &str = "user:inference";
/// Requests with a Claude login's token name this beta.
pub const OAUTH_BETA: &str = "oauth-2025-04-20";
/// A login this close to expiring is refreshed before it is used.
const REFRESH_MARGIN_MS: u64 = 120_000;
/// The Keychain item macOS Claude Code keeps its login in.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const NOT_A_LOGIN: &str =
    "This is not a Claude login file; choose the .credentials.json Claude Code keeps in ~/.claude";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLogin {
    pub access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    /// Unix milliseconds. A login without an expiry, such as a long-lived token, never
    /// refreshes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
    /// The Claude account's id, which Claude Code's requests name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The subscription, such as `pro` or `max`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

impl ClaudeLogin {
    /// The login in a Claude Code credentials file: its `claudeAiOauth` entry.
    pub fn parse(text: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(text).map_err(|_| anyhow!(NOT_A_LOGIN))?;
        let oauth = &value["claudeAiOauth"];
        let string = |key: &str| oauth[key].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let access = string("accessToken").ok_or_else(|| anyhow!(NOT_A_LOGIN))?;
        if let Some(scopes) = oauth["scopes"].as_array() {
            ensure!(
                scopes.iter().any(|s| s == INFERENCE_SCOPE),
                "This Claude login cannot make model calls: it lacks the {INFERENCE_SCOPE} scope"
            );
        }
        Ok(Self {
            access,
            refresh: string("refreshToken"),
            expires: oauth["expiresAt"].as_u64(),
            account_id: None,
            email: None,
            plan: string("subscriptionType"),
        })
    }

    fn expiring(&self, at: u64) -> bool {
        self.expires.is_some_and(|expires| expires <= at + REFRESH_MARGIN_MS)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    version: u32,
    /// Stable for this installation; with the account id it names the device to Anthropic.
    install_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    login: Option<ClaudeLogin>,
}

struct AccountState {
    login: Option<ClaudeLogin>,
    /// Why the stored login no longer works; cleared by adding a login.
    failure: Option<String>,
}

#[derive(Clone)]
pub struct ClaudeAccount(Arc<AccountInner>);

struct AccountInner {
    path: PathBuf,
    install_id: String,
    state: AsyncMutex<AccountState>,
    snapshot: Mutex<Value>,
    bridge: Bridge,
    client: reqwest::Client,
    /// The Anthropic API's root, where the login's profile is read.
    api: String,
    /// Where logins refresh.
    auth: String,
}

impl ClaudeAccount {
    /// The login stored at `path`, checked against the Anthropic API at `api` and refreshed at
    /// `auth`, announcing its changes on `bridge`.
    pub fn load(path: PathBuf, api: &str, auth: &str, bridge: Bridge) -> Result<Self> {
        let stored = if path.exists() {
            let stored: StoredAccount = serde_json::from_slice(&std::fs::read(&path)?)?;
            ensure!(stored.version == 1, "unsupported Claude account version");
            ensure!(!stored.install_id.is_empty(), "the stored Claude account has no install id");
            stored
        } else {
            StoredAccount { version: 1, install_id: uuid::Uuid::new_v4().to_string(), login: None }
        };
        let state = AccountState { login: stored.login, failure: None };
        Ok(Self(Arc::new(AccountInner {
            path,
            install_id: stored.install_id,
            snapshot: Mutex::new(snapshot(&state)),
            state: AsyncMutex::new(state),
            bridge,
            client: reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?,
            api: api.trim_end_matches('/').to_owned(),
            auth: auth.trim_end_matches('/').to_owned(),
        })))
    }

    pub fn install_id(&self) -> &str {
        &self.0.install_id
    }

    pub fn snapshot(&self) -> Value {
        self.0.snapshot.lock().unwrap().clone()
    }

    pub async fn signed_in(&self) -> bool {
        self.0.state.lock().await.login.is_some()
    }

    fn changed(&self, state: &AccountState) {
        let value = snapshot(state);
        *self.0.snapshot.lock().unwrap() = value.clone();
        self.0.bridge.event("account", json!({"account": value}));
    }

    /// The login, refreshed first when it is about to expire. Concurrent callers share one
    /// refresh.
    pub async fn current(&self) -> Result<ClaudeLogin> {
        let mut state = self.0.state.lock().await;
        if let Some(failure) = &state.failure {
            bail!("{failure}");
        }
        let login =
            state.login.clone().ok_or_else(|| anyhow!("Add a Claude login in Settings to use Claude models."))?;
        if !login.expiring(now()) {
            return Ok(login);
        }
        match self.refresh(&login).await {
            Ok(fresh) => {
                self.persist(&mut state, Some(fresh.clone()))?;
                self.changed(&state);
                Ok(fresh)
            }
            Err(Refresh::Refused(failure)) => {
                state.failure = Some(failure.clone());
                self.changed(&state);
                bail!("{failure}")
            }
            Err(Refresh::Failed(error)) => Err(error),
        }
    }

    /// Signs in with a Claude Code credentials file's text, once Anthropic accepts the login.
    /// Replies with the account as it now stands.
    pub async fn import(&self, text: &str) -> Result<Value> {
        let mut login = ClaudeLogin::parse(text)?;
        let mut state = self.0.state.lock().await;
        if login.expiring(now()) {
            login = match self.refresh(&login).await {
                Ok(fresh) => fresh,
                Err(Refresh::Refused(failure)) => bail!("{failure}"),
                Err(Refresh::Failed(error)) => return Err(error),
            };
        }
        let profile = self.profile(&login.access).await?;
        login.account_id = profile["account"]["uuid"].as_str().map(str::to_owned);
        login.email = profile["account"]["email"].as_str().map(str::to_owned);
        if login.plan.is_none() {
            login.plan = profile["organization"]["organization_type"]
                .as_str()
                .map(|kind| kind.strip_prefix("claude_").unwrap_or(kind).to_owned());
        }
        self.persist(&mut state, Some(login))?;
        state.failure = None;
        self.changed(&state);
        Ok(snapshot(&state))
    }

    /// Signs in with the login macOS Claude Code keeps in the Keychain.
    pub async fn import_keychain(&self) -> Result<Value> {
        ensure!(cfg!(target_os = "macos"), "Only macOS keeps Claude Code's login in the Keychain");
        let output = tokio::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
            .output()
            .await
            .map_err(|e| anyhow!("Could not read the Keychain: {e}"))?;
        ensure!(
            output.status.success(),
            "The Keychain has no Claude Code login, or access to it was refused; sign in with `claude` first"
        );
        self.import(String::from_utf8_lossy(&output.stdout).trim()).await
    }

    pub async fn logout(&self) -> Result<()> {
        let mut state = self.0.state.lock().await;
        self.persist(&mut state, None)?;
        state.failure = None;
        self.changed(&state);
        Ok(())
    }

    fn persist(&self, state: &mut AccountState, login: Option<ClaudeLogin>) -> Result<()> {
        let stored = StoredAccount { version: 1, install_id: self.0.install_id.clone(), login };
        write_json(&self.0.path, &stored)?;
        state.login = stored.login;
        Ok(())
    }

    /// The account and organization the token belongs to, which also proves Anthropic
    /// accepts it.
    async fn profile(&self, access: &str) -> Result<Value> {
        let response = self
            .0
            .client
            .get(format!("{}/api/oauth/profile", self.0.api))
            .bearer_auth(access)
            .header("anthropic-beta", OAUTH_BETA)
            .send()
            .await
            .map_err(connection)?;
        let status = response.status();
        ensure!(
            status.is_success(),
            "Anthropic refused this Claude login ({status}); sign in with Claude Code again and add the new login"
        );
        response.json().await.map_err(connection)
    }

    async fn refresh(&self, login: &ClaudeLogin) -> Result<ClaudeLogin, Refresh> {
        let Some(refresh) = &login.refresh else {
            return Err(Refresh::Refused(
                "This Claude login has expired; sign in with Claude Code again and add the new login".into(),
            ));
        };
        let response = self
            .0
            .client
            .post(format!("{}/v1/oauth/token", self.0.auth))
            .json(&json!({"grant_type": "refresh_token", "refresh_token": refresh, "client_id": CLIENT_ID}))
            .send()
            .await
            .map_err(|e| Refresh::Failed(connection(e)))?;
        let status = response.status();
        if matches!(status.as_u16(), 400 | 401 | 403) {
            return Err(Refresh::Refused(
                "Anthropic no longer accepts this Claude login: it was refreshed elsewhere or signed out. Add a fresh login".into(),
            ));
        }
        if !status.is_success() {
            return Err(Refresh::Failed(anyhow!("refreshing the Claude login failed ({status})")));
        }
        let value: Value = response.json().await.map_err(|e| Refresh::Failed(connection(e)))?;
        let access = value["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Refresh::Failed(anyhow!("Anthropic's refresh reply has no access token")))?;
        let expires_in = value["expires_in"]
            .as_u64()
            .ok_or_else(|| Refresh::Failed(anyhow!("Anthropic's refresh reply has no expiry")))?;
        let account = &value["account"];
        Ok(ClaudeLogin {
            access: access.into(),
            refresh: value["refresh_token"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| login.refresh.clone()),
            expires: Some(now() + expires_in * 1000),
            account_id: account["uuid"].as_str().map(str::to_owned).or_else(|| login.account_id.clone()),
            email: account["email_address"].as_str().map(str::to_owned).or_else(|| login.email.clone()),
            plan: login.plan.clone(),
        })
    }
}

enum Refresh {
    /// Anthropic turned the login down; only a new login helps.
    Refused(String),
    /// The refresh did not get an answer; a later one may.
    Failed(anyhow::Error),
}

fn connection(error: reqwest::Error) -> anyhow::Error {
    anyhow!("Anthropic connection failed: {}", error.without_url())
}

fn snapshot(state: &AccountState) -> Value {
    let status = match (&state.login, &state.failure) {
        (_, Some(_)) => "error",
        (Some(_), None) => "connected",
        (None, None) => "signed-out",
    };
    let mut methods = vec!["credentials-file"];
    if cfg!(target_os = "macos") {
        methods.push("keychain");
    }
    json!({
        "providerId": "anthropic",
        "providerLabel": "Claude",
        "status": status,
        "email": state.login.as_ref().and_then(|l| l.email.clone()),
        "plan": state.login.as_ref().and_then(|l| l.plan.clone()),
        "detail": state.failure,
        "signInMethods": methods,
        "login": null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credentials_file_gives_its_claude_login() {
        let login = ClaudeLogin::parse(
            &json!({"claudeAiOauth": {"accessToken": "a", "refreshToken": "r", "expiresAt": 5,
                "scopes": ["user:inference", "user:profile"], "subscriptionType": "pro"}})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            (login.access.as_str(), login.refresh.as_deref(), login.expires, login.plan.as_deref()),
            ("a", Some("r"), Some(5), Some("pro"))
        );
        let token_only = ClaudeLogin::parse(&json!({"claudeAiOauth": {"accessToken": "a"}}).to_string()).unwrap();
        assert_eq!((token_only.refresh, token_only.expires), (None, None));
        for other in ["{}", "[]", "\"text\"", r#"{"claudeAiOauth": {"accessToken": ""}}"#] {
            assert!(ClaudeLogin::parse(other).is_err(), "{other}");
        }
        let scopeless = json!({"claudeAiOauth": {"accessToken": "a", "scopes": ["user:profile"]}}).to_string();
        assert!(ClaudeLogin::parse(&scopeless).unwrap_err().to_string().contains("user:inference"));
    }
}
