/**
 * Runs the e2e tests. On Linux they get an offscreen X display from xvfb-run (see shell.nix);
 * elsewhere Electron opens its windows on the desktop.
 */
import { spawnSync } from 'node:child_process'

const [command, ...args] =
  process.platform === 'linux' ? ['xvfb-run', '-a', '-s', '-screen 0 1920x1080x24', 'npx', 'playwright', 'test'] : ['npx', 'playwright', 'test']
const { status } = spawnSync(command, [...args, ...process.argv.slice(2)], { stdio: 'inherit' })
process.exit(status ?? 1)
