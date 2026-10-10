/**
 * Where annotations are on pages: rectangles as fractions of a page, so they hold at any zoom.
 * A selection's rectangles on screen become a passage's lines; a drag becomes a region; a click
 * finds the annotation under it.
 */
import type { Anchor, PageRect } from '../../shared/reader'

/** A rectangle on screen, as DOMRect gives one. */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

export interface Point {
  x: number
  y: number
}

/** Rectangles of the same line overlap this much of the shorter one's height. */
const SAME_LINE = 0.5
/** Pieces of a line this close, in heights, join into one rectangle. */
const JOIN_GAP = 0.8

const right = (box: Box) => box.left + box.width
const bottom = (box: Box) => box.top + box.height

function union(a: Box, b: Box): Box {
  const left = Math.min(a.left, b.left)
  const top = Math.min(a.top, b.top)
  return { left, top, width: Math.max(right(a), right(b)) - left, height: Math.max(bottom(a), bottom(b)) - top }
}

const encloses = (outer: Box, inner: Box) =>
  outer !== inner &&
  outer.left <= inner.left + 0.5 &&
  outer.top <= inner.top + 0.5 &&
  right(outer) >= right(inner) - 0.5 &&
  bottom(outer) >= bottom(inner) - 0.5

function sameLine(a: Box, b: Box): boolean {
  const overlap = Math.min(bottom(a), bottom(b)) - Math.max(a.top, b.top)
  const height = Math.min(a.height, b.height)
  const gap = Math.max(a.left, b.left) - Math.min(right(a), right(b))
  return overlap >= height * SAME_LINE && gap <= height * JOIN_GAP
}

/**
 * A selection's rectangles on screen as lines on the pages they lie in: empty ones and those
 * enclosing others (a wholly selected element's box) left out, the pieces of each line joined.
 */
export function pageRects(rects: readonly Box[], pages: readonly { index: number; box: Box }[]): PageRect[] {
  const filled = rects.filter((rect) => rect.width > 0.5 && rect.height > 0.5)
  const pieces = filled.filter((rect) => !filled.some((other) => encloses(rect, other)))
  const out: PageRect[] = []
  for (const { index, box } of pages) {
    const lines: Box[] = []
    for (const piece of pieces) {
      const middle = { x: piece.left + piece.width / 2, y: piece.top + piece.height / 2 }
      if (middle.x < box.left || middle.x > right(box) || middle.y < box.top || middle.y > bottom(box)) continue
      const line = lines.findIndex((other) => sameLine(other, piece))
      if (line < 0) lines.push(piece)
      else lines[line] = union(lines[line], piece)
    }
    lines.sort((a, b) => a.top - b.top || a.left - b.left)
    for (const line of lines) {
      out.push({
        page: index,
        x: (line.left - box.left) / box.width,
        y: (line.top - box.top) / box.height,
        width: line.width / box.width,
        height: line.height / box.height,
      })
    }
  }
  return out
}

const clamp = (value: number) => Math.min(1, Math.max(0, value))

/** The region a drag spans on a page, from where it started to where it is, kept on the page. */
export function dragRect(from: Point, to: Point): Omit<PageRect, 'page'> {
  const [x1, x2] = [clamp(Math.min(from.x, to.x)), clamp(Math.max(from.x, to.x))]
  const [y1, y2] = [clamp(Math.min(from.y, to.y)), clamp(Math.max(from.y, to.y))]
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 }
}

/** An anchor's rectangles. */
export const anchorRects = (anchor: Anchor): PageRect[] => (anchor.kind === 'text' ? anchor.rects : [anchor.rect])

/** A little room around an annotation's lines, as a fraction of the page, so a click near one finds it. */
const REACH = 0.004

/** The smallest annotation under a point on a page. */
export function annotationAt(annotations: readonly { id: string; anchor: Anchor }[], page: number, point: Point): string | undefined {
  let found: { id: string; area: number } | undefined
  for (const { id, anchor } of annotations) {
    const rects = anchorRects(anchor).filter((rect) => rect.page === page)
    const hit = rects.some(
      (rect) =>
        point.x >= rect.x - REACH &&
        point.x <= rect.x + rect.width + REACH &&
        point.y >= rect.y - REACH &&
        point.y <= rect.y + rect.height + REACH,
    )
    if (!hit) continue
    const area = rects.reduce((sum, rect) => sum + rect.width * rect.height, 0)
    if (!found || area < found.area) found = { id, area }
  }
  return found?.id
}
