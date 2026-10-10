//! ErisReader's agents, run by its main process and spoken to over stdin and stdout (see
//! `bridge`). Each annotation of the open document is an ErisHarness agent on the signed-in
//! ChatGPT or Claude account, with the document as its only tool (see `annotations` and
//! `tools`). The data folder the main process names holds what is not the document's:
//!
//! ```text
//! agents/*.md            agent types, declared as ErisAE's .ae/agents/*.md
//! accounts/              the ChatGPT and Claude logins, readable only by the user
//! models.json            the accounts' models, as last listed
//! ```

mod accounts;
mod agent_types;
mod annotations;
mod bridge;
mod claude;
mod document;
mod models;
mod storage;
mod tools;

use accounts::{Accounts, ClaudeAccount, OpenAiAccount};
use agent_types::{AgentFiles, AgentTypes};
use annotations::Annotations;
use anyhow::{Context, Result, anyhow, bail};
use bridge::Bridge;
use document::DocumentText;
use models::{Catalog, Endpoints};
use serde_json::{Value, json};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

struct App {
    accounts: Accounts,
    catalog: Catalog,
    annotations: Annotations,
}

fn endpoint(variable: &str, default: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| default.into())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = match (args.next().as_deref(), args.next()) {
        (Some("--dir"), Some(dir)) => PathBuf::from(dir),
        _ => bail!("usage: erisreader-agents --dir <data folder>"),
    };
    std::fs::create_dir_all(&dir)?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir.join("accounts"))?;
    let bridge = Bridge::default();

    // ERISREADER_* point the helper at stand-ins, as the e2e tests do.
    let endpoints = Endpoints {
        codex: endpoint("ERISREADER_CODEX_API", "https://chatgpt.com/backend-api/codex"),
        anthropic: endpoint("ERISREADER_ANTHROPIC_API", "https://api.anthropic.com"),
    };
    let accounts = Accounts {
        openai: OpenAiAccount::load(
            dir.join("accounts/chatgpt.json"),
            endpoint("ERISREADER_OPENAI_AUTH", "https://auth.openai.com"),
            bridge.clone(),
        ),
        claude: ClaudeAccount::load(
            dir.join("accounts/claude.json"),
            &endpoints.anthropic,
            &endpoint("ERISREADER_CLAUDE_AUTH", "https://platform.claude.com"),
            bridge.clone(),
        )?,
    };
    let catalog = Catalog::new(accounts.clone(), endpoints.clone(), dir.join("models.json"));

    let files = AgentFiles::prepare(&dir.join("agents"))?;
    AgentTypes::prepare(&files)?;
    let providers = models::providers(&accounts, &catalog, &endpoints);
    let annotations = Annotations::new(providers, catalog.clone(), files, bridge.clone());
    let app = Arc::new(App { accounts, catalog, annotations });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("answer").is_some() {
            bridge.answer(message);
            continue;
        }
        let Some(id) = message["id"].as_u64() else {
            continue;
        };
        let (app, bridge) = (app.clone(), bridge.clone());
        tokio::spawn(async move {
            let result = handle(&app, &message).await;
            bridge.reply(id, result);
        });
    }
    // ErisReader has gone. Turns stop where they are, and resume when the document opens again.
    app.annotations.close().await;
    Ok(())
}

fn text<'a>(message: &'a Value, key: &str) -> Result<&'a str> {
    message[key].as_str().ok_or_else(|| anyhow!("missing {key}"))
}

async fn handle(app: &App, message: &Value) -> Result<Value> {
    let annotations = &app.annotations;
    match text(message, "command")? {
        "accounts" => Ok(json!(app.accounts.snapshots())),
        "login" => match text(message, "provider")? {
            "openai" => app.accounts.openai.login(text(message, "method")?).await,
            "anthropic" => match text(message, "method")? {
                "credentials-file" => app.accounts.claude.import(text(message, "text")?).await,
                "keychain" => app.accounts.claude.import_keychain().await,
                other => bail!("Claude has no sign-in method {other}"),
            },
            other => bail!("unknown account {other}"),
        },
        "cancel-login" => {
            app.accounts.openai.cancel().await;
            Ok(Value::Null)
        }
        "logout" => match text(message, "provider")? {
            "openai" => app.accounts.openai.logout().await.map(|()| Value::Null),
            "anthropic" => app.accounts.claude.logout().await.map(|()| Value::Null),
            other => bail!("unknown account {other}"),
        },
        "models" => Ok(serde_json::to_value(app.catalog.models().await?)?),
        "types" => annotations.types(),
        "open-document" => {
            let document: DocumentText =
                serde_json::from_value(json!({"name": message["name"], "pages": message["pages"]}))
                    .context("a malformed document")?;
            Ok(json!(annotations.open(text(message, "path")?, document).await?))
        }
        "close-document" => {
            annotations.close().await;
            Ok(Value::Null)
        }
        "annotations" => Ok(json!(annotations.list().await?)),
        "annotation" => annotations.get(text(message, "annotation")?).await,
        "create" => annotations.create(message).await,
        "send" => annotations.send(text(message, "annotation")?, message).await.map(|()| Value::Null),
        "stop" => annotations.stop(text(message, "annotation")?).await.map(|()| Value::Null),
        "delete" => annotations.delete(text(message, "annotation")?).await.map(|()| Value::Null),
        "configure" => annotations.configure(text(message, "annotation")?, message).await,
        other => Err(anyhow!("unknown command {other}")),
    }
}
