import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, describe, expect, it } from 'vitest'
import { documentFromArgs, remember } from '../../src/main/recent'

const folder = mkdtempSync(join(tmpdir(), 'erisreader-recent-'))
afterAll(() => rmSync(folder, { recursive: true, force: true }))

describe('documentFromArgs', () => {
  it('takes the last PDF on disk the command line names, relative to where it ran', () => {
    writeFileSync(join(folder, 'a.pdf'), '')
    writeFileSync(join(folder, 'b.PDF'), '')
    mkdirSync(join(folder, 'folder.pdf'))
    expect(documentFromArgs(['.', '--user-data-dir=/x.pdf', 'a.pdf', 'b.PDF', 'missing.pdf', 'folder.pdf'], folder)).toBe(join(folder, 'b.PDF'))
    expect(documentFromArgs(['.', 'notes.txt'], folder)).toBeUndefined()
  })
})

describe('remember', () => {
  it('puts the document first, once, and keeps a dozen', () => {
    const recent = Array.from({ length: 12 }, (_, i) => ({ path: `/d/${i}.pdf`, name: `${i}.pdf`, opened: i }))
    const next = remember(recent, '/d/5.pdf', 100)
    expect(next[0]).toEqual({ path: '/d/5.pdf', name: '5.pdf', opened: 100 })
    expect(next.filter((entry) => entry.path === '/d/5.pdf')).toHaveLength(1)
    expect(remember(recent, '/d/new.pdf', 100)).toHaveLength(12)
  })
})
