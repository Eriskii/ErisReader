/**
 * ErisReader's annotations: passages and regions of the open PDF, each an ErisHarness agent on
 * the signed-in ChatGPT or Claude account. They run in a helper process (agents/, in Rust)
 * that the main process starts and speaks to; the renderer reaches it, and the documents, as
 * `window.erisreader`.
 */

export type ProviderId = 'openai' | 'anthropic'

/** How an account signs in: ChatGPT in the browser or with a device code; Claude from Claude Code's login. */
export type SignInMethod = 'browser' | 'device-code' | 'credentials-file' | 'keychain'

/** An account, as ErisDE shows an ErisAE account. */
export interface Account {
  providerId: ProviderId
  providerLabel: string
  status: 'connected' | 'login-pending' | 'error' | 'signed-out'
  email: string | null
  plan: string | null
  /** Why the account is unavailable. */
  detail: string | null
  signInMethods: SignInMethod[]
  /** A sign-in under way: the page that finishes it, and for a device code, the code to enter there. */
  login: { url: string; userCode: string | null } | null
}

/** A model's provider (`openai-codex` for ChatGPT, `anthropic` for Claude) and id, as agent types name them. */
export interface ModelSelection {
  provider: string
  id: string
}

export interface Model extends ModelSelection {
  name: string
  contextWindow: number | null
  /** The thinking levels the model takes, besides `off`. */
  thinkingLevels: string[]
}

/** An agent type from the agents folder. */
export interface AgentType {
  id: string
  name: string
  description: string
  model: ModelSelection | null
  thinkingLevel: string | null
}

export interface AgentTypes {
  defaultType: string
  types: AgentType[]
}

/** A rectangle on a page: the page's index from 0, and fractions of its width and height from its top-left corner. */
export interface PageRect {
  page: number
  x: number
  y: number
  width: number
  height: number
}

/** Where an annotation is: a passage of text, line by line, or a region drawn over a page, with the text inside it. */
export type Anchor = { kind: 'text'; rects: PageRect[]; text: string } | { kind: 'region'; rect: PageRect; text: string }

export type AnnotationState = 'idle' | 'running' | 'failed'

export interface Annotation {
  id: string
  /** From its first message. */
  title: string
  anchor: Anchor
  agentType: { id: string; name: string }
  model: ModelSelection
  thinking: string
  state: AnnotationState
  /** Why the last turn failed; the next message tries again. */
  error: string | null
  /** Unix milliseconds. */
  created: number
  updated: number
  usage: { input: number; cachedInput: number; output: number; reasoning: number }
  /** How full the context is: the input tokens of the latest model call. */
  contextTokens: number
  contextWindow: number | null
}

/** One step of a conversation. */
export interface Entry {
  seq: number
  /** Unix milliseconds. */
  at: number
  /**
   * `tool` is what the agent looked at in the document; `mail` a message from someone other
   * than the reader; `compaction` a summary of what came before.
   */
  kind: 'user' | 'assistant' | 'reasoning' | 'tool' | 'compaction' | 'mail'
  text: string
  /** Images sent with a message. */
  images: number
}

export interface OpenedAnnotation {
  annotation: Annotation
  entries: Entry[]
}

/** An image as base64 data. */
export interface Image {
  mime: 'image/png' | 'image/jpeg'
  data: string
}

export interface NewAnnotation {
  anchor: Anchor
  type: string
  model: ModelSelection
  thinking: string
  text: string
  /** A region's picture, sent with its first message. */
  image: Image | null
}

/** A page's text as pdf.js extracts it, and its label where the document gives one. */
export interface PageText {
  label: string | null
  text: string
}

/** A PDF read from disk. */
export interface LoadedDocument {
  path: string
  name: string
  data: Uint8Array
}

export interface RecentDocument {
  path: string
  name: string
  /** Unix milliseconds. */
  opened: number
}

export type Held = 'queued' | 'rate-limited' | null

/** Events about annotations name the document they belong to. */
export type ReaderEvent =
  | { event: 'account'; account: Account }
  | { event: 'login-completed'; providerId: ProviderId; ok: boolean; error: string | null }
  | { event: 'annotation'; document: string; annotation: Annotation }
  | { event: 'entry'; document: string; annotation: string; entry: Entry }
  /** Reply text as it streams; the finished reply arrives as an entry. */
  | { event: 'delta'; document: string; annotation: string; text: string }
  | { event: 'held'; document: string; annotation: string; held: Held }
  /** The helper process ended; the next call starts it again. */
  | { event: 'ended'; error: string }

/** The helper asks the renderer to draw a page of a document, its longer side `longest` pixels. */
export interface RenderRequest {
  document: string
  page: number
  longest: number
}

/** Where ErisReader keeps its data. */
export interface Folders {
  data: string
  agents: string
}

/** Calls reject with the helper's own account of what went wrong. */
export interface ReaderApi {
  accounts(): Promise<Account[]>
  /** Starts signing in; a browser sign-in's page opens in the browser. */
  login(provider: 'openai', method: 'browser' | 'device-code'): Promise<Account>
  /** Adds the Claude login from a credentials file the user picks, or from the Keychain. Resolves to undefined if the user cancels the file picker. */
  addClaudeLogin(method: 'credentials-file' | 'keychain'): Promise<Account | undefined>
  cancelLogin(): Promise<void>
  logout(provider: ProviderId): Promise<void>
  models(): Promise<Model[]>
  types(): Promise<AgentTypes>

  /** Asks for a PDF to open; undefined if the user cancels. */
  pickDocument(): Promise<string | undefined>
  /** Reads a PDF, and remembers it among the recent ones. */
  loadDocument(path: string): Promise<LoadedDocument>
  /** The PDFs opened lately, the latest first, those still there. */
  recentDocuments(): Promise<RecentDocument[]>
  /** The document named on the command line, if any. */
  launchDocument(): Promise<string | undefined>
  /** Where a dropped file is. */
  pathOf(file: File): string
  /** Hands the helper the open document's text, and lists its annotations. */
  openDocument(path: string, name: string, pages: PageText[]): Promise<Annotation[]>
  closeDocument(): Promise<void>
  /** Draws pages for agents that look at them. */
  onRender(render: (request: RenderRequest) => Promise<Image>): () => void

  annotations(): Promise<Annotation[]>
  annotation(id: string): Promise<OpenedAnnotation>
  create(request: NewAnnotation): Promise<Annotation>
  send(annotation: string, text: string): Promise<void>
  /** Stops the annotation's reply. */
  stop(annotation: string): Promise<void>
  delete(annotation: string): Promise<void>
  /** Changes the model or thinking an annotation replies with next. */
  configure(annotation: string, model: ModelSelection, thinking: string): Promise<Annotation>

  folders(): Promise<Folders>
  /** Opens a folder in the system's file manager. */
  reveal(folder: keyof Folders): Promise<void>
  /** Opens an https link in the browser. */
  openExternal(url: string): Promise<void>
  onEvent(listener: (event: ReaderEvent) => void): () => void
}

export type ReaderResult<T> = { ok: true; value: T } | { ok: false; error: string }

export const READER_CHANNELS = {
  call: 'reader:call',
  event: 'reader:event',
  render: 'reader:render',
  rendered: 'reader:rendered',
  claudeLogin: 'reader:claude-login',
  pickDocument: 'reader:pick-document',
  loadDocument: 'reader:load-document',
  recentDocuments: 'reader:recent-documents',
  launchDocument: 'reader:launch-document',
  folders: 'reader:folders',
  reveal: 'reader:reveal',
  external: 'reader:external',
} as const
