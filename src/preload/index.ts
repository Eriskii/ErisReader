/**
 * The renderer's way to the main process: `window.erisreader`. Runs sandboxed, built to
 * CommonJS on its own (see vite.preload.config.ts).
 */
import { contextBridge, ipcRenderer, webUtils, type IpcRendererEvent } from 'electron'
import { READER_CHANNELS, type ReaderApi, type ReaderEvent, type ReaderResult, type RenderRequest } from '../shared/reader'

/** Takes a helper's reply, rejecting with its message when it failed. */
async function unwrap<T>(reply: Promise<ReaderResult<T>>): Promise<T> {
  const result = await reply
  if (!result.ok) throw new Error(result.error)
  return result.value
}

/** Runs a helper command. */
const agents = <T>(command: string, fields: Record<string, unknown> = {}): Promise<T> =>
  unwrap(ipcRenderer.invoke(READER_CHANNELS.call, { command, ...fields }))

const api: ReaderApi = {
  accounts: () => agents('accounts'),
  login: (provider, method) => agents('login', { provider, method }),
  addClaudeLogin: (method) => unwrap(ipcRenderer.invoke(READER_CHANNELS.claudeLogin, method)),
  cancelLogin: () => agents('cancel-login'),
  logout: (provider) => agents('logout', { provider }),
  models: () => agents('models'),
  types: () => agents('types'),

  pickDocument: () => ipcRenderer.invoke(READER_CHANNELS.pickDocument),
  loadDocument: (path) => ipcRenderer.invoke(READER_CHANNELS.loadDocument, path),
  recentDocuments: () => ipcRenderer.invoke(READER_CHANNELS.recentDocuments),
  launchDocument: () => ipcRenderer.invoke(READER_CHANNELS.launchDocument),
  pathOf: (file) => webUtils.getPathForFile(file),
  openDocument: (path, name, pages) => agents('open-document', { path, name, pages }),
  closeDocument: () => agents('close-document'),
  onRender(render) {
    const draw = async (_event: IpcRendererEvent, { ask, request }: { ask: number; request: RenderRequest }) => {
      try {
        ipcRenderer.send(READER_CHANNELS.rendered, { ask, image: await render(request) })
      } catch (error) {
        ipcRenderer.send(READER_CHANNELS.rendered, { ask, error: error instanceof Error ? error.message : String(error) })
      }
    }
    ipcRenderer.on(READER_CHANNELS.render, draw)
    return () => void ipcRenderer.off(READER_CHANNELS.render, draw)
  },

  annotations: () => agents('annotations'),
  annotation: (annotation) => agents('annotation', { annotation }),
  create: (request) => agents('create', { ...request }),
  send: (annotation, text) => agents('send', { annotation, text }),
  stop: (annotation) => agents('stop', { annotation }),
  delete: (annotation) => agents('delete', { annotation }),
  configure: (annotation, model, thinking) => agents('configure', { annotation, model, thinking }),

  folders: () => ipcRenderer.invoke(READER_CHANNELS.folders),
  reveal: (folder) => ipcRenderer.invoke(READER_CHANNELS.reveal, folder),
  openExternal: (url) => ipcRenderer.invoke(READER_CHANNELS.external, url),
  onEvent(listener) {
    const forward = (_event: IpcRendererEvent, value: ReaderEvent) => listener(value)
    ipcRenderer.on(READER_CHANNELS.event, forward)
    return () => void ipcRenderer.off(READER_CHANNELS.event, forward)
  },
}

contextBridge.exposeInMainWorld('erisreader', api)
