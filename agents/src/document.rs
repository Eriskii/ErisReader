//! The open document's text, page by page, as ErisReader's renderer extracts it with pdf.js:
//! what agents read and search. Pages are numbered from 1, in order; a page whose document
//! labels it otherwise (a preface's `iv`, say) carries that label too.
use anyhow::{Result, ensure};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Page {
    /// The document's own label for the page, where it gives one.
    #[serde(default)]
    pub label: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocumentText {
    /// The file's name, as the reader sees it.
    pub name: String,
    pub pages: Vec<Page>,
}

/// The most text one `read` returns.
pub const READ_LIMIT: usize = 50_000;
/// The most matches one `search` lists.
pub const SEARCH_LIMIT: usize = 40;
/// How much text a match shows on either side.
const CONTEXT: usize = 60;

impl DocumentText {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// A page by its number from 1, as tools and prompts name it: `Page 3 of 120`, with the
    /// document's label where it differs.
    pub fn page_name(&self, number: usize) -> String {
        let total = self.page_count();
        match self.label(number) {
            Some(label) => format!("Page {number} of {total}, labelled \"{label}\""),
            None => format!("Page {number} of {total}"),
        }
    }

    /// The page's label, unless it is just its number.
    pub fn label(&self, number: usize) -> Option<&str> {
        let label = self.pages.get(number.checked_sub(1)?)?.label.as_deref()?.trim();
        (!label.is_empty() && label != number.to_string()).then_some(label)
    }

    /// Checks a page number from a model or the renderer.
    pub fn check(&self, number: usize) -> Result<()> {
        let total = self.page_count();
        ensure!((1..=total).contains(&number), "There is no page {number}: the document has pages 1 to {total}.");
        Ok(())
    }

    /// Pages `from` to `to`, each under a `[Page n of N]` line, stopping before a page that
    /// would take the text past [`READ_LIMIT`] and saying where to go on.
    pub fn read(&self, from: usize, to: usize) -> Result<String> {
        self.check(from)?;
        let to = to.min(self.page_count());
        ensure!(from <= to, "Read from a page up to a later one: {from} comes after {to}.");
        let mut out = String::new();
        for number in from..=to {
            let page = self.pages[number - 1].text.trim();
            let body = if page.is_empty() {
                "(No text: the page may be a picture or a scan. view_page shows it.)"
            } else {
                page
            };
            let block = format!("[{}]\n{body}\n\n", self.page_name(number));
            if out.len() + block.len() > READ_LIMIT {
                if out.is_empty() {
                    let cut = floor_boundary(&block, READ_LIMIT);
                    out.push_str(&block[..cut]);
                    out.push_str(&format!(
                        "\n\n[Page {number} goes on past {READ_LIMIT} characters; search finds the rest of it, and view_page shows it whole.]"
                    ));
                    if number < to {
                        out.push_str(&format!(" [Continue with from: {}.]", number + 1));
                    }
                } else {
                    out.push_str(&format!(
                        "[Stopped before page {number} to stay under {READ_LIMIT} characters. Continue with from: {number}.]"
                    ));
                }
                return Ok(out);
            }
            out.push_str(&block);
        }
        Ok(out.trim_end().to_owned())
    }

    /// Every place `query` appears, ignoring case and treating any run of whitespace as one
    /// space, with the text around it: at most [`SEARCH_LIMIT`] of them.
    pub fn search(&self, query: &str) -> Result<String> {
        let wanted: Vec<char> = fold(&normalize(query));
        ensure!(!wanted.is_empty(), "Search for some text.");
        let mut total = 0;
        let mut lines = Vec::new();
        for (index, page) in self.pages.iter().enumerate() {
            let shown: Vec<char> = normalize(&page.text);
            let folded = fold(&shown);
            let mut at = 0;
            while at + wanted.len() <= folded.len() {
                if folded[at..at + wanted.len()] != wanted[..] {
                    at += 1;
                    continue;
                }
                total += 1;
                if lines.len() < SEARCH_LIMIT {
                    let start = at.saturating_sub(CONTEXT);
                    let end = (at + wanted.len() + CONTEXT).min(shown.len());
                    let before = if start > 0 { "…" } else { "" };
                    let after = if end < shown.len() { "…" } else { "" };
                    let context: String = shown[start..end].iter().collect();
                    lines.push(format!("Page {}: {before}{context}{after}", index + 1));
                }
                at += wanted.len();
            }
        }
        let query = query.trim();
        Ok(match total {
            0 => format!("No matches for \"{query}\"."),
            1 => format!("1 match for \"{query}\":\n{}", lines.join("\n")),
            _ if total > lines.len() => {
                format!("{total} matches for \"{query}\"; the first {}:\n{}", lines.len(), lines.join("\n"))
            }
            _ => format!("{total} matches for \"{query}\":\n{}", lines.join("\n")),
        })
    }
}

/// Text with every run of whitespace one space, trimmed.
fn normalize(text: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars());
    }
    out
}

/// Each character in lower case, where that is one character, so positions stay aligned with
/// the text shown.
fn fold(text: &[char]) -> Vec<char> {
    text.iter()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => *c,
            }
        })
        .collect()
}

fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len())).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(pages: &[&str]) -> DocumentText {
        DocumentText {
            name: "test.pdf".into(),
            pages: pages.iter().map(|text| Page { label: None, text: (*text).into() }).collect(),
        }
    }

    #[test]
    fn reading_gives_each_page_under_its_number() {
        let doc = document(&["one\nalpha", "two", "", "four"]);
        assert_eq!(doc.read(2, 3).unwrap(), "[Page 2 of 4]\ntwo\n\n[Page 3 of 4]\n(No text: the page may be a picture or a scan. view_page shows it.)");
        assert_eq!(doc.read(4, 99).unwrap(), "[Page 4 of 4]\nfour");
        assert!(doc.read(0, 1).unwrap_err().to_string().contains("pages 1 to 4"));
        assert!(doc.read(5, 5).is_err());
        assert!(doc.read(3, 2).is_err());
    }

    #[test]
    fn labels_show_where_they_differ_from_the_number() {
        let mut doc = document(&["a", "b", "c"]);
        doc.pages[0].label = Some("i".into());
        doc.pages[1].label = Some("2".into());
        assert_eq!(doc.page_name(1), "Page 1 of 3, labelled \"i\"");
        assert_eq!(doc.page_name(2), "Page 2 of 3");
        assert_eq!(doc.page_name(3), "Page 3 of 3");
    }

    #[test]
    fn reading_stops_before_the_limit_and_says_where_to_go_on() {
        let page = "x".repeat(READ_LIMIT / 3);
        let doc = document(&[&page, &page, &page, &page]);
        let read = doc.read(1, 4).unwrap();
        assert!(read.len() <= READ_LIMIT + 200);
        assert!(read.contains("[Page 2 of 4]") && !read.contains("[Page 3 of 4]"), "{}", &read[read.len() - 200..]);
        assert!(read.ends_with("Continue with from: 3.]"));

        let huge = document(&[&"é".repeat(READ_LIMIT), "next"]);
        let read = huge.read(1, 2).unwrap();
        assert!(read.contains("goes on past") && read.ends_with("[Continue with from: 2.]"));
    }

    #[test]
    fn search_ignores_case_and_line_breaks() {
        let doc = document(&["The quick brown fox jumps over the lazy\ndog.", "nothing", "The LAZY dog sleeps."]);
        let found = doc.search("lazy   DOG").unwrap();
        assert!(found.starts_with("2 matches for \"lazy   DOG\":"), "{found}");
        assert!(found.contains("Page 1: The quick brown fox jumps over the lazy dog."));
        assert!(found.contains("Page 3: The LAZY dog sleeps."));
        assert_eq!(doc.search("absent").unwrap(), "No matches for \"absent\".");
        assert!(doc.search("  ").is_err());
    }

    #[test]
    fn search_shows_the_text_around_a_match_and_caps_the_list() {
        let long = format!("{} needle {}", "a ".repeat(100), "b ".repeat(100));
        let found = document(&[&long]).search("needle").unwrap();
        let line = found.lines().nth(1).unwrap();
        assert!(line.starts_with("Page 1: …") && line.ends_with('…') && line.contains("needle"), "{line}");

        let many = document(&[&"hit ".repeat(SEARCH_LIMIT + 5)]);
        let found = many.search("hit").unwrap();
        assert!(found.starts_with(&format!("{} matches for \"hit\"; the first {SEARCH_LIMIT}:", SEARCH_LIMIT + 5)));
        assert_eq!(found.lines().count(), SEARCH_LIMIT + 1);
    }
}
