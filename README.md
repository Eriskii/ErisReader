# ErisReader

Read a PDF and talk its passages through with [ErisHarness](https://github.com/Eriskii/ErisHarness)
agents, in the margin, on a ChatGPT or Claude login. An Electron app whose renderer is a SvelteKit
app: the document fills the window, with no frame around it.

- **Reading**: pages in a column down the middle of a dark reading surface, drawn by pdf.js with
  its text layer for selecting. The view fits the widest page to the window, leaving room for a
  chat beside it; `Ctrl +`, `Ctrl −` and `Ctrl` with the wheel zoom, and `Ctrl 0` fits again.
  A new zoom or window size stretches the pages at once and draws them again once it holds.
  A document reopens where it was left.
- **Annotations**: select text and choose **Ask about this** (or `Ctrl L`), or press
  `Ctrl Shift S` and drag over a page to screenshot a region. The chat opens in the right margin,
  level with the passage, moving the pages over to make room. It keeps to its page: it rises as
  it grows rather than run past the page's foot, stays in view while the page is, never taller
  than the part of the page in view, and its conversation scrolls inside it. Replies are Markdown
  with their math typeset by KaTeX: `$…$` and `\(…\)` inline, `$$…$$` and `\[…\]` displayed.
  Sending the first message makes the
  annotation: the passage stays highlighted on the page, a region stays framed by a thin
  wireframe box. Clicking either opens its chat again. `Esc` puts the chat away.
- **The agent** sees the document and nothing else. Its system prompt is its agent type's, then
  the document's name and length and the passage, or, for a region, the text inside it, with the
  region's picture sent along with the first message. Its tools:
  - `read_pages`: the document's text from one page to another, each page under a
    `[Page n of N]` line (with the document's own label where it has one), at most 50,000
    characters a call;
  - `search`: every match of some text, ignoring case and line breaks, with its page and the text
    around it;
  - `view_page`: a page drawn as an image, 1568 pixels on its longer side, for layout, figures,
    tables and equations.

  The conversation shows what the agent read, searched for and looked at.
- **Annotations list**: the button at the top left, or `Ctrl K`, opens every annotation of the
  document down the left side, in the order they come in it: its page, its passage and its first
  message. Pick one to scroll to it and open its chat; delete one (deleting asks first).
- **Controls** float in a column down the left edge, quiet until pointed at: the list, opening
  another PDF (`Ctrl O`), the screenshot tool, settings, and at the bottom, zoom and the page
  being read. `Ctrl W` closes the document, back to the welcome, which lists recent documents.
  A PDF dropped on the window opens, and so does one named on the command line.
- **Choices**, below a new annotation's message: the **agent type** (where the agents folder
  has more than one), the **model**, from every signed-in account's, and its **thinking** level.
  Changing the model or thinking in an open chat applies from its next reply.
- **Settings**: the accounts, the agent types, and where the data lives.

## Where annotations live

Beside the document, in a folder named for it: `paper.pdf` keeps its annotations in
`paper.pdf.erisreader/`.

```
paper.pdf
paper.pdf.erisreader/
  harness.db                   each annotation's record (where it is, title, type, model, thinking, usage) and mail
  chats/<id>/transcript.jsonl  each annotation's conversation
```

The first annotation makes the folder, and deleting the last one removes it, so reading a
document leaves nothing beside it. Moving or renaming a document with its folder keeps its
annotations. An annotation is an ErisHarness agent: its record and its transcript. One that was
replying when the document closed resumes its reply when the document opens again.

## Accounts

As ErisChat's, and signed in the same ways:

- **ChatGPT**: signed in as the Codex CLI signs in, in the browser or with a device code.
- **Claude**: a Claude subscription's login, from the `.credentials.json` Claude Code keeps in
  `~/.claude`, or on macOS from the Keychain. ErisReader refreshes its copy of the login itself;
  Claude refresh tokens are single-use, so the Claude Code session the login came from then has to
  sign in again, and the other way around.

## Agent types

Each annotation talks to an agent of a type: a Markdown file in the `agents` folder, declared
exactly as ErisAE's [`.ae/agents/*.md`](https://github.com/Eriskii/ErisAE/blob/main/docs/agent-types.md),
and read under the same rules and limits. A new folder gets `default.md`, the **Reader**. A type's
`tools`, `capabilities`, `permissions` and `forgeRoles` are read and checked as ErisAE checks
them, but every annotation's agent has the document tools alone.

## Data

Agent types, logins, recent documents and the model list are kept in the system's place for an
application's data: `~/Library/Application Support/ErisReader` on macOS, `$XDG_DATA_HOME/erisreader`
(by default `~/.local/share/erisreader`) elsewhere. `ERISREADER_DATA` names another folder, and an
app started with `--user-data-dir` keeps its data in that profile's `data/`.

```
agents/*.md            agent types
accounts/              the ChatGPT and Claude logins, readable only by the user
models.json            the accounts' models, as last listed
recent.json            the documents opened lately
```

## Running

Needs Node 22+ and Rust (cargo). Inside the monorepo, `.cargo/config.toml` builds the agents
helper against the local ErisHarness. On Linux, the Nix dev shell supplies Node, a Nix-built
Electron 44, and `xvfb-run` for the e2e tests; on macOS, npm's Electron is used.

```sh
nix-shell                 # Linux
npm install
npm run dev               # Electron on SvelteKit's dev server, with hot reloading
npm run build             # agents helper into out/agents, pdf.js assets into static/pdfjs, main, preload and renderer into out/
npm start                 # build, then run the production bundle
npm start -- paper.pdf    # … opening a document
npm test                  # the helper's tests, unit, then e2e
npm run test:agents       # cargo test for the helper
npm run test:unit         # offline, fast
npm run test:e2e          # builds, then drives the app (offscreen on Linux) against a stand-in ChatGPT
npm run test:e2e:headed
npm run typecheck         # svelte-check over the whole project
```

The main process builds with Vite (`vite.main.config.ts`) to `out/main`. The preload
(`vite.preload.config.ts`) builds to one CommonJS file, as a sandboxed preload must be, and gives
the renderer `window.erisreader`. The renderer builds with SvelteKit (`vite.config.ts`, static
adapter) and is served at `erisreader://app/`, with pdf.js's CMaps, standard fonts and decoders
under `/pdfjs/`. Renderer modules import each other as `#lib/...` (`package.json` subpath imports).

The annotations run in a helper process, `agents/` (Rust), which the main process starts and
speaks to in JSON lines over stdin and stdout. The renderer hands it each page's text when a
document opens; when an agent looks at a page, the helper asks the renderer, through the main
process, to draw it. `ERISREADER_OPENAI_AUTH`, `ERISREADER_CODEX_API`, `ERISREADER_ANTHROPIC_API`
and `ERISREADER_CLAUDE_AUTH` point the helper at other servers, as the e2e tests do with a
stand-in.

## Layout

| Path | Role |
| --- | --- |
| `src/main/index.ts` | Main process: the window, its menu, external links, the `erisreader:` scheme |
| `src/main/documents.ts`, `src/main/recent.ts` | Picking, reading and remembering PDFs; the one a command line names |
| `src/main/agents.ts` | Starts the agents helper and relays between it and the renderer, page drawings included |
| `src/main/app-scheme.ts`, `src/main/data-folder.ts` | Serving the renderer build; where the data lives on each system |
| `agents/src/main.rs` | The helper: its commands |
| `agents/src/annotations.rs` | Annotations as ErisHarness agents in the document's sidecar: anchors, prompts, events |
| `agents/src/document.rs`, `agents/src/tools.rs` | The document's text, and the agents' `read_pages`, `search` and `view_page` |
| `agents/src/agent_types/` | Agent types, as ErisAE declares and reads them |
| `agents/src/accounts/`, `agents/src/models.rs`, `agents/src/claude.rs` | The ChatGPT sign-in and the Claude login, their models, and the providers agents call them through |
| `src/shared/reader.ts` | Accounts, models, types, anchors, annotations, entries, events, and IPC channels |
| `src/shared/palette.ts` | ErisDE's OKLCH colour table, with the reading surface |
| `src/preload/` | `window.erisreader`: the renderer's way to the main process |
| `src/lib/reader/` | The window: the document or the welcome, the floating controls, keys, dropping files |
| `src/lib/document/` | pdf.js: opening, text, drawing pages and regions; the pages' layout, selection and screenshots; geometry of anchors |
| `src/lib/annotations/` | The store, the chat in the margin, the composer, the list, the model picker, Markdown and math |
| `src/lib/settings/` | ErisDE's settings modal and its sections: the accounts, the agent types |
| `src/lib/theme/` | Base and control styles, and painting the palette into `--color-*` properties |
| `tests/unit/` | Anchor geometry, recent documents, the app scheme's path checks, the data folder |
| `tests/support/` | A stand-in for ChatGPT that streams replies and calls tools, and a PDF writer for fixtures |
| `tests/e2e/` | Playwright tests driving the built app |
