//! Annotations: each one a passage or region of the open document, and an ErisHarness agent
//! that talks about it with the reader. They live in the document's sidecar, a folder beside it
//! named for it (`paper.pdf.erisreader/`): the annotations' records and mail in `harness.db`,
//! their conversations in `chats/<id>/transcript.jsonl`. The first annotation makes the
//! sidecar, and the last one deleted takes it away, so reading a document leaves nothing beside
//! it.
//!
//! An annotation's agent runs directly on this computer with the document tools alone (see
//! `tools`). Its record keeps how it runs and what ErisReader knows of it: its title, where it
//! is in the document, its agent type and thinking level.
use crate::agent_types::{AgentFiles, AgentTypes, ThinkingLevel};
use crate::bridge::Bridge;
use crate::document::DocumentText;
use crate::models::{Catalog, Model};
use crate::storage;
use crate::tools;
use anyhow::{Context, Result, anyhow, bail, ensure};
use erisharness::Harness;
use erisharness::agent::{AgentRecord, AgentSpec, AgentState, Entry, Image, Item, Observation};
use erisharness::machine::{DirectSpec, MachineSpec};
use erisharness::provider::{Hold, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use tokio::sync::{Mutex, broadcast};

/// A rectangle on a page: the page's index from 0, and fractions of its width and height
/// from its top-left corner.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PageRect {
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where an annotation is in its document.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Anchor {
    /// A passage of selected text: its lines' rectangles, and the text.
    Text { rects: Vec<PageRect>, text: String },
    /// A region drawn over a page, and the text found inside it.
    Region { rect: PageRect, text: String },
}

/// The longest passage an annotation keeps.
const PASSAGE_LIMIT: usize = 100_000;
/// The largest image a region's first message carries.
const IMAGE_LIMIT: usize = 12 * 1024 * 1024;

impl Anchor {
    fn rects(&self) -> &[PageRect] {
        match self {
            Self::Text { rects, .. } => rects,
            Self::Region { rect, .. } => std::slice::from_ref(rect),
        }
    }

    /// Checks the anchor lies in a document of `pages` pages.
    fn check(&self, pages: usize) -> Result<()> {
        ensure!(!self.rects().is_empty(), "An annotation needs somewhere on a page.");
        let unit = |v: f64| v.is_finite() && (-0.001..=1.001).contains(&v);
        for rect in self.rects() {
            ensure!(rect.page < pages, "The document has no page {}.", rect.page + 1);
            ensure!(
                unit(rect.x) && unit(rect.y) && unit(rect.x + rect.width) && unit(rect.y + rect.height),
                "An annotation's rectangle lies outside its page."
            );
            ensure!(rect.width > 0.0 && rect.height > 0.0, "An annotation's rectangle is empty.");
        }
        match self {
            Self::Text { text, .. } => ensure!(!text.trim().is_empty(), "Select some text to ask about."),
            Self::Region { .. } => {}
        }
        let text = match self {
            Self::Text { text, .. } | Self::Region { text, .. } => text,
        };
        ensure!(text.len() <= PASSAGE_LIMIT, "That passage is too long to annotate; select less of it.");
        Ok(())
    }

    /// The first page it is on, counting from 1, and the last.
    fn pages(&self) -> (usize, usize) {
        let pages = self.rects().iter().map(|r| r.page + 1);
        (pages.clone().min().unwrap_or(1), pages.max().unwrap_or(1))
    }
}

/// What ErisReader keeps with each annotation's agent, as its metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationInfo {
    title: String,
    anchor: Anchor,
    agent_type: TypeRef,
    /// The thinking level chosen: `off`, or one of the model's.
    thinking: String,
    /// Unix milliseconds.
    created: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TypeRef {
    id: String,
    name: String,
}

/// The longest title an annotation gets from its first message.
const TITLE_LENGTH: usize = 64;

/// The sidecar beside a document: its path with `.erisreader` after it.
pub fn sidecar(document: &Path) -> PathBuf {
    let mut path = document.as_os_str().to_owned();
    path.push(".erisreader");
    PathBuf::from(path)
}

/// The system prompt of an annotation's agent: its type's, then the document and the part of
/// it the annotation marks.
fn instructions(type_prompt: &str, document: &DocumentText, anchor: &Anchor) -> String {
    let pages = document.page_count();
    let mut prompt = format!(
        "{}\n\nThe reader is reading \"{}\", a PDF of {pages} page{}, and has marked part of it to talk about with you. \
The document is the only thing you can see: read its text with read_pages, find words in it with search, and look at \
a page with view_page when its layout, figures, tables or equations matter. Pages count from 1, in order; read_pages \
gives the document's own label where a page has one.\n\n",
        type_prompt.trim_end(),
        document.name,
        if pages == 1 { "" } else { "s" },
    );
    let (first, last) = anchor.pages();
    let place = |number: usize| match document.label(number) {
        Some(label) => format!("page {number} (labelled \"{label}\")"),
        None => format!("page {number}"),
    };
    match anchor {
        Anchor::Text { text, .. } => {
            let place = if first == last { place(first) } else { format!("{} to {}", place(first), place(last)) };
            prompt.push_str(&format!("The annotation marks this passage on {place}:\n\n<passage>\n{}\n</passage>\n", text.trim()));
        }
        Anchor::Region { text, .. } => {
            prompt.push_str(&format!(
                "The annotation marks a region of {}, which comes as an image with the reader's first message.",
                place(first)
            ));
            if text.trim().is_empty() {
                prompt.push_str(" The PDF has no text inside the region.\n");
            } else {
                prompt.push_str(&format!(
                    " The PDF's text inside the region:\n\n<region-text>\n{}\n</region-text>\n",
                    text.trim()
                ));
            }
        }
    }
    prompt
}

/// The document open in ErisReader, and its annotations' harness once it has a sidecar.
struct OpenDocument {
    path: String,
    text: Arc<DocumentText>,
    harness: Option<Arc<Harness>>,
    forwarding: Option<tokio::task::JoinHandle<()>>,
}

impl OpenDocument {
    fn sidecar(&self) -> PathBuf {
        sidecar(Path::new(&self.path))
    }

    /// Stops its agents where they are, to resume when it opens again.
    async fn close(&mut self) {
        if let Some(forwarding) = self.forwarding.take() {
            forwarding.abort();
        }
        if let Some(harness) = self.harness.take() {
            harness.shutdown().await;
        }
    }

    /// Closes the harness and takes the sidecar away. Anything else put there stays, and keeps
    /// the folder.
    async fn retire(&mut self) -> Result<()> {
        self.close().await;
        let dir = self.sidecar();
        for name in ["harness.db", "harness.db-wal", "harness.db-shm"] {
            storage::remove(&dir.join(name))?;
        }
        let _ = std::fs::remove_dir(dir.join("chats"));
        if let Err(error) = std::fs::remove_dir(&dir) {
            eprintln!("erisreader: leaving {}: {error}", dir.display());
        }
        Ok(())
    }
}

pub struct Annotations {
    providers: Vec<(&'static str, Arc<dyn Provider>)>,
    catalog: Catalog,
    files: AgentFiles,
    bridge: Bridge,
    open: Mutex<Option<OpenDocument>>,
}

impl Annotations {
    pub fn new(
        providers: Vec<(&'static str, Arc<dyn Provider>)>,
        catalog: Catalog,
        files: AgentFiles,
        bridge: Bridge,
    ) -> Self {
        Self { providers, catalog, files, bridge, open: Mutex::default() }
    }

    /// The agent types in the agents folder, read again each time.
    pub fn types(&self) -> Result<Value> {
        let types = AgentTypes::load(&self.files)?;
        Ok(json!({
            "defaultType": types.default_type,
            "types": types.types.iter().map(|(id, definition)| json!({
                "id": id,
                "name": definition.name,
                "description": definition.description,
                "model": definition.model,
                "thinkingLevel": definition.thinking_level,
            })).collect::<Vec<_>>(),
        }))
    }

    /// Opens a document, closing the one open before, and lists its annotations.
    pub async fn open(&self, path: &str, text: DocumentText) -> Result<Vec<Value>> {
        ensure!(Path::new(path).is_absolute(), "A document opens by its full path.");
        ensure!(!text.pages.is_empty(), "The document has no pages.");
        let mut open = self.open.lock().await;
        if let Some(mut before) = open.take() {
            before.close().await;
        }
        let mut document = OpenDocument { path: path.to_owned(), text: Arc::new(text), harness: None, forwarding: None };
        if document.sidecar().join("harness.db").is_file() {
            self.start(&mut document).await?;
        }
        let annotations = match &document.harness {
            Some(harness) => list(harness)?,
            None => Vec::new(),
        };
        *open = Some(document);
        Ok(annotations)
    }

    /// Closes the open document; its agents resume when it opens again.
    pub async fn close(&self) {
        if let Some(mut document) = self.open.lock().await.take() {
            document.close().await;
        }
    }

    /// Opens the document's harness in its sidecar, making the sidecar if it has none.
    async fn start(&self, document: &mut OpenDocument) -> Result<Arc<Harness>> {
        if let Some(harness) = &document.harness {
            return Ok(harness.clone());
        }
        let dir = document.sidecar();
        std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
        let harness = self
            .providers
            .iter()
            .fold(Harness::builder(&dir).transcripts(dir.join("chats")), |builder, (name, provider)| {
                builder.provider(name, provider.clone())
            })
            .tools(tools::for_document(document.text.clone(), self.bridge.clone(), document.path.clone()))
            .open()
            .await
            .with_context(|| format!("opening the annotations in {}", dir.display()))?;
        let observations = harness.subscribe();
        document.forwarding =
            Some(tokio::spawn(forward(Arc::downgrade(&harness), observations, self.bridge.clone(), document.path.clone())));
        document.harness = Some(harness.clone());
        Ok(harness)
    }

    /// The open document's harness, if it has annotations.
    async fn harness(&self) -> Result<Arc<Harness>> {
        let open = self.open.lock().await;
        let document = open.as_ref().ok_or_else(|| anyhow!("No document is open."))?;
        document.harness.clone().ok_or_else(|| anyhow!("The document has no such annotation."))
    }

    pub async fn list(&self) -> Result<Vec<Value>> {
        let open = self.open.lock().await;
        match open.as_ref().and_then(|document| document.harness.as_ref()) {
            Some(harness) => list(harness),
            None => Ok(Vec::new()),
        }
    }

    /// An annotation and its conversation.
    pub async fn get(&self, id: &str) -> Result<Value> {
        let harness = self.harness().await?;
        let annotation = summary(&harness, &harness.agent(id)?)?;
        let entries: Vec<Value> = harness.transcript(id)?.iter().filter_map(entry).collect();
        Ok(json!({"annotation": annotation, "entries": entries}))
    }

    /// Starts an annotation of an agent type on a model, with its first message: for a region,
    /// with the region's image.
    pub async fn create(&self, request: &Value) -> Result<Value> {
        let text = message_text(request)?;
        let anchor: Anchor = serde_json::from_value(request["anchor"].clone()).context("a malformed annotation")?;
        let image = match &request["image"] {
            Value::Null => None,
            image => Some(serde_json::from_value::<Image>(image.clone()).context("a malformed image")?),
        };
        if let Some(image) = &image {
            ensure!(["image/png", "image/jpeg"].contains(&image.mime.as_str()), "A region's image is a PNG or JPEG.");
            ensure!(image.data.len() <= IMAGE_LIMIT, "The region's image is too large; draw a smaller one.");
        }
        let types = AgentTypes::load(&self.files)?;
        let agent_type = types.resolve(request["type"].as_str())?;
        let (provider, model, thinking) = self.choice(request).await?;

        let mut open = self.open.lock().await;
        let document = open.as_mut().ok_or_else(|| anyhow!("No document is open."))?;
        anchor.check(document.text.page_count())?;
        let system_prompt = instructions(&agent_type.definition.system_prompt, &document.text, &anchor);
        let cwd = Path::new(&document.path).parent().map_or_else(|| "/".into(), |dir| dir.display().to_string());
        let info = AnnotationInfo {
            title: title(text),
            anchor,
            agent_type: TypeRef { id: agent_type.id, name: agent_type.definition.name.clone() },
            thinking: thinking.name().to_owned(),
            created: storage::now(),
        };
        let harness = self.start(document).await?;
        let created = harness.create_agent(AgentSpec {
            system_prompt,
            tools: tools::NAMES.map(str::to_owned).to_vec(),
            provider,
            reasoning_effort: model.effort(thinking.name())?,
            context_window: model.context_window,
            model: model.id,
            metadata: serde_json::to_value(&info)?,
            machine: MachineSpec::Direct(DirectSpec { cwd, env: None }),
        });
        let id = match created {
            Ok(id) => id,
            Err(error) => {
                if harness.agents()?.is_empty() {
                    document.retire().await?;
                }
                return Err(error);
            }
        };
        drop(open);
        match image {
            Some(image) => harness.send_with_images(&id, "user", text, vec![image])?,
            None => harness.send(&id, "user", text)?,
        };
        summary(&harness, &harness.agent(&id)?)
    }

    pub async fn send(&self, id: &str, request: &Value) -> Result<()> {
        self.harness().await?.send(id, "user", message_text(request)?)?;
        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<()> {
        self.harness().await?.interrupt(id);
        Ok(())
    }

    /// Deletes an annotation and its conversation; the last one takes the sidecar with it.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut open = self.open.lock().await;
        let document = open.as_mut().ok_or_else(|| anyhow!("No document is open."))?;
        let harness = document.harness.clone().ok_or_else(|| anyhow!("The document has no such annotation."))?;
        harness.remove_agent(id).await?;
        let empty = harness.agents()?.is_empty();
        drop(harness);
        if empty {
            document.retire().await?;
        }
        Ok(())
    }

    /// Changes an annotation's model or thinking for its next turn.
    pub async fn configure(&self, id: &str, request: &Value) -> Result<Value> {
        let harness = self.harness().await?;
        let (provider, model, thinking) = self.choice(request).await?;
        let effort = model.effort(thinking.name())?;
        harness.update_agent(id, |spec| {
            spec.provider = provider;
            spec.model = model.id;
            spec.reasoning_effort = effort;
            spec.context_window = model.context_window;
            spec.metadata["thinking"] = json!(thinking.name());
        })?;
        summary(&harness, &harness.agent(id)?)
    }

    /// The model and thinking level a request names, checked against the signed-in accounts.
    async fn choice(&self, request: &Value) -> Result<(String, Model, ThinkingLevel)> {
        let selection = &request["model"];
        let (Some(provider), Some(id)) = (selection["provider"].as_str(), selection["id"].as_str()) else {
            bail!("Choose a model.");
        };
        let model = self.catalog.find(provider, id).await?;
        let thinking = ThinkingLevel::parse(request["thinking"].as_str().unwrap_or("off"))?;
        model.effort(thinking.name())?;
        Ok((provider.to_owned(), model, thinking))
    }
}

/// Every annotation, in the order they come in the document.
fn list(harness: &Harness) -> Result<Vec<Value>> {
    let mut annotations: Vec<Value> =
        harness.agents()?.iter().filter_map(|record| summary(harness, record).ok()).collect();
    let place = |annotation: &Value| {
        let rect = match &annotation["anchor"]["rects"][0] {
            Value::Null => &annotation["anchor"]["rect"],
            first => first,
        };
        (rect["page"].as_u64().unwrap_or_default(), rect["y"].as_f64().unwrap_or_default())
    };
    annotations.sort_by(|a, b| {
        let ((page_a, y_a), (page_b, y_b)) = (place(a), place(b));
        page_a.cmp(&page_b).then(y_a.total_cmp(&y_b))
    });
    Ok(annotations)
}

/// An annotation as ErisReader shows it.
fn summary(harness: &Harness, record: &AgentRecord) -> Result<Value> {
    let info: AnnotationInfo =
        serde_json::from_value(record.spec.metadata.clone()).context("not one of ErisReader's annotations")?;
    let updated = std::fs::metadata(harness.transcript_path(&record.id))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(info.created, |at| at.as_millis() as u64);
    let state = match record.state {
        AgentState::Idle => "idle",
        AgentState::Running => "running",
        AgentState::Failed => "failed",
    };
    let usage = record.usage;
    Ok(json!({
        "id": record.id,
        "title": info.title,
        "anchor": info.anchor,
        "agentType": info.agent_type,
        "model": {"provider": record.spec.provider, "id": record.spec.model},
        "thinking": info.thinking,
        "state": state,
        "error": record.error,
        "created": info.created,
        "updated": updated,
        "usage": {"input": usage.input, "cachedInput": usage.cached_input, "output": usage.output, "reasoning": usage.reasoning},
        "contextTokens": record.context_tokens,
        "contextWindow": record.spec.context_window,
    }))
}

/// Tells ErisReader what the document's annotations do: their state and usage, each finished
/// item, and replies as they stream. Every event names the document.
async fn forward(
    harness: Weak<Harness>,
    mut observations: broadcast::Receiver<Observation>,
    bridge: Bridge,
    document: String,
) {
    loop {
        let observation = match observations.recv().await {
            Ok(observation) => observation,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => return,
        };
        let send = |event: &str, mut fields: Value| {
            fields["document"] = json!(document);
            bridge.event(event, fields);
        };
        match observation {
            Observation::State { agent, .. } | Observation::Usage { agent, .. } => {
                let Some(harness) = harness.upgrade() else { return };
                if let Ok(annotation) = harness.agent(&agent).and_then(|record| summary(&harness, &record)) {
                    send("annotation", json!({"annotation": annotation}));
                }
            }
            Observation::Item { agent, entry: item } => {
                if let Some(entry) = entry(&item) {
                    send("entry", json!({"annotation": agent, "entry": entry}));
                }
            }
            Observation::TextDelta { agent, text } => send("delta", json!({"annotation": agent, "text": text})),
            Observation::Held { agent, hold } => {
                let held = match hold {
                    None => None,
                    Some(Hold::Queued) => Some("queued"),
                    Some(Hold::RateLimited { .. }) => Some("rate-limited"),
                };
                send("held", json!({"annotation": agent, "held": held}));
            }
            _ => {}
        }
    }
}

fn message_text(request: &Value) -> Result<&str> {
    let text = request["text"].as_str().unwrap_or_default();
    ensure!(!text.trim().is_empty(), "Write a message first.");
    Ok(text)
}

/// An annotation's title from its first message: its first line, cut at a word near the length.
fn title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    if line.chars().count() <= TITLE_LENGTH {
        return line.to_owned();
    }
    let cut: String = line.chars().take(TITLE_LENGTH).collect();
    let cut = match cut.rfind(char::is_whitespace) {
        Some(space) if space > TITLE_LENGTH / 2 => &cut[..space],
        _ => &cut,
    };
    format!("{}…", cut.trim_end_matches(|c: char| c.is_whitespace() || c.is_ascii_punctuation()))
}

/// A transcript entry as the conversation shows it: messages, replies, thinking, and what the
/// agent looked at. Tool results and empty reasoning show nothing.
pub fn entry(entry: &Entry) -> Option<Value> {
    let (kind, text, images) = match &entry.item {
        Item::Input { from, text, images } if from == "user" => ("user", text.clone(), images.len()),
        Item::Input { from, text, images } => ("mail", format!("From {from}:\n\n{text}"), images.len()),
        Item::Assistant { text } => ("assistant", text.clone(), 0),
        Item::Reasoning { summary, .. } if summary.iter().any(|s| !s.trim().is_empty()) => {
            ("reasoning", summary.join("\n\n"), 0)
        }
        Item::ToolCall { name, arguments, .. } => ("tool", tools::describe(name, arguments)?, 0),
        Item::Compaction { summary } => ("compaction", summary.clone(), 0),
        _ => return None,
    };
    Some(json!({"seq": entry.seq, "at": entry.at, "kind": kind, "text": text, "images": images}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Page;

    fn document(pages: usize) -> DocumentText {
        DocumentText {
            name: "paper.pdf".into(),
            pages: (0..pages).map(|_| Page { label: None, text: "words".into() }).collect(),
        }
    }

    fn rect(page: usize) -> PageRect {
        PageRect { page, x: 0.1, y: 0.2, width: 0.5, height: 0.03 }
    }

    #[test]
    fn a_passage_reaches_the_agent_with_its_page() {
        let anchor = Anchor::Text { rects: vec![rect(3)], text: "  the passage\n".into() };
        let prompt = instructions("Be helpful.\n", &document(12), &anchor);
        assert!(prompt.starts_with("Be helpful.\n\nThe reader is reading \"paper.pdf\", a PDF of 12 pages,"), "{prompt}");
        assert!(prompt.ends_with("The annotation marks this passage on page 4:\n\n<passage>\nthe passage\n</passage>\n"), "{prompt}");

        let across = Anchor::Text { rects: vec![rect(3), rect(4)], text: "spans".into() };
        let mut labelled = document(12);
        labelled.pages[3].label = Some("iv".into());
        assert!(instructions("", &labelled, &across).contains("on page 4 (labelled \"iv\") to page 5:"));
    }

    #[test]
    fn a_region_reaches_the_agent_as_an_image_and_its_text() {
        let region = Anchor::Region { rect: rect(0), text: "Figure 2".into() };
        let prompt = instructions("", &document(1), &region);
        assert!(prompt.contains("a PDF of 1 page,"));
        assert!(prompt.contains("a region of page 1, which comes as an image with the reader's first message."));
        assert!(prompt.ends_with("<region-text>\nFigure 2\n</region-text>\n"));
        let blank = Anchor::Region { rect: rect(0), text: " ".into() };
        assert!(instructions("", &document(1), &blank).ends_with("The PDF has no text inside the region.\n"));
    }

    #[test]
    fn anchors_must_lie_on_the_document_pages() {
        let text = |rects, text: &str| Anchor::Text { rects, text: text.into() };
        assert!(text(vec![rect(1)], "fine").check(2).is_ok());
        assert!(text(vec![rect(2)], "past the end").check(2).is_err());
        assert!(text(vec![], "nowhere").check(2).is_err());
        assert!(text(vec![rect(0)], "  ").check(2).is_err());
        let outside = PageRect { page: 0, x: 0.8, y: 0.1, width: 0.5, height: 0.1 };
        assert!(Anchor::Region { rect: outside, text: String::new() }.check(1).is_err());
        let empty = PageRect { page: 0, x: 0.1, y: 0.1, width: 0.0, height: 0.1 };
        assert!(Anchor::Region { rect: empty, text: String::new() }.check(1).is_err());
        assert!(Anchor::Region { rect: rect(0), text: String::new() }.check(1).is_ok());
    }

    #[test]
    fn anchors_read_as_the_renderer_writes_them() {
        let anchor: Anchor = serde_json::from_value(json!({
            "kind": "region", "rect": {"page": 2, "x": 0.1, "y": 0.2, "width": 0.3, "height": 0.4}, "text": "",
        }))
        .unwrap();
        assert_eq!(anchor.pages(), (3, 3));
        assert!(serde_json::from_value::<Anchor>(json!({"kind": "text", "text": "no rects"})).is_err());
    }

    #[test]
    fn the_sidecar_sits_beside_the_document() {
        assert_eq!(sidecar(Path::new("/books/paper.pdf")), PathBuf::from("/books/paper.pdf.erisreader"));
    }

    #[test]
    fn titles_are_the_first_line_cut_at_a_word() {
        assert_eq!(title("\n  Hello there  \nsecond line"), "Hello there");
        let long = "Explain how the borrow checker decides when two mutable references to the same value overlap";
        let cut = title(long);
        assert!(cut.ends_with('…') && cut.chars().count() <= TITLE_LENGTH + 1, "{cut}");
        assert!(long.starts_with(cut.trim_end_matches('…')));
    }

    #[test]
    fn the_conversation_shows_messages_replies_thinking_and_reading() {
        let at = |item| Entry { seq: 1, at: 2, event: None, item };
        let shown = |item| entry(&at(item)).map(|v| (v["kind"].as_str().unwrap().to_owned(), v["text"].clone()));
        assert_eq!(
            shown(Item::Input { from: "user".into(), text: "hi".into(), images: vec![] }),
            Some(("user".into(), json!("hi")))
        );
        assert_eq!(shown(Item::Assistant { text: "hello".into() }), Some(("assistant".into(), json!("hello"))));
        assert_eq!(shown(Item::Reasoning { summary: vec![], encrypted: Some("x".into()) }), None);
        assert_eq!(
            shown(Item::ToolCall { call_id: "c".into(), name: "view_page".into(), arguments: r#"{"page":3}"#.into() }),
            Some(("tool".into(), json!("Looked at page 3")))
        );
    }
}
