/**
 * Starts the agents helper (agents/, built to out/agents) and relays between it and the
 * renderer: the renderer's calls go to it as commands, its events go to every window, and its
 * agents' requests to look at a page go to the renderer, which draws them. Opening files,
 * folders and pages stays here, with the main process.
 */
import { spawn, type ChildProcess } from 'node:child_process'
import { readFile, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron'
import { READER_CHANNELS, type Account, type Folders, type ReaderResult } from '../shared/reader'

const HELPER = join(import.meta.dirname, '../agents/erisreader-agents')

/** A Claude Code credentials file is a few hundred bytes. */
const CREDENTIALS_LIMIT = 64 * 1024

type Reply = ReaderResult<unknown>

export const isHttps = (url: string): boolean => {
  try {
    return new URL(url).protocol === 'https:'
  } catch {
    return false
  }
}

export function serveAgents(folders: Folders): void {
  let helper: ChildProcess | undefined
  let next = 1
  const replies = new Map<number, (reply: Reply) => void>()

  const broadcast = (value: unknown) => {
    for (const window of BrowserWindow.getAllWindows()) window.webContents.send(READER_CHANNELS.event, value)
  }

  const write = (message: unknown) => helper?.stdin?.write(`${JSON.stringify(message)}\n`)

  /** Asks the window to draw a page; with no window, there is nothing to draw it. */
  const render = (ask: number, request: unknown) => {
    const [window] = BrowserWindow.getAllWindows()
    if (window) window.webContents.send(READER_CHANNELS.render, { ask, request })
    else write({ answer: ask, error: 'ErisReader has no window to draw the page in.' })
  }

  const start = (): ChildProcess => {
    if (helper) return helper
    const started = spawn(HELPER, ['--dir', folders.data], { stdio: ['pipe', 'pipe', 'inherit'] })
    createInterface({ input: started.stdout! }).on('line', (line) => {
      let message: Record<string, unknown>
      try {
        message = JSON.parse(line)
      } catch {
        return
      }
      if (typeof message.reply === 'number') {
        const answer = replies.get(message.reply)
        replies.delete(message.reply)
        answer?.(message.ok ? { ok: true, value: message.value } : { ok: false, error: String(message.error) })
      } else if (typeof message.ask === 'number' && message.render) {
        render(message.ask, message.render)
      } else if (typeof message.event === 'string') {
        broadcast(message)
      }
    })
    const ended = (error: string) => {
      if (helper !== started) return
      helper = undefined
      for (const answer of replies.values()) answer({ ok: false, error })
      replies.clear()
      broadcast({ event: 'ended', error })
    }
    started.on('error', (error) => ended(`The agents helper could not start: ${error.message}`))
    started.on('exit', (code, signal) => ended(`The agents helper stopped (${signal ?? code}).`))
    helper = started
    return started
  }

  const call = (command: Record<string, unknown>) =>
    new Promise<Reply>((resolve) => {
      const id = next++
      start()
      replies.set(id, resolve)
      write({ ...command, id })
    })

  ipcMain.on(READER_CHANNELS.rendered, (_event, answer: unknown) => {
    if (typeof answer !== 'object' || answer === null || typeof (answer as { ask?: unknown }).ask !== 'number') return
    const { ask, image, error } = answer as { ask: number; image?: unknown; error?: unknown }
    write(image ? { answer: ask, image } : { answer: ask, error: typeof error === 'string' ? error : 'The page could not be drawn.' })
  })

  ipcMain.handle(READER_CHANNELS.call, async (_event, command: unknown) => {
    if (typeof command !== 'object' || command === null || typeof (command as { command?: unknown }).command !== 'string') {
      return { ok: false, error: 'Malformed command.' }
    }
    const reply = await call(command as Record<string, unknown>)
    // A browser sign-in finishes in the browser.
    if ((command as { command: string; method?: string }).command === 'login' && reply.ok) {
      const url = (reply.value as Account).login?.url
      if ((command as { method?: string }).method === 'browser' && url && /^https?:\/\//.test(url)) void shell.openExternal(url)
    }
    return reply
  })

  ipcMain.handle(READER_CHANNELS.claudeLogin, async (event, method: unknown): Promise<Reply> => {
    if (method === 'keychain') return call({ command: 'login', provider: 'anthropic', method })
    if (method !== 'credentials-file') return { ok: false, error: 'Malformed command.' }
    const window = BrowserWindow.fromWebContents(event.sender)
    const options: Electron.OpenDialogOptions = {
      title: 'Add a Claude login',
      buttonLabel: 'Add login',
      defaultPath: join(homedir(), '.claude'),
      properties: ['openFile', 'showHiddenFiles'],
      filters: [{ name: 'Claude Code credentials', extensions: ['json'] }],
    }
    const picked = window ? await dialog.showOpenDialog(window, options) : await dialog.showOpenDialog(options)
    const [path] = picked.filePaths
    if (picked.canceled || !path) return { ok: true, value: undefined }
    if ((await stat(path)).size > CREDENTIALS_LIMIT) return { ok: false, error: 'That file is too large to be a Claude login.' }
    return call({ command: 'login', provider: 'anthropic', method, text: await readFile(path, 'utf8') })
  })

  ipcMain.handle(READER_CHANNELS.folders, () => folders)

  ipcMain.handle(READER_CHANNELS.reveal, async (_event, folder: unknown) => {
    if (folder === 'data' || folder === 'agents') await shell.openPath(folders[folder])
  })

  ipcMain.handle(READER_CHANNELS.external, async (_event, url: unknown) => {
    if (typeof url === 'string' && isHttps(url)) await shell.openExternal(url)
  })

  app.on('will-quit', () => helper?.stdin?.end())
}
