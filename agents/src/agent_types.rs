//! Agent types: Markdown files in the `agents` folder, declared exactly as ErisAE's
//! `.ae/agents/*.md` are. YAML frontmatter sets the name, tools and settings; everything below
//! its closing `---` is the type's system prompt. An annotation keeps the definition it was
//! started with.
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod files;
mod markdown;
#[cfg(test)]
mod tests;

pub use files::AgentFiles;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Read,
    Bash,
    Edit,
    Write,
    Desktop,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Display,
    Gpu,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permissions {
    pub filesystem: FileAccess,
    pub execute_commands: bool,
    pub spawn_agents: bool,
}
impl Default for Permissions {
    fn default() -> Self {
        Self { filesystem: FileAccess::ReadWrite, execute_commands: true, spawn_agents: true }
    }
}

fn writer() -> Vec<String> {
    vec!["writer".into()]
}

/// A model a type names: its provider (`openai-codex` for ChatGPT logins, `anthropic` for
/// Claude logins) and id.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ModelSelection {
    pub provider: String,
    pub id: String,
}

/// How hard an agent thinks: `off`, or a reasoning effort its model lists, such as `high` or
/// `ultra`. Which efforts exist is up to the model, so any short lowercase name parses.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ThinkingLevel(String);

impl ThinkingLevel {
    pub fn name(&self) -> &str {
        &self.0
    }

    pub fn parse(name: &str) -> Result<Self> {
        ensure!(
            (1..=32).contains(&name.len())
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "a thinking level is a short lowercase name such as off, low, high or xhigh, not {name:?}"
        );
        Ok(Self(name.to_owned()))
    }
}

impl<'de> Deserialize<'de> for ThinkingLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub system_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    pub tools: BTreeSet<Tool>,
    #[serde(default)]
    pub capabilities: BTreeSet<Capability>,
    pub permissions: Permissions,
    /// Roles agents of this type hold in an ErisAE forge project's repository.
    #[serde(default = "writer")]
    pub forge_roles: Vec<String>,
}
impl Default for Definition {
    /// The type a new `agents` folder starts with.
    fn default() -> Self {
        Self {
            name: "Reader".into(),
            description: "Helps you understand what you are reading.".into(),
            system_prompt: "You help a reader understand the document they are reading. Answer clearly and directly, \
grounded in the document: quote it where that helps, and say which pages you draw on. Use Markdown where it helps: \
lists, tables, and fenced code blocks with their language.\n"
                .into(),
            model: None,
            thinking_level: None,
            tools: BTreeSet::new(),
            capabilities: BTreeSet::new(),
            permissions: Permissions::default(),
            forge_roles: writer(),
        }
    }
}
impl Definition {
    pub fn validate(&self) -> Result<()> {
        ensure!(!(self.name.trim().is_empty() || self.name.len() > 128), "type name must contain 1 to 128 bytes");
        ensure!(
            !(self.description.len() > 4096 || self.system_prompt.len() > 65536),
            "type description or system prompt is too large"
        );
        ensure!(
            !self.model.as_ref().is_some_and(|model| model.id.trim().is_empty() || model.provider.trim().is_empty()),
            "model provider and id are required"
        );
        ensure!(
            !self.tools.contains(&Tool::Desktop) || self.capabilities.contains(&Capability::Display),
            "the desktop tool requires the display capability"
        );
        ensure!(
            !(self.capabilities.contains(&Capability::Display)
                && (!self.permissions.execute_commands || self.permissions.filesystem != FileAccess::ReadWrite)),
            "the display capability requires command execution and read-write filesystem permission"
        );
        ensure!(
            self.forge_roles.len() <= 16
                && self.forge_roles.iter().all(|role| {
                    !role.is_empty()
                        && role.len() <= 64
                        && role.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                }),
            "forge roles are up to 16 names of letters, numbers, underscores and hyphens"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentType {
    pub id: String,
    pub definition: Definition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTypes {
    pub default_type: String,
    pub types: BTreeMap<String, Definition>,
}
impl Default for AgentTypes {
    fn default() -> Self {
        Self { default_type: "default".into(), types: BTreeMap::from([("default".into(), Definition::default())]) }
    }
}
impl AgentTypes {
    pub fn validate(&self) -> Result<()> {
        ensure!(!(self.types.is_empty() || self.types.len() > 128), "define between 1 and 128 agent types");
        for (id, definition) in &self.types {
            ensure!(
                !(["__proto__", "prototype", "constructor"].contains(&id.as_str())
                    || id.is_empty()
                    || id.len() > 64
                    || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))),
                "invalid agent type ID {id:?}; use 1 to 64 letters, numbers, underscores, or hyphens"
            );
            definition.validate().map_err(|error| anyhow!("type {id:?}: {error}"))?;
        }
        self.resolve(None).map(|_| ())
    }

    pub fn resolve(&self, requested: Option<&str>) -> Result<AgentType> {
        let id = requested.unwrap_or(&self.default_type);
        let definition = self
            .types
            .get(id)
            .ok_or_else(|| anyhow!("unknown agent type {id:?}; choose one of the types in the agents folder"))?;
        Ok(AgentType { id: id.into(), definition: definition.clone() })
    }
}
