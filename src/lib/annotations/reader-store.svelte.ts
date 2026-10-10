/**
 * Everything the window knows: the accounts and their models, the agent types, the open
 * document and its annotations, and the open annotation's conversation as it streams in, or
 * the draft of a new one. One store, shared by the reading view, the chat in the margin, the
 * list of annotations and settings.
 */
import type {
  Account,
  AgentType,
  AgentTypes,
  Anchor,
  Annotation,
  Entry,
  Held,
  Image,
  Model,
  ModelSelection,
  ProviderId,
  ReaderApi,
  ReaderEvent,
  RecentDocument,
} from '../../shared/reader'
import { openPdf, pageImage, type PdfDocument } from '../document/pdf'
import { loadChoice, pickModel, pickThinking, saveChoice, sameModel, type NewAnnotationChoice } from './choices'

/** The document the app reopens on start, until it is closed. */
const OPEN_KEY = 'erisreader.document'

/** What went wrong, without Electron's wrapping. */
export const message = (error: unknown): string =>
  (error instanceof Error ? error.message : String(error)).replace(/^Error invoking remote method '[^']+': (Error: )?/, '')

/** A document being read. */
export interface OpenDocument {
  path: string
  name: string
  pdf: PdfDocument
}

/** An annotation not yet made: where it would be, and a region's picture. */
export interface Draft {
  anchor: Anchor
  image: Image | null
}

/** Where annotations go: in the order they come in the document. */
function place(annotation: { anchor: Anchor }): [number, number] {
  const first = annotation.anchor.kind === 'text' ? annotation.anchor.rects[0] : annotation.anchor.rect
  return [first?.page ?? 0, first?.y ?? 0]
}

const inOrder = (annotations: Annotation[]) =>
  [...annotations].sort((a, b) => {
    const [[pa, ya], [pb, yb]] = [place(a), place(b)]
    return pa - pb || ya - yb
  })

const remember = (path: string | undefined) => {
  try {
    if (path) localStorage.setItem(OPEN_KEY, path)
    else localStorage.removeItem(OPEN_KEY)
  } catch {
    // The next start shows the welcome.
  }
}

export class ReaderStore {
  readonly api: ReaderApi

  /** Undefined until first read. */
  accounts = $state.raw<Account[]>()
  /** Why signing in to an account failed, by provider. */
  accountProblems = $state<Partial<Record<ProviderId, string>>>({})
  models = $state.raw<Model[]>()
  modelsProblem = $state<string>()
  types = $state.raw<AgentTypes>()
  typesProblem = $state<string>()

  document = $state.raw<OpenDocument>()
  /** The document being opened. */
  opening = $state<string>()
  /** Why the last document could not open. */
  documentProblem = $state<string>()
  recent = $state.raw<RecentDocument[]>([])

  annotations = $state.raw<Annotation[]>([])
  /** The annotation whose chat is open. */
  current = $state<string>()
  /** A new annotation being written, whose chat is open. */
  draft = $state.raw<Draft>()
  /** The open annotation's conversation. */
  entries = $state.raw<Entry[]>([])
  loading = $state(false)
  /** Each running annotation's reply so far, before it is finished. */
  streams = $state<Record<string, string>>({})
  held = $state<Record<string, Held>>({})
  /** What the last action could not do. */
  problem = $state<string>()
  newChoice = $state<NewAnnotationChoice>(loadChoice(localStorage))
  /** Unsent text, by annotation; a draft's under ''. */
  drafts = $state<Record<string, string>>({})
  /** Whether the list of annotations is open. */
  listOpen = $state(false)

  constructor(api: ReaderApi) {
    this.api = api
  }

  get signedIn(): boolean {
    return this.accounts?.some((account) => account.status === 'connected') === true
  }

  get annotation(): Annotation | undefined {
    return this.annotations.find((annotation) => annotation.id === this.current)
  }

  /** Where the open chat's annotation is. */
  get anchor(): Anchor | undefined {
    return this.draft?.anchor ?? this.annotation?.anchor
  }

  get chatOpen(): boolean {
    return this.anchor !== undefined
  }

  /** The agent type a new annotation would talk to. */
  get newType(): AgentType | undefined {
    const types = this.types?.types ?? []
    return types.find((type) => type.id === this.newChoice.type) ?? types.find((type) => type.id === this.types?.defaultType)
  }

  /** The model a new annotation would use. */
  get newModel(): Model | undefined {
    return pickModel(this.models ?? [], this.newType, this.newChoice.model)
  }

  get newThinking(): string {
    return pickThinking(this.newModel, this.newChoice.thinking, this.newType?.thinkingLevel)
  }

  modelOf(selection: ModelSelection | undefined): Model | undefined {
    return this.models?.find((model) => sameModel(model, selection))
  }

  /** Loads what settings and the welcome show, and reopens the document named on the command line or left open. */
  async start(): Promise<void> {
    await Promise.all([this.loadAccounts(), this.loadTypes(), this.loadRecent()])
    let left: string | null = null
    try {
      left = localStorage.getItem(OPEN_KEY)
    } catch {
      // Show the welcome.
    }
    const path = (await this.api.launchDocument()) ?? left
    if (path) await this.openDocument(path)
  }

  async loadAccounts(): Promise<void> {
    try {
      this.accounts = await this.api.accounts()
      if (this.signedIn) await this.loadModels()
    } catch (error) {
      this.problem = message(error)
    }
  }

  async loadModels(): Promise<void> {
    try {
      this.models = await this.api.models()
      this.modelsProblem = undefined
    } catch (error) {
      this.modelsProblem = message(error)
    }
  }

  async loadTypes(): Promise<void> {
    try {
      this.types = await this.api.types()
      this.typesProblem = undefined
    } catch (error) {
      this.typesProblem = message(error)
    }
  }

  async loadRecent(): Promise<void> {
    this.recent = await this.api.recentDocuments()
  }

  /** Asks for a PDF and opens it. */
  async pickDocument(): Promise<void> {
    const path = await this.api.pickDocument()
    if (path) await this.openDocument(path)
  }

  /** Opens a PDF: reads it, lays out its pages and text, and hands the text to the agents with its annotations. */
  async openDocument(path: string): Promise<void> {
    this.opening = path
    this.documentProblem = undefined
    try {
      const loaded = await this.api.loadDocument(path)
      const pdf = await openPdf(loaded.data)
      const annotations = await this.api.openDocument(loaded.path, loaded.name, pdf.text)
      if (this.opening !== path) {
        void pdf.proxy.loadingTask.destroy()
        return
      }
      const before = this.document
      this.#leave()
      this.document = { path: loaded.path, name: loaded.name, pdf }
      this.annotations = inOrder(annotations)
      remember(loaded.path)
      void before?.pdf.proxy.loadingTask.destroy()
    } catch (error) {
      this.documentProblem = `${path} could not be opened: ${message(error)}`
      if (!this.document) remember(undefined)
    } finally {
      if (this.opening === path) this.opening = undefined
    }
    void this.loadRecent()
  }

  /** Closes the document, back to the welcome. */
  async closeDocument(): Promise<void> {
    const before = this.document
    if (!before) return
    this.#leave()
    this.document = undefined
    remember(undefined)
    await this.api.closeDocument()
    void before.pdf.proxy.loadingTask.destroy()
    void this.loadRecent()
  }

  /** Forgets the open document's annotations and chat. */
  #leave(): void {
    this.annotations = []
    this.current = undefined
    this.draft = undefined
    this.entries = []
    this.streams = {}
    this.held = {}
    this.drafts = {}
  }

  /** Draws a page for an agent that looks at it. */
  async render(document: string, page: number, longest: number): Promise<Image> {
    if (document !== this.document?.path) throw new Error('That document is no longer open.')
    return pageImage(await this.document.pdf.proxy.getPage(page + 1), longest)
  }

  /** Opens an annotation's chat. */
  async open(id: string): Promise<void> {
    if (this.current === id && !this.draft) return
    this.draft = undefined
    this.current = id
    this.entries = []
    this.loading = true
    try {
      const opened = await this.api.annotation(id)
      if (this.current !== id) return
      // Entries that streamed in while the transcript was read come after it.
      const last = opened.entries.at(-1)?.seq ?? -1
      this.entries = [...opened.entries, ...this.entries.filter((entry) => entry.seq > last)]
      this.#merge(opened.annotation)
    } catch (error) {
      this.problem = message(error)
    } finally {
      if (this.current === id) this.loading = false
    }
  }

  /** Opens the chat of an annotation not yet made. */
  startDraft(anchor: Anchor, image: Image | null = null): void {
    this.current = undefined
    this.entries = []
    this.loading = false
    this.draft = { anchor, image }
    this.drafts[''] = ''
    void this.loadTypes()
  }

  /** Puts the chat away; a draft is dropped. */
  closeChat(): void {
    this.current = undefined
    this.draft = undefined
    this.entries = []
    this.loading = false
  }

  async send(text: string): Promise<boolean> {
    if (!text.trim()) return false
    this.problem = undefined
    try {
      const current = this.current
      if (current) {
        await this.api.send(current, text)
        return true
      }
      const draft = this.draft
      const model = this.newModel
      const type = this.newType
      if (!draft || !model || !type) return false
      const thinking = this.newThinking
      this.newChoice = { type: type.id, model: { provider: model.provider, id: model.id }, thinking }
      saveChoice(localStorage, $state.snapshot(this.newChoice))
      const annotation = await this.api.create({
        anchor: draft.anchor,
        type: type.id,
        model: { provider: model.provider, id: model.id },
        thinking,
        text,
        image: draft.image,
      })
      this.#merge(annotation)
      if (this.draft === draft) await this.open(annotation.id)
      return true
    } catch (error) {
      this.problem = message(error)
      return false
    }
  }

  async stop(): Promise<void> {
    if (this.current) await this.api.stop(this.current)
  }

  async delete(id: string): Promise<void> {
    try {
      await this.api.delete(id)
      this.annotations = this.annotations.filter((other) => other.id !== id)
      if (this.current === id) this.closeChat()
    } catch (error) {
      this.problem = message(error)
    }
  }

  /** Chooses the model or thinking: the open annotation's for its next reply, or a new one's. */
  async choose(model: ModelSelection, thinking: string): Promise<void> {
    const annotation = this.annotation
    if (!annotation) {
      this.newChoice = { ...this.newChoice, model, thinking: pickThinking(this.modelOf(model), thinking) }
      saveChoice(localStorage, $state.snapshot(this.newChoice))
      return
    }
    try {
      this.#merge(await this.api.configure(annotation.id, model, pickThinking(this.modelOf(model), thinking)))
    } catch (error) {
      this.problem = message(error)
    }
  }

  /** Chooses a new annotation's type, and with it the model and thinking it declares. */
  chooseType(type: string): void {
    const declared = this.types?.types.find((other) => other.id === type)
    this.newChoice = {
      type,
      model: declared?.model ?? this.newChoice.model,
      thinking: declared?.thinkingLevel ?? this.newChoice.thinking,
    }
    saveChoice(localStorage, $state.snapshot(this.newChoice))
  }

  async login(provider: 'openai', method: 'browser' | 'device-code'): Promise<void> {
    this.accountProblems = { ...this.accountProblems, [provider]: undefined }
    try {
      this.#account(await this.api.login(provider, method))
    } catch (error) {
      this.accountProblems = { ...this.accountProblems, [provider]: message(error) }
    }
  }

  async addClaudeLogin(method: 'credentials-file' | 'keychain'): Promise<void> {
    this.accountProblems = { ...this.accountProblems, anthropic: undefined }
    try {
      const account = await this.api.addClaudeLogin(method)
      if (account) this.#account(account)
    } catch (error) {
      this.accountProblems = { ...this.accountProblems, anthropic: message(error) }
    }
  }

  async cancelLogin(): Promise<void> {
    await this.api.cancelLogin()
  }

  async logout(provider: ProviderId): Promise<void> {
    try {
      await this.api.logout(provider)
    } catch (error) {
      this.accountProblems = { ...this.accountProblems, [provider]: message(error) }
    }
  }

  #account(account: Account): void {
    const before = this.accounts?.find((other) => other.providerId === account.providerId)
    this.accounts = (this.accounts ?? []).map((other) => (other.providerId === account.providerId ? account : other))
    // Signing in or out changes which models there are.
    if ((before?.status === 'connected') !== (account.status === 'connected')) {
      if (this.signedIn) void this.loadModels()
      else this.models = undefined
    }
  }

  /**
   * An annotation as a reply gives it. Events report its state and usage in order, and a reply
   * may be older than the latest of them, so a listed annotation keeps those.
   */
  #merge(annotation: Annotation): void {
    const listed = this.annotations.find((other) => other.id === annotation.id)
    this.#upsert(
      listed
        ? {
            ...annotation,
            state: listed.state,
            error: listed.error,
            usage: listed.usage,
            contextTokens: listed.contextTokens,
            updated: Math.max(annotation.updated, listed.updated),
          }
        : annotation,
    )
  }

  #upsert(annotation: Annotation): void {
    this.annotations = inOrder([annotation, ...this.annotations.filter((other) => other.id !== annotation.id)])
    if (annotation.state !== 'running') {
      delete this.streams[annotation.id]
      delete this.held[annotation.id]
    }
  }

  handle(event: ReaderEvent): void {
    if ('document' in event && event.document !== this.document?.path) return
    switch (event.event) {
      case 'account':
        this.#account(event.account)
        break
      case 'login-completed':
        this.accountProblems = {
          ...this.accountProblems,
          [event.providerId]: event.ok ? undefined : (event.error ?? 'Signing in failed.'),
        }
        break
      case 'annotation':
        this.#upsert(event.annotation)
        break
      case 'entry':
        if (event.entry.kind === 'assistant') delete this.streams[event.annotation]
        if (event.annotation === this.current && !this.entries.some((entry) => entry.seq === event.entry.seq)) {
          this.entries = [...this.entries, event.entry]
        }
        break
      case 'delta':
        this.streams[event.annotation] = (this.streams[event.annotation] ?? '') + event.text
        break
      case 'held':
        this.held[event.annotation] = event.held
        break
      case 'ended':
        this.problem = event.error
        break
    }
  }
}
