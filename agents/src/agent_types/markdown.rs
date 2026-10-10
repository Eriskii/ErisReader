use super::*;

const FILE_LIMIT: u64 = 128 * 1024;
const TOTAL_LIMIT: usize = 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Frontmatter {
    name: String,
    #[serde(default, alias = "desc")]
    description: String,
    #[serde(deserialize_with = "tools", alias = "allowed-tools")]
    tools: BTreeSet<Tool>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    capabilities: BTreeSet<Capability>,
    #[serde(default)]
    permissions: Permissions,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_level: Option<ThinkingLevel>,
    #[serde(default = "writer")]
    forge_roles: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    default: bool,
}

fn tools<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeSet<Tool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        List(Vec<String>),
        CommaSeparated(String),
    }
    let names = match Input::deserialize(deserializer)? {
        Input::List(names) => names,
        Input::CommaSeparated(names) => names.split(',').map(str::to_owned).collect(),
    };
    names
        .into_iter()
        .map(|name| {
            serde_json::from_value(serde_json::Value::String(name.trim().to_ascii_lowercase()))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

fn parse(bytes: &[u8]) -> Result<(Definition, bool)> {
    let text = std::str::from_utf8(bytes)?.trim_start_matches('\u{feff}');
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or_default();
    ensure!(first.trim_end() == "---", "expected YAML frontmatter starting with ---");
    let mut end = first.len();
    for line in lines {
        if line.trim_end() == "---" {
            let header: Frontmatter = serde_yaml_ng::from_str(&text[first.len()..end])?;
            let definition = Definition {
                name: header.name,
                description: header.description,
                system_prompt: text[end + line.len()..].into(),
                model: header.model,
                thinking_level: header.thinking_level,
                tools: header.tools,
                capabilities: header.capabilities,
                permissions: header.permissions,
                forge_roles: header.forge_roles,
            };
            definition.validate()?;
            return Ok((definition, header.default));
        }
        end += line.len();
    }
    Err(anyhow!("YAML frontmatter is missing its closing ---"))
}

fn render(definition: &Definition, default: bool) -> Result<String> {
    let header = Frontmatter {
        name: definition.name.clone(),
        description: definition.description.clone(),
        tools: definition.tools.clone(),
        capabilities: definition.capabilities.clone(),
        permissions: definition.permissions.clone(),
        model: definition.model.clone(),
        thinking_level: definition.thinking_level.clone(),
        forge_roles: definition.forge_roles.clone(),
        default,
    };
    Ok(format!("---\n{}---\n{}", serde_yaml_ng::to_string(&header)?, definition.system_prompt))
}

impl AgentTypes {
    /// A folder without definitions receives the built-in default type once.
    pub fn prepare(files: &AgentFiles) -> Result<()> {
        if files.names_with_suffix(".md", 128)?.is_empty() {
            files.write_if_missing("default.md", render(&Definition::default(), false)?.as_bytes())?;
        }
        Ok(())
    }

    pub fn load(files: &AgentFiles) -> Result<Self> {
        let mut types = BTreeMap::new();
        let mut default = None;
        let mut size = 0;
        for name in files.names_with_suffix(".md", 128)? {
            let loaded = (|| -> Result<(Definition, bool)> {
                let bytes =
                    files.read_limit(&name, FILE_LIMIT)?.ok_or(anyhow!("definition was removed while loading"))?;
                size += bytes.len();
                ensure!(size <= TOTAL_LIMIT, "agent definitions exceed 1 MiB in total");
                parse(&bytes)
            })();
            let (definition, selected) = loaded.map_err(|error| anyhow!("invalid agents/{name}: {error}"))?;
            let id = name.strip_suffix(".md").unwrap().to_owned();
            if selected {
                ensure!(
                    default.is_none(),
                    "only one agent definition may set default: true; conflicting file agents/{name}"
                );
                default = Some(id.clone());
            }
            types.insert(id, definition);
        }
        ensure!(
            default.is_some() || types.contains_key("default"),
            "no default agent definition in the agents folder; provide default.md or mark one definition default: true"
        );
        let result = Self { default_type: default.unwrap_or_else(|| "default".into()), types };
        result.validate().map_err(|error| anyhow!("invalid agents folder: {error}"))?;
        Ok(result)
    }
}
