/** Which documents ErisReader opens without asking: the one a command line names, and the recent ones. */
import { existsSync, statSync } from 'node:fs'
import { basename, resolve } from 'node:path'
import type { RecentDocument } from '../shared/reader'

/** How many documents the welcome lists. */
const RECENT_LIMIT = 12

const isPdf = (path: string) => /\.pdf$/i.test(path)

/** The document a command line names: its last argument that is a PDF on disk. */
export function documentFromArgs(args: readonly string[], cwd: string): string | undefined {
  return args
    .filter((arg) => !arg.startsWith('-') && isPdf(arg))
    .map((arg) => resolve(cwd, arg))
    .filter((path) => existsSync(path) && statSync(path).isFile())
    .at(-1)
}

/** The recent list with `path` opened at `at`, the latest first. */
export function remember(recent: readonly RecentDocument[], path: string, at: number): RecentDocument[] {
  return [{ path, name: basename(path), opened: at }, ...recent.filter((other) => other.path !== path)].slice(0, RECENT_LIMIT)
}
