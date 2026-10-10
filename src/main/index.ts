import { homedir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { app, BrowserWindow, Menu, net, protocol, shell } from 'electron'
import { READER_HEX } from '../shared/palette'
import { isHttps, serveAgents } from './agents'
import { APP_SCHEME, APP_URL, appFile, appSchemePrivileges } from './app-scheme'
import { dataFolder } from './data-folder'
import { serveDocuments } from './documents'
import { documentFromArgs } from './recent'

// Chromium's --user-data-dir names the profile, as in a browser: settings and caches live there,
// and so does the data of a profile started on purpose.
const profile = app.commandLine.getSwitchValue('user-data-dir') || undefined
if (profile) app.setPath('userData', profile)

protocol.registerSchemesAsPrivileged([appSchemePrivileges])

/** The SvelteKit build (see vite.config.ts). */
const RENDERER_ROOT = join(import.meta.dirname, '../renderer')

/** The preload build (see vite.preload.config.ts). */
const PRELOAD = join(import.meta.dirname, '../preload/index.cjs')

function serveApp(): void {
  protocol.handle(APP_SCHEME, (request) => {
    const file = appFile(RENDERER_ROOT, request.url)
    return file ? net.fetch(pathToFileURL(file).toString()) : new Response('Not found', { status: 404 })
  })
}

/**
 * The menu keeps editing and developer keys, and leaves zooming to the reader: Ctrl + and
 * Ctrl − scale the document, not the window, and Ctrl W closes the document.
 */
function setMenu(): void {
  const mac = process.platform === 'darwin'
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      ...(mac ? [{ role: 'appMenu' as const }] : []),
      { role: 'editMenu' },
      { label: 'View', submenu: [{ role: 'reload' }, { role: 'toggleDevTools' }, { role: 'togglefullscreen' }] },
      ...(mac ? [{ label: 'Window', submenu: [{ role: 'minimize' as const }] }] : []),
    ]),
  )
}

function createWindow(): void {
  const window = new BrowserWindow({
    width: 1280,
    height: 900,
    minWidth: 640,
    minHeight: 480,
    show: false,
    title: 'ErisReader',
    backgroundColor: READER_HEX,
    autoHideMenuBar: true,
    webPreferences: { sandbox: true, contextIsolation: true, preload: PRELOAD },
  })

  window.once('ready-to-show', () => window.show())

  // Links in replies open in the system browser; the app window stays on the app.
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (isHttps(url)) void shell.openExternal(url)
    return { action: 'deny' }
  })
  window.webContents.on('will-navigate', (event, url) => {
    event.preventDefault()
    if (isHttps(url)) void shell.openExternal(url)
  })

  // `npm run dev` serves the renderer from SvelteKit's dev server.
  void window.loadURL(process.env.ELECTRON_RENDERER_URL ?? APP_URL)
}

void app.whenReady().then(() => {
  const data = dataFolder({ platform: process.platform, env: process.env, home: homedir(), profile })
  serveApp()
  setMenu()
  serveAgents({ data, agents: join(data, 'agents') })
  serveDocuments(data, documentFromArgs(process.argv.slice(1), process.cwd()))
  createWindow()
})

app.on('window-all-closed', () => app.quit())
