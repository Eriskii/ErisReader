/**
 * Copies pdf.js's runtime assets (CMaps, standard fonts, image decoders, ICC profiles) into
 * static/pdfjs/, where the renderer serves them; see src/lib/document/pdf.ts.
 */
import { cpSync, mkdirSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const root = dirname(createRequire(import.meta.url).resolve('pdfjs-dist/package.json'))
const target = join(import.meta.dirname, '..', 'static', 'pdfjs')
rmSync(target, { recursive: true, force: true })
mkdirSync(target, { recursive: true })
for (const folder of ['cmaps', 'standard_fonts', 'wasm', 'iccs']) cpSync(join(root, folder), join(target, folder), { recursive: true })
