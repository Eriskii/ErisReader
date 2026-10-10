/**
 * The PDFs ErisReader opens: picked in the system's dialog, dropped on the window, or named on
 * the command line. The main process reads them for the renderer and remembers the recent ones
 * in the data folder's `recent.json`.
 */
import { existsSync } from 'node:fs'
import { readFile, writeFile } from 'node:fs/promises'
import { basename, isAbsolute, join } from 'node:path'
import { BrowserWindow, dialog, ipcMain } from 'electron'
import { READER_CHANNELS, type LoadedDocument, type RecentDocument } from '../shared/reader'
import { remember } from './recent'

export function serveDocuments(data: string, launched: string | undefined): void {
  const recentFile = join(data, 'recent.json')

  const readRecent = async (): Promise<RecentDocument[]> => {
    try {
      const saved: unknown = JSON.parse(await readFile(recentFile, 'utf8'))
      if (!Array.isArray(saved)) return []
      return saved.filter(
        (entry): entry is RecentDocument =>
          typeof entry?.path === 'string' && typeof entry?.name === 'string' && typeof entry?.opened === 'number',
      )
    } catch {
      return []
    }
  }

  ipcMain.handle(READER_CHANNELS.pickDocument, async (event) => {
    const window = BrowserWindow.fromWebContents(event.sender)
    const options: Electron.OpenDialogOptions = {
      title: 'Open a PDF',
      buttonLabel: 'Open',
      properties: ['openFile'],
      filters: [{ name: 'PDF documents', extensions: ['pdf'] }],
    }
    const picked = window ? await dialog.showOpenDialog(window, options) : await dialog.showOpenDialog(options)
    return picked.canceled ? undefined : picked.filePaths[0]
  })

  ipcMain.handle(READER_CHANNELS.loadDocument, async (_event, path: unknown): Promise<LoadedDocument> => {
    if (typeof path !== 'string' || !isAbsolute(path)) throw new Error('A document opens by its full path.')
    const data = new Uint8Array(await readFile(path))
    try {
      await writeFile(recentFile, `${JSON.stringify(remember(await readRecent(), path, Date.now()), null, 2)}\n`)
    } catch (error) {
      console.error('erisreader: remembering the document:', error)
    }
    return { path, name: basename(path), data }
  })

  ipcMain.handle(READER_CHANNELS.recentDocuments, async () => (await readRecent()).filter((entry) => existsSync(entry.path)))

  ipcMain.handle(READER_CHANNELS.launchDocument, () => launched)
}
