import { join, resolve, sep } from 'node:path'
import type { CustomScheme } from 'electron'

/**
 * The renderer, a static SvelteKit build, is served at erisreader://app/: a
 * standard, secure origin, so its router, workers and storage behave as on
 * the web, which a file:// page does not.
 */
export const APP_SCHEME = 'erisreader'
export const APP_HOST = 'app'
export const APP_URL = `${APP_SCHEME}://${APP_HOST}/`

export const appSchemePrivileges: CustomScheme = {
  scheme: APP_SCHEME,
  privileges: { standard: true, secure: true, supportFetchAPI: true },
}

/** The file under `root` an erisreader://app/ URL names, the root itself naming index.html; nothing outside `root`. */
export function appFile(root: string, url: string): string | undefined {
  const { host, pathname } = new URL(url)
  if (host !== APP_HOST) return undefined
  let path: string
  try {
    path = decodeURIComponent(pathname)
  } catch {
    return undefined
  }
  const base = resolve(root)
  const file = resolve(base, `.${path === '/' ? '/index.html' : path}`)
  return file.startsWith(base + sep) ? join(file) : undefined
}
