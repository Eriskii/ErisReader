//! The ChatGPT account, signed in as the Codex CLI signs in: OAuth with PKCE through the
//! browser, or a device code. Saved readable only by the user, and refreshed before it
//! expires, once for every annotation.

use crate::bridge::Bridge;
use crate::storage::{now, remove, write_private};
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex as AsyncMutex;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CALLBACK: &str = "127.0.0.1:1455";
const REDIRECT: &str = "http://localhost:1455/auth/callback";
pub const ORIGINATOR: &str = "erisreader";
/// How long a sign-in may wait for the browser or the device code.
const SIGN_IN_FOR: Duration = Duration::from_secs(900);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub access: String,
    pub refresh: String,
    /// Unix milliseconds.
    pub expires: u64,
    pub account_id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

#[derive(Clone)]
pub struct OpenAiAccount(Arc<Inner>);

struct Inner {
    path: PathBuf,
    /// The OpenAI auth server.
    base: String,
    client: reqwest::Client,
    bridge: Bridge,
    state: AsyncMutex<State>,
    snapshot: Mutex<Value>,
}

#[derive(Default)]
struct State {
    credential: Option<Credential>,
    pending: Option<Pending>,
}

struct Pending {
    task: tokio::task::AbortHandle,
    url: String,
    user_code: Option<String>,
}

fn snapshot(state: &State) -> Value {
    let credential = state.credential.as_ref();
    let status = match (credential, &state.pending) {
        (_, Some(_)) => "login-pending",
        (Some(_), None) => "connected",
        (None, None) => "signed-out",
    };
    json!({
        "providerId": "openai",
        "providerLabel": "ChatGPT",
        "status": status,
        "email": credential.and_then(|c| c.email.clone()),
        "plan": credential.and_then(|c| c.plan.clone()),
        "detail": null,
        "signInMethods": ["browser", "device-code"],
        "login": state.pending.as_ref().map(|p| json!({"url": p.url, "userCode": p.user_code})),
    })
}

impl OpenAiAccount {
    pub fn load(path: PathBuf, base: String, bridge: Bridge) -> Self {
        let credential = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Credential>(&bytes).ok());
        let state = State { credential, pending: None };
        Self(Arc::new(Inner {
            snapshot: Mutex::new(snapshot(&state)),
            path,
            base,
            client: reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("an HTTP client"),
            bridge,
            state: AsyncMutex::new(state),
        }))
    }

    pub fn snapshot(&self) -> Value {
        self.0.snapshot.lock().unwrap().clone()
    }

    pub async fn signed_in(&self) -> bool {
        self.0.state.lock().await.credential.is_some()
    }

    fn changed(&self, state: &State) {
        let value = snapshot(state);
        *self.0.snapshot.lock().unwrap() = value.clone();
        self.0.bridge.event("account", json!({"account": value}));
    }

    fn save(&self, state: &mut State, credential: Option<Credential>) -> Result<()> {
        match &credential {
            Some(credential) => write_private(&self.0.path, &serde_json::to_vec(credential)?)?,
            None => remove(&self.0.path)?,
        }
        state.credential = credential;
        Ok(())
    }

    /// The signed-in credential, refreshed first when it expires within two minutes.
    /// Concurrent callers share one refresh.
    pub async fn current(&self) -> Result<Credential> {
        let mut state = self.0.state.lock().await;
        let credential =
            state.credential.clone().ok_or_else(|| anyhow!("Sign in with ChatGPT in Settings to use its models."))?;
        if credential.expires > now() + 120_000 {
            return Ok(credential);
        }
        let request = self.post("/oauth/token").json(&json!({
            "grant_type": "refresh_token", "refresh_token": credential.refresh, "client_id": CLIENT_ID,
        }));
        let refreshed = token_credential(request, &credential.refresh).await?;
        self.save(&mut state, Some(refreshed.clone()))?;
        self.changed(&state);
        Ok(refreshed)
    }

    /// Starts signing in, `browser` or `device-code`; the account's snapshot then holds the
    /// page to open, and for a device code the code to enter there. A `login-completed` event
    /// follows.
    pub async fn login(&self, method: &str) -> Result<Value> {
        let mut state = self.0.state.lock().await;
        if let Some(pending) = state.pending.take() {
            pending.task.abort();
        }
        let flow = match method {
            "browser" => self.browser_flow().await?,
            "device-code" => self.device_flow().await?,
            _ => bail!("ChatGPT has no sign-in method {method}"),
        };
        let account = self.clone();
        let task = tokio::spawn(async move {
            let result = match tokio::time::timeout(SIGN_IN_FOR, flow.completion).await {
                Ok(result) => result,
                Err(_) => Err(anyhow!("Signing in timed out.")),
            };
            let mut state = account.0.state.lock().await;
            state.pending = None;
            let result = result.and_then(|credential| account.save(&mut state, Some(credential)));
            account.changed(&state);
            account.0.bridge.event(
                "login-completed",
                json!({"providerId": "openai", "ok": result.is_ok(), "error": result.err().map(|e| format!("{e:#}"))}),
            );
        });
        state.pending = Some(Pending { task: task.abort_handle(), url: flow.url, user_code: flow.user_code });
        self.changed(&state);
        Ok(snapshot(&state))
    }

    pub async fn cancel(&self) {
        let mut state = self.0.state.lock().await;
        if let Some(pending) = state.pending.take() {
            pending.task.abort();
        }
        self.changed(&state);
    }

    pub async fn logout(&self) -> Result<()> {
        let mut state = self.0.state.lock().await;
        if let Some(pending) = state.pending.take() {
            pending.task.abort();
        }
        self.save(&mut state, None)?;
        self.changed(&state);
        Ok(())
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.0.client.post(format!("{}{path}", self.0.base))
    }

    async fn browser_flow(&self) -> Result<Flow> {
        let verifier =
            URL_SAFE_NO_PAD.encode([*uuid::Uuid::new_v4().as_bytes(), *uuid::Uuid::new_v4().as_bytes()].concat());
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let nonce = uuid::Uuid::new_v4().to_string();
        let listener = tokio::net::TcpListener::bind(CALLBACK)
            .await
            .with_context(|| format!("listening for the sign-in on {CALLBACK} (is another sign-in open?)"))?;
        let mut url = url::Url::parse(&format!("{}/oauth/authorize", self.0.base))?;
        url.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", REDIRECT),
            ("scope", "openid profile email offline_access"),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &nonce),
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("originator", ORIGINATOR),
        ]);
        let account = self.clone();
        Ok(Flow {
            url: url.into(),
            user_code: None,
            completion: Box::pin(async move {
                let code = callback(listener, &nonce).await?;
                account.exchange(&code, &verifier, REDIRECT).await
            }),
        })
    }

    async fn device_flow(&self) -> Result<Flow> {
        let value =
            auth_json(self.post("/api/accounts/deviceauth/usercode").json(&json!({"client_id": CLIENT_ID}))).await?;
        let device = required(&value, "device_auth_id")?.to_owned();
        let code = required(&value, "user_code")?.to_owned();
        let mut interval =
            value["interval"].as_u64().or_else(|| value["interval"].as_str()?.parse().ok()).unwrap_or(5).max(1);
        let account = self.clone();
        Ok(Flow {
            url: format!("{}/codex/device", self.0.base),
            user_code: Some(code.clone()),
            completion: Box::pin(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                    let response = account
                        .post("/api/accounts/deviceauth/token")
                        .json(&json!({"device_auth_id": device, "user_code": code}))
                        .send()
                        .await
                        .map_err(http_error)?;
                    let status = response.status();
                    if status.is_success() {
                        let value: Value = response.json().await.map_err(http_error)?;
                        let redirect = format!("{}/deviceauth/callback", account.0.base);
                        return account
                            .exchange(
                                required(&value, "authorization_code")?,
                                required(&value, "code_verifier")?,
                                &redirect,
                            )
                            .await;
                    }
                    if matches!(status.as_u16(), 403 | 404) {
                        continue;
                    }
                    let error: Value = response.json().await.unwrap_or(Value::Null);
                    match error["error"].as_str().or(error["error"]["code"].as_str()) {
                        Some("slow_down") => interval += 5,
                        Some("deviceauth_authorization_pending") => {}
                        _ => bail!("ChatGPT refused the device code ({status})."),
                    }
                }
            }),
        })
    }

    async fn exchange(&self, code: &str, verifier: &str, redirect: &str) -> Result<Credential> {
        let request = self.post("/oauth/token").form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
        ]);
        token_credential(request, "").await
    }
}

struct Flow {
    url: String,
    user_code: Option<String>,
    completion: std::pin::Pin<Box<dyn Future<Output = Result<Credential>> + Send>>,
}

/// Serves the browser's return to the redirect address until it brings a code for this
/// sign-in, then stops listening.
async fn callback(listener: tokio::net::TcpListener, nonce: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut request = vec![0; 8192];
        let read = stream.read(&mut request).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&request[..read]);
        let target = request.split_whitespace().nth(1).unwrap_or_default();
        let url = url::Url::parse(&format!("http://localhost{target}")).ok();
        let query = |key: &str| url.as_ref()?.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned());
        let found = url.as_ref().is_some_and(|u| u.path() == "/auth/callback");
        let (status, body, code) = match (found, query("state"), query("code")) {
            (false, ..) => ("404 Not Found", "Not found.", None),
            (true, Some(state), Some(code)) if state == nonce && !code.is_empty() => {
                ("200 OK", "Signed in to ErisReader. You can close this window.", Some(code))
            }
            _ => ("400 Bad Request", "This sign-in did not match the one ErisReader started.", None),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        if let Some(code) = code {
            return Ok(code);
        }
    }
}

fn claims(token: &str) -> Value {
    token
        .split('.')
        .nth(1)
        .and_then(|part| URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

async fn token_credential(request: reqwest::RequestBuilder, refresh: &str) -> Result<Credential> {
    let value = auth_json(request).await?;
    let access = required(&value, "access_token")?.to_owned();
    let claims = claims(&access);
    let auth = &claims["https://api.openai.com/auth"];
    let expires =
        value["expires_in"].as_u64().map(|s| now() + s * 1000).unwrap_or(claims["exp"].as_u64().unwrap_or(0) * 1000);
    let credential = Credential {
        refresh: value["refresh_token"].as_str().filter(|s| !s.is_empty()).unwrap_or(refresh).to_owned(),
        expires,
        account_id: required(auth, "chatgpt_account_id")?.to_owned(),
        email: claims["email"].as_str().map(str::to_owned),
        plan: auth["chatgpt_plan_type"].as_str().map(str::to_owned),
        access,
    };
    ensure!(!credential.refresh.is_empty(), "ChatGPT returned no refresh token.");
    ensure!(credential.expires > now(), "ChatGPT returned an expired sign-in.");
    Ok(credential)
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("ChatGPT's answer is missing {key}."))
}

fn http_error(error: reqwest::Error) -> anyhow::Error {
    anyhow!("Could not reach ChatGPT: {}", error.without_url())
}

async fn auth_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request.send().await.map_err(http_error)?;
    let status = response.status();
    ensure!(status.is_success(), "ChatGPT refused the sign-in ({status}).");
    response.json().await.map_err(http_error)
}
