import { writeFileSync } from 'node:fs'

/** US Letter, in points. */
export const PAGE = { width: 612, height: 792 }
/** Where the first line's baseline sits, from the page's top, and how far apart lines are. */
export const FIRST_LINE = 72
export const LINE = 24
export const FONT_SIZE = 14

const escape = (text: string) => text.replace(/[\\()]/g, (c) => `\\${c}`)

/**
 * A PDF whose pages each hold lines of Helvetica text, 72 pt in from the left and
 * starting 72 pt down from the top: enough for pdf.js to render and lay out a text layer.
 */
export function pdf(pages: string[][]): Buffer {
  const objects: string[] = []
  const add = (body: string) => objects.push(body)

  // 1: catalog, 2: pages, 3: font, then each page and its contents.
  add('<< /Type /Catalog /Pages 2 0 R >>')
  const kids = pages.map((_, index) => `${4 + index * 2} 0 R`).join(' ')
  add(`<< /Type /Pages /Kids [${kids}] /Count ${pages.length} >>`)
  add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>')
  for (const [index, lines] of pages.entries()) {
    const content = [
      'BT',
      `/F1 ${FONT_SIZE} Tf`,
      `${LINE} TL`,
      `72 ${PAGE.height - FIRST_LINE} Td`,
      ...lines.map((line) => `(${escape(line)}) Tj T*`),
      'ET',
    ].join('\n')
    add(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${PAGE.width} ${PAGE.height}] /Resources << /Font << /F1 3 0 R >> >> /Contents ${5 + index * 2} 0 R >>`,
    )
    add(`<< /Length ${Buffer.byteLength(content, 'latin1')} >>\nstream\n${content}\nendstream`)
  }

  let out = '%PDF-1.4\n'
  const offsets: number[] = []
  for (const [index, body] of objects.entries()) {
    offsets.push(Buffer.byteLength(out, 'latin1'))
    out += `${index + 1} 0 obj\n${body}\nendobj\n`
  }
  const xref = Buffer.byteLength(out, 'latin1')
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`
  for (const offset of offsets) out += `${String(offset).padStart(10, '0')} 00000 n \n`
  out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`
  return Buffer.from(out, 'latin1')
}

export function writePdf(path: string, pages: string[][]): string {
  writeFileSync(path, pdf(pages))
  return path
}

/** Three pages of known text, for tests that read, search and annotate. */
export const FIXTURE_PAGES = [
  ['On Reading Closely', 'The quick brown fox jumps over the lazy dog.', 'Every sentence here is a fixture.'],
  ['Chapter Two', 'Margins hold the questions of a reader.', 'A lantern burns in the reading room.'],
  ['Chapter Three', 'The lazy dog sleeps through the afternoon.', 'Nothing else happens on this page.'],
]
