import { describe, expect, it } from 'vitest'
import { annotationAt, dragRect, pageRects } from '../../src/lib/document/geometry'
import type { Anchor } from '../../src/shared/reader'

const box = (x: number, y: number, width: number, height: number) => ({ left: x, top: y, width, height })

/** Two pages of 600 × 800 px, 20 px apart. */
const pages = [
  { index: 0, box: box(100, 0, 600, 800) },
  { index: 1, box: box(100, 820, 600, 800) },
]

describe('pageRects', () => {
  it('places each rect on the page it lies in, as fractions of the page', () => {
    expect(pageRects([box(160, 80, 300, 20), box(160, 900, 120, 20)], pages)).toEqual([
      { page: 0, x: 0.1, y: 0.1, width: 0.5, height: 0.025 },
      { page: 1, x: 0.1, y: 0.1, width: 0.2, height: 0.025 },
    ])
  })

  it('joins the pieces of a line into one rect, and keeps lines apart', () => {
    const rects = pageRects([box(160, 80, 100, 20), box(262, 81, 100, 18), box(160, 104, 60, 20)], pages)
    expect(rects).toEqual([
      { page: 0, x: 0.1, y: 0.1, width: 202 / 600, height: 0.025 },
      { page: 0, x: 0.1, y: 0.13, width: 0.1, height: 0.025 },
    ])
  })

  it('leaves out empty rects and those outside every page', () => {
    expect(pageRects([box(160, 80, 0, 20), box(160, 80, 40, 0), box(10, 80, 40, 20), box(160, 805, 40, 10)], pages)).toEqual([])
  })

  it('drops a rect enclosing others, as a fully selected element’s box does', () => {
    const rects = pageRects([box(150, 70, 400, 60), box(160, 80, 100, 20), box(160, 104, 60, 20)], pages)
    expect(rects.map((r) => r.y)).toEqual([0.1, 0.13])
  })
})

describe('dragRect', () => {
  it('spans the two corners whichever way the drag went, kept on the page', () => {
    const near = (rect: Record<string, number>) => Object.fromEntries(Object.entries(rect).map(([key, value]) => [key, expect.closeTo(value)]))
    expect(dragRect({ x: 0.6, y: 0.5 }, { x: 0.2, y: 0.1 })).toEqual(near({ x: 0.2, y: 0.1, width: 0.4, height: 0.4 }))
    expect(dragRect({ x: 0.9, y: 0.9 }, { x: 1.4, y: -0.2 })).toEqual(near({ x: 0.9, y: 0, width: 0.1, height: 0.9 }))
  })
})

describe('annotationAt', () => {
  const text = (id: string, rects: { page: number; x: number; y: number; width: number; height: number }[]) => ({
    id,
    anchor: { kind: 'text', rects, text: 'passage' } satisfies Anchor,
  })
  const region = (id: string, rect: { page: number; x: number; y: number; width: number; height: number }) => ({
    id,
    anchor: { kind: 'region', rect, text: '' } satisfies Anchor,
  })

  const annotations = [
    text('line', [{ page: 0, x: 0.1, y: 0.1, width: 0.5, height: 0.025 }]),
    region('box', { page: 0, x: 0.05, y: 0.05, width: 0.8, height: 0.3 }),
    region('later', { page: 1, x: 0.05, y: 0.05, width: 0.8, height: 0.3 }),
  ]

  it('finds the smallest annotation under a point on its page', () => {
    expect(annotationAt(annotations, 0, { x: 0.3, y: 0.11 })).toBe('line')
    expect(annotationAt(annotations, 0, { x: 0.3, y: 0.3 })).toBe('box')
    expect(annotationAt(annotations, 1, { x: 0.3, y: 0.11 })).toBe('later')
    expect(annotationAt(annotations, 0, { x: 0.95, y: 0.9 })).toBeUndefined()
  })
})
