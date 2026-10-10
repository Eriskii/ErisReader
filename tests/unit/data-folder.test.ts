import { describe, expect, it } from 'vitest'
import { dataFolder } from '../../src/main/data-folder'

describe('dataFolder', () => {
  const home = '/home/eris'

  it('is Application Support on macOS', () => {
    expect(dataFolder({ platform: 'darwin', env: {}, home: '/Users/eris' })).toBe('/Users/eris/Library/Application Support/ErisReader')
  })

  it('follows the XDG base directories elsewhere', () => {
    expect(dataFolder({ platform: 'linux', env: {}, home })).toBe('/home/eris/.local/share/erisreader')
    expect(dataFolder({ platform: 'linux', env: { XDG_DATA_HOME: '/data' }, home })).toBe('/data/erisreader')
    expect(dataFolder({ platform: 'linux', env: { XDG_DATA_HOME: '' }, home })).toBe('/home/eris/.local/share/erisreader')
  })

  it('keeps a profile’s data with the profile, then honours ERISREADER_DATA', () => {
    const env = { ERISREADER_DATA: '/elsewhere' }
    expect(dataFolder({ platform: 'linux', env, home, profile: '/tmp/profile' })).toBe('/tmp/profile/data')
    expect(dataFolder({ platform: 'darwin', env, home })).toBe('/elsewhere')
  })
})
