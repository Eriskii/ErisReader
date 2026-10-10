/**
 * PDFs through pdf.js: opening one, reading its pages' sizes and text, and drawing a page or a
 * region of one as an image for an agent. Its assets (CMaps, standard fonts, decoders) are
 * served from /pdfjs/, copied there by scripts/pdfjs-assets.mjs.
 */
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy, type PDFPageProxy } from 'pdfjs-dist'
import type { TextContent, TextItem } from 'pdfjs-dist/types/src/display/api'
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url'
import type { Image, PageRect, PageText } from '../../shared/reader'

GlobalWorkerOptions.workerSrc = workerUrl

/** CSS pixels per PDF point at 100%, as in a browser's PDF viewer. */
export const CSS_PER_POINT = 96 / 72

/** A page's size in points, as it is shown (after its rotation). */
export interface PageSize {
  width: number
  height: number
}

export interface PdfDocument {
  proxy: PDFDocumentProxy
  sizes: PageSize[]
  /** Each page's text, as pdf.js lays it out, for the text layer and for regions. */
  contents: TextContent[]
  /** Each page's text as agents read it, and its label. */
  text: PageText[]
}

/** The largest canvas drawn, in pixels: beyond it a page draws at a lower resolution. */
export const MAX_CANVAS_PIXELS = 16_777_216
/** How much a region may be magnified for an agent. */
const MAX_REGION_SCALE = 8
/** A base64 PNG longer than this goes as a JPEG instead, as photographs and scans would. */
const PNG_LIMIT = 3_500_000

const isText = (item: TextContent['items'][number]): item is TextItem => 'str' in item

/** A page's text as agents read it: its runs, a line break where pdf.js ends a line. */
export function pageText(content: TextContent): string {
  return content.items
    .filter(isText)
    .map((item) => item.str + (item.hasEOL ? '\n' : ''))
    .join('')
    .replace(/[ \t]+\n/g, '\n')
    .trim()
}

/** Opens a PDF and reads every page's size and text. */
export async function openPdf(data: Uint8Array): Promise<PdfDocument> {
  const proxy = await getDocument({
    data,
    cMapUrl: '/pdfjs/cmaps/',
    cMapPacked: true,
    standardFontDataUrl: '/pdfjs/standard_fonts/',
    wasmUrl: '/pdfjs/wasm/',
    iccUrl: '/pdfjs/iccs/',
  }).promise
  const labels = await proxy.getPageLabels().catch(() => null)
  const pages = await Promise.all(
    Array.from({ length: proxy.numPages }, async (_, index) => {
      const page = await proxy.getPage(index + 1)
      const { width, height } = page.getViewport({ scale: 1 })
      const content = await page.getTextContent()
      return { size: { width, height }, content, text: { label: labels?.[index] ?? null, text: pageText(content) } }
    }),
  )
  return {
    proxy,
    sizes: pages.map((page) => page.size),
    contents: pages.map((page) => page.content),
    text: pages.map((page) => page.text),
  }
}

/** The text inside a region of a page: the runs whose middle lies within it, line by line. */
export function regionText(page: PDFPageProxy, content: TextContent, rect: PageRect): string {
  const viewport = page.getViewport({ scale: 1 })
  const lines: string[] = []
  let line = ''
  for (const item of content.items.filter(isText)) {
    const [x, baseline] = viewport.convertToViewportPoint(item.transform[4], item.transform[5]) as [number, number]
    const middle = { x: (x + item.width / 2) / viewport.width, y: (baseline - item.height / 2) / viewport.height }
    const inside =
      middle.x >= rect.x && middle.x <= rect.x + rect.width && middle.y >= rect.y && middle.y <= rect.y + rect.height
    if (inside) line += item.str
    if (item.hasEOL && line.trim()) {
      lines.push(line.trim())
      line = ''
    }
  }
  if (line.trim()) lines.push(line.trim())
  return lines.join('\n')
}

/** A canvas's picture as base64: a PNG, or a JPEG when the PNG would be too large. */
function encode(canvas: HTMLCanvasElement): Image {
  const png = canvas.toDataURL('image/png').split(',')[1]
  if (png.length <= PNG_LIMIT) return { mime: 'image/png', data: png }
  return { mime: 'image/jpeg', data: canvas.toDataURL('image/jpeg', 0.88).split(',')[1] }
}

/** Draws a part of a page, `rect` in fractions of it, with its longer side `longest` pixels at most. */
export async function drawRegion(page: PDFPageProxy, rect: Omit<PageRect, 'page'>, longest: number): Promise<HTMLCanvasElement> {
  const unit = page.getViewport({ scale: 1 })
  const span = Math.max(rect.width * unit.width, rect.height * unit.height)
  const scale = Math.min(longest / span, MAX_REGION_SCALE)
  const full = page.getViewport({ scale })
  const viewport = page.getViewport({ scale, offsetX: -rect.x * full.width, offsetY: -rect.y * full.height })
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(rect.width * full.width))
  canvas.height = Math.max(1, Math.round(rect.height * full.height))
  await page.render({ canvas, viewport }).promise
  return canvas
}

/** A page as an agent sees it, its longer side `longest` pixels. */
export async function pageImage(page: PDFPageProxy, longest: number): Promise<Image> {
  return encode(await drawRegion(page, { x: 0, y: 0, width: 1, height: 1 }, longest))
}

/** A region of a page as an agent sees it. */
export async function regionImage(page: PDFPageProxy, rect: PageRect, longest: number): Promise<Image> {
  return encode(await drawRegion(page, rect, longest))
}

export const dataUrl = (image: Image) => `data:${image.mime};base64,${image.data}`
