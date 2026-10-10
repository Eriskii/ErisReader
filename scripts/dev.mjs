/**
 * Development: builds the agents helper, the main process and the preload
 * once, copies pdf.js's assets, serves the renderer from SvelteKit's dev server (hot reloading), and
 * starts Electron on it.
 */
import { spawn, spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { build, createServer } from 'vite'

const agents = spawnSync('npm', ['run', 'build:agents'], { stdio: 'inherit' })
if (agents.status !== 0) process.exit(agents.status ?? 1)
spawnSync('node', ['scripts/pdfjs-assets.mjs'], { stdio: 'inherit' })
await build({ configFile: 'vite.main.config.ts', mode: 'development' })
await build({ configFile: 'vite.preload.config.ts', mode: 'development' })

const server = await createServer({ configFile: 'vite.config.ts' })
await server.listen()
const [url] = server.resolvedUrls?.local ?? []
if (!url) throw new Error('The renderer dev server has no local URL')
server.printUrls()

// The electron package resolves its executable, honouring ELECTRON_OVERRIDE_DIST_PATH (see shell.nix).
const electron = createRequire(import.meta.url)('electron')
const child = spawn(electron, ['.'], { stdio: 'inherit', env: { ...process.env, ELECTRON_RENDERER_URL: url } })

child.on('exit', async (code) => {
  await server.close()
  process.exit(code ?? 0)
})
/** @type {NodeJS.Signals[]} */
const signals = ['SIGINT', 'SIGTERM']
for (const signal of signals) process.on(signal, () => child.kill(signal))
