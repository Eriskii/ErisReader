//! The accounts chats make model calls with: a ChatGPT login and a Claude login, each optional.
//! Chats can use the models of any account that is signed in.
pub mod claude;
pub mod openai;

pub use claude::{ClaudeAccount, ClaudeLogin};
pub use openai::{Credential, OpenAiAccount};

use serde_json::Value;

#[derive(Clone)]
pub struct Accounts {
    pub openai: OpenAiAccount,
    pub claude: ClaudeAccount,
}

impl Accounts {
    /// Each account as ErisDE shows an ErisAE account: provider, status, who is signed in, and
    /// how to sign in.
    pub fn snapshots(&self) -> Vec<Value> {
        vec![self.openai.snapshot(), self.claude.snapshot()]
    }
}
