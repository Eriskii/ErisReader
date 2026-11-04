import { describe, expect, it } from 'vitest'
import { shortcut } from '../../src/lib/reader/shortcut'

describe('shortcut', () => {
  it('names the command key as the system does', () => {
    expect(shortcut('K', 'MacIntel')).toBe('⌘K')
    expect(shortcut('Shift S', 'MacIntel')).toBe('⇧⌘S')
    expect(shortcut('K', 'Linux x86_64')).toBe('Ctrl K')
    expect(shortcut('Shift S', 'Linux x86_64')).toBe('Ctrl Shift S')
  })
})
