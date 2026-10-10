import { join } from 'node:path'

export interface Place {
  platform: NodeJS.Platform
  env: Record<string, string | undefined>
  home: string
  /** Chromium's --user-data-dir, when the app was started on a profile of its own. */
  profile?: string
}

/**
 * Where ErisReader keeps its agent types, logins and recent documents: the system's place for an
 * application's data. A profile named on the command line keeps its own, as the tests start
 * the app; ERISREADER_DATA names another.
 */
export function dataFolder({ platform, env, home, profile }: Place): string {
  if (profile) return join(profile, 'data')
  if (env.ERISREADER_DATA) return env.ERISREADER_DATA
  if (platform === 'darwin') return join(home, 'Library', 'Application Support', 'ErisReader')
  return join(env.XDG_DATA_HOME || join(home, '.local', 'share'), 'erisreader')
}
