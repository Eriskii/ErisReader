//! Claude models through a Claude login, as ErisAE reaches them. ErisHarness's Anthropic
//! provider thinks one way for every request, and Claude models differ: newer ones think
//! adaptively at an effort, older ones within a budget. This provider looks each request's
//! model up in the catalog and hands the request to an Anthropic provider set up for that model
//! and level. Every one of them makes Claude Code's requests with the account's current login
//! and shares one rate gate.
use crate::accounts::ClaudeAccount;
use crate::models::{ANTHROPIC, CLAUDE_BUDGETS, Catalog, Thinking};
use anyhow::{Result, anyhow};
use erisharness::provider::{
    self, Anthropic, AnthropicAuth, AnthropicConfig, Authorization, ClaudeCode, Completion, Credentials, Progress,
    Provider, RateGate, Request,
};
use futures_util::future::BoxFuture;
use std::sync::{Arc, Mutex};

/// Subscriptions write at most this much per reply.
const MAX_OUTPUT: u64 = 64000;

pub struct ClaudeModels {
    account: ClaudeAccount,
    catalog: Catalog,
    /// The Anthropic API's `/v1`.
    base_url: String,
    gate: Arc<RateGate>,
    /// Providers made so far, by what sets them apart.
    providers: Mutex<Vec<(Setup, Arc<Anthropic>)>>,
}

/// What an Anthropic provider is set up with besides the account's credentials and gate.
#[derive(Clone, Debug, PartialEq)]
struct Setup {
    /// The Claude account requests name; a new login may belong to another.
    account: Option<String>,
    thinking: provider::Thinking,
    max_tokens: u64,
}

impl ClaudeModels {
    pub fn new(account: ClaudeAccount, catalog: Catalog, base_url: String, gate: Arc<RateGate>) -> Self {
        Self { account, catalog, base_url, gate, providers: Mutex::default() }
    }

    fn provider(&self, setup: Setup) -> Result<Arc<Anthropic>> {
        let mut providers = self.providers.lock().unwrap();
        providers.retain(|(made, _)| made.account == setup.account);
        if let Some((_, provider)) = providers.iter().find(|(made, _)| *made == setup) {
            return Ok(provider.clone());
        }
        let identity = ClaudeCode {
            install_id: self.account.install_id().to_owned(),
            account_id: setup.account.clone(),
            version: None,
        };
        let mut config =
            AnthropicConfig::new(Arc::new(Login(self.account.clone())), AnthropicAuth::ClaudeCode(identity));
        config.base_url = self.base_url.clone();
        config.gate = self.gate.clone();
        config.max_retries = 5;
        config.thinking = setup.thinking;
        config.max_tokens = setup.max_tokens;
        let provider = Arc::new(Anthropic::new(config)?);
        providers.push((setup, provider.clone()));
        Ok(provider)
    }
}

/// How a request at `level` thinks on a model that thinks as `thinking` says. No level leaves
/// out thinking, so the model thinks as it does by default.
fn thinking(thinking: Thinking, level: Option<&str>, max_tokens: u64) -> Result<provider::Thinking> {
    Ok(match (level, thinking) {
        (None, _) => provider::Thinking::Disabled,
        (Some(_), Thinking::Adaptive { summarized }) => provider::Thinking::Adaptive { display: summarized },
        (Some(level), Thinking::Budget) => {
            let (_, tokens) = CLAUDE_BUDGETS
                .into_iter()
                .find(|(name, _)| *name == level)
                .ok_or_else(|| anyhow!("Claude thinking budgets have no level {level}"))?;
            provider::Thinking::Budget { tokens: tokens.min(max_tokens - 1), display: false }
        }
        (Some(level), Thinking::Effort) => return Err(anyhow!("Claude models do not take a {level} reasoning effort")),
    })
}

impl Provider for ClaudeModels {
    fn complete<'a>(
        &'a self,
        request: Request<'a>,
        progress: &'a (dyn Fn(Progress) + Send + Sync),
    ) -> BoxFuture<'a, Result<Completion>> {
        Box::pin(async move {
            let login = self.account.current().await?;
            let model = self.catalog.find(ANTHROPIC, request.model).await?;
            let max_tokens = model.max_output.unwrap_or(MAX_OUTPUT).min(MAX_OUTPUT);
            let provider = self.provider(Setup {
                account: login.account_id,
                thinking: thinking(model.thinking, request.reasoning_effort, max_tokens)?,
                max_tokens,
            })?;
            provider.complete(request, progress).await
        })
    }
}

/// Authorizes each request with the account's login, refreshed when it is due.
struct Login(ClaudeAccount);

impl Credentials for Login {
    fn authorize(&self) -> BoxFuture<'_, Result<Authorization>> {
        Box::pin(async move { Ok(Authorization { token: self.0.current().await?.access, headers: Vec::new() }) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_claude_model_thinks_its_own_way() {
        let adaptive = Thinking::Adaptive { summarized: true };
        assert_eq!(
            thinking(adaptive, Some("xhigh"), MAX_OUTPUT).unwrap(),
            provider::Thinking::Adaptive { display: true }
        );
        assert_eq!(thinking(adaptive, None, MAX_OUTPUT).unwrap(), provider::Thinking::Disabled);
        assert_eq!(
            thinking(Thinking::Budget, Some("high"), MAX_OUTPUT).unwrap(),
            provider::Thinking::Budget { tokens: 16000, display: false }
        );
        assert_eq!(
            thinking(Thinking::Budget, Some("max"), 32000).unwrap(),
            provider::Thinking::Budget { tokens: 31999, display: false },
            "a budget stays below the reply's ceiling"
        );
        assert!(thinking(Thinking::Budget, Some("xhigh"), MAX_OUTPUT).is_err());
        assert!(thinking(Thinking::Effort, Some("high"), MAX_OUTPUT).is_err());
    }
}
