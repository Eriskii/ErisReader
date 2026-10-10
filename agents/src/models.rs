//! Models annotations can use and how requests reach them, as in ErisAE: a ChatGPT login talks to
//! the Codex endpoint with the account's id; a Claude login talks to the Anthropic API as
//! Claude Code does. Each account authorizes every request, so logins refresh without touching
//! running annotations.
use crate::accounts::{Accounts, ClaudeLogin, Credential, OpenAiAccount, claude::OAUTH_BETA, openai::ORIGINATOR};
use crate::claude::ClaudeModels;
use crate::storage::{now, write_json};
use anyhow::{Result, anyhow, bail, ensure};
use erisharness::provider::{Authorization, Credentials, Provider, RateGate, Responses, ResponsesConfig};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub const CODEX: &str = "openai-codex";
pub const ANTHROPIC: &str = "anthropic";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub context_window: Option<u64>,
    /// The longest reply the model writes, where the provider reports it.
    pub max_output: Option<u64>,
    /// The thinking levels the model accepts, in the provider's order.
    pub thinking_levels: Vec<String>,
    pub thinking: Thinking,
}

/// How a model's thinking level reaches its provider.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Thinking {
    /// The level is the request's reasoning effort.
    Effort,
    /// Claude thinks adaptively, at the level as its effort. `summarized` asks for summaries
    /// of the thinking, which models that also take budgets give without asking.
    Adaptive { summarized: bool },
    /// Claude thinks within a token budget the level sizes.
    Budget,
}

impl Model {
    /// The reasoning effort to request for a thinking level, which must be one this model has.
    /// `off` requests none, leaving the model's own default.
    pub fn effort(&self, level: &str) -> Result<Option<String>> {
        if level == "off" {
            return Ok(None);
        }
        ensure!(
            self.thinking_levels.iter().any(|l| l == level),
            "{} supports thinking levels {}, not {level}",
            self.id,
            self.thinking_levels.join(", ")
        );
        Ok(Some(level.to_owned()))
    }
}

/// The ChatGPT backend lists models by the Codex client version asking; ErisReader asks as a
/// current client.
const CODEX_CLIENT_VERSION: &str = "99.0.0";
/// A listing older than this is fetched again when it is next needed.
const FRESH_FOR: Duration = Duration::from_secs(3600);
/// Efforts Claude models may accept, in order; each model's capabilities say which it does.
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// The thinking budget for each level of a Claude model that thinks on a budget.
pub const CLAUDE_BUDGETS: [(&str, u64); 4] = [("low", 1024), ("medium", 4096), ("high", 16000), ("max", 31999)];

/// An account that is signed in, as the catalog lists its models.
enum Source {
    OpenAi(Credential),
    Claude(ClaudeLogin),
}

impl Source {
    /// Which account the models belong to: its provider and, where known, its account id.
    fn key(&self) -> String {
        match self {
            Self::OpenAi(credential) => format!("{CODEX}:{}", credential.account_id),
            Self::Claude(login) => match &login.account_id {
                Some(account) => format!("{ANTHROPIC}:{account}"),
                None => ANTHROPIC.to_owned(),
            },
        }
    }
}

/// The models the signed-in accounts offer, fetched from their providers and kept on disk so
/// a provider outage does not stop annotations from starting.
#[derive(Clone)]
pub struct Catalog(Arc<CatalogInner>);

struct CatalogInner {
    accounts: Accounts,
    endpoints: Endpoints,
    client: reqwest::Client,
    path: PathBuf,
    listings: tokio::sync::Mutex<Vec<Listing>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Listing {
    account: String,
    fetched_at: u64,
    models: Vec<Model>,
}

impl Catalog {
    pub fn new(accounts: Accounts, endpoints: Endpoints, path: PathBuf) -> Self {
        let listings =
            std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Self(Arc::new(CatalogInner {
            accounts,
            endpoints,
            client: reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("HTTP client"),
            path,
            listings: tokio::sync::Mutex::new(listings),
        }))
    }

    /// Every signed-in account's models: ChatGPT's, then Claude's, each newest or most
    /// preferred first. An account whose models cannot be listed is left out while another's
    /// can.
    pub async fn models(&self) -> Result<Vec<Model>> {
        let mut models = Vec::new();
        let mut failure = None;
        for provider in [CODEX, ANTHROPIC] {
            let source = match self.source(provider).await {
                Ok(Some(source)) => source,
                Ok(None) => continue,
                Err(error) => {
                    failure.get_or_insert(error);
                    continue;
                }
            };
            match self.listing(&source, false).await {
                Ok(listed) => models.extend(listed),
                Err(error) => {
                    eprintln!("erisreader: {error:#}");
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) if models.is_empty() => Err(error),
            _ => Ok(models),
        }
    }

    /// A model a signed-in account offers. A model the cached listing lacks is looked up
    /// again, so models the provider adds are usable without restarting.
    pub async fn find(&self, provider: &str, id: &str) -> Result<Model> {
        let source = self.source(provider).await?.ok_or_else(|| match provider {
            ANTHROPIC => anyhow!("Add a Claude login in Settings to use Claude models."),
            _ => anyhow!("Sign in with ChatGPT in Settings to use its models."),
        })?;
        let found = |models: Vec<Model>| models.into_iter().find(|m| m.id == id);
        if let Some(model) = found(self.listing(&source, false).await?) {
            return Ok(model);
        }
        found(self.listing(&source, true).await?).ok_or_else(|| anyhow!("The signed-in account has no model {id}."))
    }

    /// The signed-in account serving `provider`'s models, or `None` when it is signed out.
    async fn source(&self, provider: &str) -> Result<Option<Source>> {
        let accounts = &self.0.accounts;
        Ok(match provider {
            CODEX if accounts.openai.signed_in().await => Some(Source::OpenAi(accounts.openai.current().await?)),
            ANTHROPIC if accounts.claude.signed_in().await => Some(Source::Claude(accounts.claude.current().await?)),
            CODEX | ANTHROPIC => None,
            _ => bail!("unknown model provider {provider}; ErisReader has {CODEX} and {ANTHROPIC}"),
        })
    }

    async fn listing(&self, source: &Source, refresh: bool) -> Result<Vec<Model>> {
        let account = source.key();
        let mut listings = self.0.listings.lock().await;
        let cached = listings.iter().find(|l| l.account == account).cloned();
        if let Some(cached) = &cached
            && !refresh
            && now().saturating_sub(cached.fetched_at) < FRESH_FOR.as_millis() as u64
        {
            return Ok(cached.models.clone());
        }
        match self.fetch(source).await {
            Ok(models) => {
                // Listings of other logins to the same provider are stale.
                let provider = account.split(':').next().unwrap_or_default().to_owned();
                listings.retain(|l| l.account.split(':').next() != Some(&provider));
                listings.push(Listing { account, fetched_at: now(), models: models.clone() });
                if let Err(error) = write_json(&self.0.path, &*listings) {
                    eprintln!("erisreader: saving the model list: {error:#}");
                }
                Ok(models)
            }
            Err(error) => match cached {
                Some(cached) => {
                    eprintln!("erisreader: using the saved model list: {error:#}");
                    Ok(cached.models)
                }
                None => Err(error.context("listing the account's models")),
            },
        }
    }

    async fn fetch(&self, source: &Source) -> Result<Vec<Model>> {
        let client = &self.0.client;
        let endpoints = &self.0.endpoints;
        let get = |request: reqwest::RequestBuilder| async move {
            let response = request.send().await?;
            let status = response.status();
            ensure!(status.is_success(), "the provider answered {status}");
            Ok::<Value, anyhow::Error>(response.json().await?)
        };
        Ok(match source {
            Source::OpenAi(credential) => codex_models(
                &get(client
                    .get(format!(
                        "{}/models?client_version={CODEX_CLIENT_VERSION}",
                        endpoints.codex.trim_end_matches('/')
                    ))
                    .bearer_auth(&credential.access)
                    .header("chatgpt-account-id", &credential.account_id)
                    .header("originator", ORIGINATOR))
                .await?,
            ),
            Source::Claude(login) => {
                let mut models = Vec::new();
                let mut after: Option<String> = None;
                loop {
                    let mut request = client
                        .get(format!("{}/models", endpoints.anthropic_api()))
                        .query(&[("limit", "1000")])
                        .bearer_auth(&login.access)
                        .header("anthropic-version", "2023-06-01")
                        .header("anthropic-beta", OAUTH_BETA);
                    if let Some(after) = &after {
                        request = request.query(&[("after_id", after)]);
                    }
                    let page = get(request).await?;
                    models.extend(claude_models(&page));
                    match page["last_id"].as_str() {
                        Some(last) if page["has_more"] == true => after = Some(last.into()),
                        _ => break models,
                    }
                }
            }
        })
    }
}

/// The ChatGPT models an account may pick, in the backend's order of preference.
fn codex_models(body: &Value) -> Vec<Model> {
    body["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| {
            Some(Model {
                provider: CODEX.into(),
                id: m["slug"].as_str()?.to_owned(),
                name: m["display_name"].as_str().or(m["slug"].as_str())?.to_owned(),
                context_window: m["context_window"].as_u64(),
                max_output: None,
                thinking_levels: m["supported_reasoning_levels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l["effort"].as_str().map(str::to_owned))
                    .collect(),
                thinking: Thinking::Effort,
            })
        })
        .collect()
}

/// The Claude models a login may use, newest first, each thinking as its capabilities say:
/// adaptively at the efforts it lists, or else within a budget.
fn claude_models(body: &Value) -> Vec<Model> {
    let supported = |capability: &Value| capability["supported"] == true;
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let capabilities = &m["capabilities"];
            let kinds = &capabilities["thinking"]["types"];
            let budget = supported(&kinds["enabled"]);
            let (thinking, levels): (_, Vec<&str>) = if supported(&kinds["adaptive"]) {
                let effort = &capabilities["effort"];
                (
                    Thinking::Adaptive { summarized: !budget },
                    CLAUDE_EFFORTS
                        .into_iter()
                        .filter(|level| supported(effort) && supported(&effort[*level]))
                        .collect(),
                )
            } else if budget {
                (Thinking::Budget, CLAUDE_BUDGETS.map(|(level, _)| level).to_vec())
            } else {
                (Thinking::Budget, Vec::new())
            };
            let id = m["id"].as_str()?;
            Some(Model {
                provider: ANTHROPIC.into(),
                id: id.to_owned(),
                name: m["display_name"].as_str().unwrap_or(id).to_owned(),
                context_window: m["max_input_tokens"].as_u64(),
                max_output: m["max_tokens"].as_u64(),
                thinking_levels: levels.into_iter().map(str::to_owned).collect(),
                thinking,
            })
        })
        .collect()
}

struct ChatGptCredentials(OpenAiAccount);

impl Credentials for ChatGptCredentials {
    fn authorize(&self) -> BoxFuture<'_, Result<Authorization>> {
        Box::pin(async move {
            let credential = self.0.current().await?;
            Ok(Authorization {
                token: credential.access,
                headers: vec![("chatgpt-account-id".to_owned(), credential.account_id)],
            })
        })
    }
}

/// Endpoints for each provider. Tests point them at a local server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub codex: String,
    /// The Anthropic API's root: Messages and models under `/v1`, a login's profile under
    /// `/api`.
    pub anthropic: String,
}

impl Endpoints {
    pub fn anthropic_api(&self) -> String {
        format!("{}/v1", self.anthropic.trim_end_matches('/'))
    }
}

/// How many model calls each account runs at once. A rate limit halves it for a while.
const CALL_SLOTS: usize = 16;

/// The harness's providers, by the names model selections use.
pub fn providers(
    accounts: &Accounts,
    catalog: &Catalog,
    endpoints: &Endpoints,
) -> Vec<(&'static str, Arc<dyn Provider>)> {
    let codex = Responses::new(ResponsesConfig {
        base_url: endpoints.codex.clone(),
        headers: vec![
            ("OpenAI-Beta".into(), "responses=experimental".into()),
            ("originator".into(), ORIGINATOR.into()),
        ],
        store: false,
        max_retries: 5,
        credentials: Arc::new(ChatGptCredentials(accounts.openai.clone())),
        gate: RateGate::new(CALL_SLOTS, CALL_SLOTS),
    });
    let claude = ClaudeModels::new(
        accounts.claude.clone(),
        catalog.clone(),
        endpoints.anthropic_api(),
        RateGate::new(CALL_SLOTS, CALL_SLOTS),
    );
    vec![(CODEX, Arc::new(codex)), (ANTHROPIC, Arc::new(claude))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chatgpt_accounts_offer_the_models_the_backend_lists() {
        let body = json!({"models": [
            {"slug": "gpt-6-luna", "display_name": "GPT-6 Luna", "visibility": "list",
             "context_window": 272000, "supported_reasoning_levels": [
                {"effort": "low"}, {"effort": "high"}, {"effort": "ultra"}]},
            {"slug": "codex-auto-review", "display_name": "Codex Auto Review", "visibility": "hide",
             "context_window": 272000, "supported_reasoning_levels": [{"effort": "low"}]},
            {"slug": "gpt-5.5", "visibility": "list", "supported_reasoning_levels": []},
        ]});
        let models = codex_models(&body);
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["gpt-6-luna", "gpt-5.5"]);
        let luna = &models[0];
        assert_eq!((luna.provider.as_str(), luna.name.as_str()), (CODEX, "GPT-6 Luna"));
        assert_eq!(luna.context_window, Some(272000));
        assert_eq!(luna.effort("ultra").unwrap().as_deref(), Some("ultra"));
        assert_eq!(luna.effort("off").unwrap(), None);
        assert!(luna.effort("medium").is_err());
        assert_eq!(models[1].name, "gpt-5.5");
    }

    #[test]
    fn claude_models_think_as_their_capabilities_say() {
        let model = |id: &str, adaptive: bool, enabled: bool, efforts: Value| {
            json!({"id": id, "display_name": id.to_uppercase(), "max_input_tokens": 200000,
                "max_tokens": 64000, "capabilities": {"effort": efforts,
                "thinking": {"supported": true, "types": {
                    "adaptive": {"supported": adaptive}, "enabled": {"supported": enabled}}}}})
        };
        let some = json!({"supported": true, "low": {"supported": true}, "medium": {"supported": false},
            "high": {"supported": true}, "xhigh": {"supported": false}, "max": {"supported": true}});
        let models = claude_models(&json!({"data": [
            model("adaptive", true, false, some.clone()),
            model("both", true, true, some),
            model("budget", false, true, json!({"supported": false})),
            model("plain", false, false, json!({"supported": false})),
        ]}));
        let shapes: Vec<_> = models.iter().map(|m| (m.id.as_str(), m.thinking, m.thinking_levels.join(" "))).collect();
        assert_eq!(
            shapes,
            [
                ("adaptive", Thinking::Adaptive { summarized: true }, "low high max".into()),
                ("both", Thinking::Adaptive { summarized: false }, "low high max".into()),
                ("budget", Thinking::Budget, "low medium high max".into()),
                ("plain", Thinking::Budget, String::new()),
            ]
        );
        let first = &models[0];
        assert_eq!(
            (first.provider.as_str(), first.name.as_str(), first.context_window, first.max_output),
            (ANTHROPIC, "ADAPTIVE", Some(200000), Some(64000))
        );
    }
}
