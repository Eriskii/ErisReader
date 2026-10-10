import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { appFile } from '../../src/main/app-scheme'

const root = '/opt/erisreader/out/renderer'

describe('appFile', () => {

  it('maps the root to index.html, ignoring any query or fragment', () => {
    expect(appFile(root, 'erisreader://app/')).toBe(join(root, 'index.html'))
    expect(appFile(root, 'erisreader://app/?view=1#top')).toBe(join(root, 'index.html'))
  })

  it('maps other paths to files under the root', () => {
    expect(appFile(root, 'erisreader://app/_app/immutable/entry/start.js')).toBe(join(root, '_app/immutable/entry/start.js'))
    expect(appFile(root, 'erisreader://app/fonts/a%20b.woff2')).toBe(join(root, 'fonts/a b.woff2'))
  })

  it('resolves dot segments at the root, as on the web', () => {
    expect(appFile(root, 'erisreader://app/%2e%2e/%2e%2e/etc/passwd')).toBe(join(root, 'etc/passwd'))
  })

  it('refuses paths that escape the root through encoded slashes', () => {
    expect(appFile(root, 'erisreader://app/..%2f..%2fetc%2fpasswd')).toBeUndefined()
    expect(appFile(root, 'erisreader://app/%2e%2e%2f%2e%2e%2fetc%2fpasswd')).toBeUndefined()
  })

  it('serves nothing for another host', () => {
    expect(appFile(root, 'erisreader://elsewhere/index.html')).toBeUndefined()
  })
})
