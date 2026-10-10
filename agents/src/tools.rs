//! The tools an annotation's agent has: the open document, and nothing else. There is no shell
//! and no file system behind them. `read_pages` and `search` work on the text the renderer
//! extracted; `view_page` asks the renderer to draw a page.
use crate::bridge::Bridge;
use crate::document::{DocumentText, READ_LIMIT, SEARCH_LIMIT};
use erisharness::tools::{Content, Tool, ToolContext, ToolOutput};
use futures_util::future::BoxFuture;
use serde_json::{Value, json};
use std::sync::Arc;

pub const READ_PAGES: &str = "read_pages";
pub const SEARCH: &str = "search";
pub const VIEW_PAGE: &str = "view_page";

/// Every tool's name, as agents list them.
pub const NAMES: [&str; 3] = [READ_PAGES, SEARCH, VIEW_PAGE];

/// How long a page image's longer side is: as large as models look at closely.
pub const PAGE_IMAGE: u32 = 1568;

/// The tools for one document, which the renderer at `path` draws.
pub fn for_document(text: Arc<DocumentText>, bridge: Bridge, path: String) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadPages(text.clone())),
        Arc::new(Search(text.clone())),
        Arc::new(ViewPage { text, bridge, path }),
    ]
}

/// A page number argument.
fn page(args: &Value, key: &str) -> Option<usize> {
    args[key].as_u64().map(|n| n as usize).or_else(|| args[key].as_f64().filter(|n| n.fract() == 0.0 && *n >= 0.0).map(|n| n as usize))
}

struct ReadPages(Arc<DocumentText>);

impl Tool for ReadPages {
    fn name(&self) -> &str {
        READ_PAGES
    }

    fn description(&self) -> String {
        format!(
            "Read the document's text, page by page, from page `from` through page `to` (pages count from 1; `to` defaults to the last page). Each page starts with a [Page n of N] line. One call returns at most {READ_LIMIT} characters, and when it stops early it says where to continue. The text is extracted from the PDF: figures, tables and equations may come out garbled or missing, so look at those pages with view_page."
        )
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from": {"type": "integer", "minimum": 1, "description": "The first page to read."},
                "to": {"type": "integer", "minimum": 1, "description": "The last page to read."}
            },
            "required": ["from"],
            "additionalProperties": false
        })
    }

    fn snippet(&self) -> &str {
        "Read the document's text by page"
    }

    fn call<'a>(&'a self, _context: &'a ToolContext, args: Value) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let Some(from) = page(&args, "from") else {
                return ToolOutput::error("Give `from`, the first page to read, counting from 1.");
            };
            let to = page(&args, "to").unwrap_or(self.0.page_count());
            match self.0.read(from, to) {
                Ok(text) => ToolOutput::text(text),
                Err(error) => ToolOutput::error(error.to_string()),
            }
        })
    }
}

struct Search(Arc<DocumentText>);

impl Tool for Search {
    fn name(&self) -> &str {
        SEARCH
    }

    fn description(&self) -> String {
        format!(
            "Find text in the document. Matching ignores case and treats any run of spaces and line breaks as one space. Lists the page of each match with the text around it, at most {SEARCH_LIMIT} of them."
        )
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"query": {"type": "string", "description": "The text to find."}},
            "required": ["query"],
            "additionalProperties": false
        })
    }

    fn snippet(&self) -> &str {
        "Find text in the document"
    }

    fn call<'a>(&'a self, _context: &'a ToolContext, args: Value) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            match self.0.search(args["query"].as_str().unwrap_or_default()) {
                Ok(text) => ToolOutput::text(text),
                Err(error) => ToolOutput::error(error.to_string()),
            }
        })
    }
}

struct ViewPage {
    text: Arc<DocumentText>,
    bridge: Bridge,
    path: String,
}

impl Tool for ViewPage {
    fn name(&self) -> &str {
        VIEW_PAGE
    }

    fn description(&self) -> String {
        "Look at a page of the document as an image, as the reader sees it: its layout, figures, tables and equations, which the extracted text loses. Use it for pages without text, too.".into()
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"page": {"type": "integer", "minimum": 1, "description": "The page to look at, counting from 1."}},
            "required": ["page"],
            "additionalProperties": false
        })
    }

    fn snippet(&self) -> &str {
        "See a page of the document as an image"
    }

    fn call<'a>(&'a self, context: &'a ToolContext, args: Value) -> BoxFuture<'a, ToolOutput> {
        Box::pin(async move {
            let Some(number) = page(&args, "page") else {
                return ToolOutput::error("Give `page`, the page to look at, counting from 1.");
            };
            if let Err(error) = self.text.check(number) {
                return ToolOutput::error(error.to_string());
            }
            let question = json!({"render": {"document": self.path, "page": number - 1, "longest": PAGE_IMAGE}});
            let answer = tokio::select! {
                answer = self.bridge.ask(question) => answer,
                () = context.cancel.cancelled() => return ToolOutput::error("Interrupted before the page was drawn."),
            };
            match answer {
                Ok(answer) => match (answer["image"]["mime"].as_str(), answer["image"]["data"].as_str()) {
                    (Some(mime), Some(data)) => ToolOutput::new(vec![
                        Content::Text(self.text.page_name(number)),
                        Content::Image { mime: mime.to_owned(), data: data.to_owned() },
                    ]),
                    _ => ToolOutput::error("ErisReader drew no image of that page."),
                },
                Err(error) => ToolOutput::error(format!("The page could not be drawn: {error:#}")),
            }
        })
    }
}

/// What a tool call did, as the conversation shows it: "Read pages 2–3".
pub fn describe(name: &str, arguments: &str) -> Option<String> {
    let args: Value = serde_json::from_str(arguments).ok()?;
    Some(match name {
        READ_PAGES => match (page(&args, "from"), page(&args, "to")) {
            (Some(from), Some(to)) if to > from => format!("Read pages {from}–{to}"),
            (Some(from), Some(_)) => format!("Read page {from}"),
            (Some(from), None) => format!("Read from page {from}"),
            _ => "Read the document".into(),
        },
        SEARCH => format!("Searched for “{}”", args["query"].as_str()?.trim()),
        VIEW_PAGE => format!("Looked at page {}", page(&args, "page")?),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_calls_read_as_what_they_did() {
        assert_eq!(describe(READ_PAGES, r#"{"from": 2, "to": 3}"#).as_deref(), Some("Read pages 2–3"));
        assert_eq!(describe(READ_PAGES, r#"{"from": 4, "to": 4}"#).as_deref(), Some("Read page 4"));
        assert_eq!(describe(READ_PAGES, r#"{"from": 4}"#).as_deref(), Some("Read from page 4"));
        assert_eq!(describe(SEARCH, r#"{"query": " lazy dog "}"#).as_deref(), Some("Searched for “lazy dog”"));
        assert_eq!(describe(VIEW_PAGE, r#"{"page": 2.0}"#).as_deref(), Some("Looked at page 2"));
        assert_eq!(describe(VIEW_PAGE, "{\"page\""), None);
        assert_eq!(describe("bash", "{}"), None);
    }
}
