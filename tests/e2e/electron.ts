import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron, expect, type ElectronApplication, type Page } from '@playwright/test'

export interface Launched {
  app: ElectronApplication
  page: Page
  /** The profile, which holds the app's data folder at `data/`. */
  profile: string
  /** Quits the app, and removes the profile unless it was given. */
  close(): Promise<void>
}

/** A profile for launches to share, as the same app started again. */
export const makeProfile = () => mkdtempSync(join(tmpdir(), 'erisreader-e2e-'))

/**
 * Starts the built app on a profile, fresh unless one is given, so nothing one
 * test changes carries into the next, with `args` after its own (a document to open), at 1440 × 900 and a unit scale factor, so
 * layout is in whole pixels whatever the desktop's text scaling is. The basic
 * password store keeps the desktop's keyring out of it.
 */
export async function launchApp({
  env = {},
  profile,
  args = [],
}: { env?: Record<string, string>; profile?: string; args?: string[] } = {}): Promise<Launched> {
  const fresh = profile === undefined
  profile ??= makeProfile()
  // On Linux `npm test` runs under xvfb-run, so X11 is the offscreen display; headed runs, and
  // other systems, use the desktop's own.
  const display = process.platform === 'linux' && !process.env.E2E_HEADED ? ['--ozone-platform=x11'] : []
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${profile}`, '--force-device-scale-factor=1', '--password-store=basic', ...display, ...args],
    env: { ...(process.env as Record<string, string>), ...env },
  })
  const page = await app.firstWindow()
  await resizeWindow(app, page, 1440, 900)
  return {
    app,
    page,
    profile,
    async close() {
      await app.close()
      if (fresh) rmSync(profile, { recursive: true, force: true })
    },
  }
}

export const viewport = (page: Page) => page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }))

export async function resizeWindow(app: ElectronApplication, page: Page, width: number, height: number): Promise<void> {
  await app.evaluate(({ BrowserWindow }, size) => BrowserWindow.getAllWindows()[0].setContentSize(size.width, size.height), {
    width,
    height,
  })
  await expect.poll(() => viewport(page)).toEqual({ width, height })
}
