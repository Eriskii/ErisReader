//! ErisAE's agent type tests, run against ErisReader's folder: both read the same files.
use super::*;
use std::{fs, path::PathBuf};

struct Folder(PathBuf, AgentFiles);
impl Folder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("erisreader-types-{}", uuid::Uuid::new_v4()));
        let files = AgentFiles::prepare(&path.join("agents")).unwrap();
        Self(path, files)
    }
    fn write(&self, name: &str, content: &str) {
        self.1.write(name, content.as_bytes()).unwrap();
    }
    fn load(&self) -> Result<AgentTypes> {
        AgentTypes::load(&self.1)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_new_folder_gets_a_default_type_once() {
    let folder = Folder::new();
    AgentTypes::prepare(&folder.1).unwrap();
    let types = folder.load().unwrap();
    assert_eq!(types.default_type, "default");
    let default = &types.types["default"];
    assert_eq!(*default, Definition::default());
    assert!(default.tools.is_empty());
    folder.write("default.md", "---\nname: Mine\ntools: []\n---\nMy prompt\n");
    AgentTypes::prepare(&folder.1).unwrap();
    assert_eq!(folder.load().unwrap().types["default"].name, "Mine");
}

#[test]
fn markdown_definitions_load_metadata_and_preserve_the_prompt_body() {
    let folder = Folder::new();
    AgentTypes::prepare(&folder.1).unwrap();
    let prompt = "\n# Review carefully\n\nKeep Unicode: café.\n---\n```yaml\nname: prompt text\n```\n";
    folder.write("reviewer.md", &format!("---\nname: Code reviewer\ndescription: >-\n  Review changes\n  without writing.\ntools: Read, Bash, Edit\ndefault: true\nmodel:\n  provider: openai\n  id: test-model\nthinkingLevel: high\nforgeRoles: [reviewer]\npermissions:\n  filesystem: read-only\n  executeCommands: false\n  spawnAgents: false\n---\n{prompt}"));
    let types = folder.load().unwrap();
    assert_eq!(types.default_type, "reviewer");
    let agent = types.resolve(None).unwrap();
    assert_eq!(agent.definition.name, "Code reviewer");
    assert_eq!(agent.definition.description, "Review changes without writing.");
    assert_eq!(agent.definition.system_prompt, prompt);
    assert_eq!(agent.definition.tools, BTreeSet::from([Tool::Read, Tool::Bash, Tool::Edit]));
    assert_eq!(agent.definition.model.unwrap().id, "test-model");
    assert_eq!(agent.definition.thinking_level.unwrap().name(), "high");
    assert_eq!(agent.definition.forge_roles, ["reviewer"]);
    assert!(!agent.definition.permissions.spawn_agents);
    // Discovery only reads top-level Markdown.
    fs::create_dir(folder.1.path().join("saved")).unwrap();
    fs::write(folder.1.path().join("saved/nested.md"), "not a definition").unwrap();
    folder.write("catalog.json", "{}");
    assert_eq!(folder.load().unwrap().types.len(), 2);
    folder.write("reviewer.md", "---\r\nname: Reviewer\r\ndesc: Short description\r\nallowed-tools: [read, write]\r\ndefault: true\r\n---\r\nExact prompt\r\n");
    let agent = folder.load().unwrap().resolve(None).unwrap();
    assert_eq!(agent.definition.system_prompt, "Exact prompt\r\n");
    assert_eq!(agent.definition.tools, BTreeSet::from([Tool::Read, Tool::Write]));
    assert_eq!(agent.definition.forge_roles, ["writer"]);
}

#[test]
fn malformed_or_ambiguous_definitions_fail_without_fallback() {
    let folder = Folder::new();
    for content in [
        "No frontmatter",
        "---\nname: Missing close\ntools: []\n",
        "---\nname: Missing tools\n---\n",
        "---\nname: Unknown tool\ntools: [unknown]\n---\n",
        "---\nname: Blank tool\ntools: read,\n---\n",
        "---\nname: Typo\ntools: []\ntoolz: []\n---\n",
        "---\nname: Duplicate\nname: Ambiguous\ntools: []\n---\n",
        "---\nname: Desktop\ntools: [desktop]\n---\n",
        "---\nname: Model\ntools: []\nmodel: {id: missing-provider}\n---\n",
        "---\nname: \"\"\ntools: []\n---\n",
        "---\nname: Permission typo\ntools: []\npermissions: {filesystem: unrestricted}\n---\n",
        "---\nname: Old tool\ntools: [git]\n---\n",
        "---\nname: Old permission\ntools: []\npermissions: {filesystem: read-write, executeCommands: true, spawnAgents: true, git: write}\n---\n",
        "---\nname: Bad role\ntools: []\nforgeRoles: [\"no spaces\"]\n---\n",
        "---\nname: Bad level\ntools: []\nthinkingLevel: High\n---\n",
    ] {
        folder.write("default.md", content);
        let error = folder.load().unwrap_err().to_string();
        assert!(error.contains("agents/default.md"), "{error}");
    }
    folder.write("default.md", "---\nname: No tools\ntools: []\n---\n");
    let types = folder.load().unwrap();
    assert!(types.types["default"].tools.is_empty());
    assert!(types.types["default"].system_prompt.is_empty());
    folder.write("a.md", "---\nname: A\ntools: []\ndefault: true\n---\n");
    folder.write("b.md", "---\nname: B\ntools: []\ndefault: true\n---\n");
    assert!(folder.load().unwrap_err().to_string().contains("only one"));
    fs::remove_file(folder.1.path().join("b.md")).unwrap();
    assert!(folder.1.write("../escape.md", b"unused").is_err());
    folder.write("bad.name.md", "---\nname: Bad ID\ntools: []\n---\n");
    assert!(folder.load().unwrap_err().to_string().contains("invalid agent type ID"));
    fs::remove_file(folder.1.path().join("bad.name.md")).unwrap();
    fs::remove_file(folder.1.path().join("default.md")).unwrap();
    folder.write("a.md", "---\nname: A\ntools: []\n---\n");
    assert!(folder.load().unwrap_err().to_string().contains("no default"));
}

#[test]
fn markdown_loading_keeps_file_boundaries_and_size_limits() {
    use std::os::unix::fs::symlink;
    let folder = Folder::new();
    AgentTypes::prepare(&folder.1).unwrap();
    let path = folder.1.path().join("default.md");
    let outside = folder.0.join("outside.md");
    fs::write(&outside, "---\nname: Outside\ntools: []\n---\n").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(folder.load().is_err());
    assert!(!folder.1.write_if_missing("default.md", b"unused").unwrap(), "a symlink keeps its name");
    fs::remove_file(&path).unwrap();
    fs::hard_link(&outside, &path).unwrap();
    assert!(folder.load().unwrap_err().to_string().contains("singly linked"));
    fs::remove_file(&path).unwrap();
    let header = "---\nname: Large\ntools: []\n---\n";
    folder.write("default.md", &format!("{header}{}", "x".repeat(65537)));
    assert!(folder.load().unwrap_err().to_string().contains("too large"));
    folder.write("default.md", &"x".repeat(128 * 1024 + 1));
    assert!(folder.load().unwrap_err().to_string().contains("exceeds 128 KiB"));
    folder.write("default.md", &format!("{header}{}", "x".repeat(65536)));
    assert!(folder.load().is_ok());
    for i in 0..16 {
        folder.write(&format!("agent-{i}.md"), &format!("{header}{}", "x".repeat(65536)));
    }
    assert!(folder.load().unwrap_err().to_string().contains("1 MiB"));
    for i in 16..128 {
        folder.write(&format!("agent-{i}.md"), header);
    }
    assert!(folder.load().unwrap_err().to_string().contains("maximum is 128"));
}

#[test]
fn thinking_levels_are_whatever_short_names_models_use() {
    for name in ["off", "xhigh", "ultra", "max-2"] {
        assert_eq!(ThinkingLevel::parse(name).unwrap().name(), name);
    }
    for name in ["", "High", "a b", "2x", &"x".repeat(33)] {
        assert!(ThinkingLevel::parse(name).is_err(), "{name:?}");
    }
}
