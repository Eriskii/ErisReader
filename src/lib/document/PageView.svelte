<script lang="ts" module>
  import type { PageRect } from '../../shared/reader'

  /** An annotation as a page shows it: its rectangles on this page. */
  export interface Mark {
    /** Undefined for one not yet made: a draft, or a region being drawn. */
    id: string | undefined
    kind: 'text' | 'region'
    rects: PageRect[]
    /** Its chat is open. */
    open: boolean
    hover: boolean
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte'
  import { TextLayer, type PDFDocumentProxy, type RenderTask } from 'pdfjs-dist'
  import type { TextContent } from 'pdfjs-dist/types/src/display/api'
  import { MAX_CANVAS_PIXELS } from './pdf'

  /**
   * A page: drawn on a canvas, with pdf.js's text layer over it for selecting, and the
   * annotations on it: passages highlighted beneath the text, regions framed above it. It
   * draws only while near the view, and draws again at a new zoom, showing the old drawing
   * stretched until then.
   */
  let {
    proxy,
    index,
    content,
    scale,
    box,
    visible,
    marks,
  }: {
    proxy: PDFDocumentProxy
    index: number
    content: TextContent
    /** CSS pixels per point. */
    scale: number
    box: { left: number; top: number; width: number; height: number }
    visible: boolean
    marks: Mark[]
  } = $props()

  let surface: HTMLElement
  let text: HTMLElement
  let userUnit = $state(1)
  let selecting = $state(false)

  /** How long a new zoom or window size holds before the page draws at it; until then the drawing stretches. */
  const SETTLE_MS = 140

  /**
   * The scale the page draws at: the shown one, once it has held, so a zoom, a resize or the
   * list opening draws once. A page with no drawing yet draws at once.
   */
  let drawScale = $state(0)
  /** Whether a drawing of the page is on show, to stretch while a new one waits. */
  let drawing = false
  $effect(() => {
    const next = scale
    const drawn = untrack(() => drawScale)
    if (drawn === next) return
    if (!drawing || !visible) {
      drawScale = next
      return
    }
    const timer = setTimeout(() => (drawScale = next), SETTLE_MS)
    return () => clearTimeout(timer)
  })

  /**
   * The page's text layer, kept while the page is near the view. Its runs are placed in
   * fractions of the page and sized by --scale-factor, so a new zoom only lays them out again.
   */
  let layer: TextLayer | undefined

  $effect(() => {
    const at = drawScale
    if (!visible || at === 0) {
      drawing = false
      layer?.cancel()
      layer = undefined
      surface?.replaceChildren()
      text?.replaceChildren()
      return
    }
    let cancelled = false
    let task: RenderTask | undefined
    void (async () => {
      const page = await proxy.getPage(index + 1)
      if (cancelled) return
      const viewport = page.getViewport({ scale: at })
      userUnit = viewport.userUnit
      if (layer) layer.update({ viewport })
      else {
        const container = document.createElement('div')
        container.className = 'textLayer'
        const created = new TextLayer({ textContentSource: content, container, viewport })
        layer = created
        created
          .render()
          .then(() => {
            if (layer !== created) return
            const end = document.createElement('div')
            end.className = 'endOfContent'
            container.append(end)
            text.replaceChildren(container)
          })
          .catch(() => {})
      }
      const ratio = Math.min(devicePixelRatio || 1, Math.sqrt(MAX_CANVAS_PIXELS / (viewport.width * viewport.height)))
      const drawn = page.getViewport({ scale: at * ratio })
      const canvas = document.createElement('canvas')
      canvas.setAttribute('aria-hidden', 'true')
      canvas.width = Math.floor(drawn.width)
      canvas.height = Math.floor(drawn.height)
      task = page.render({ canvas, viewport: drawn })
      try {
        await task.promise
      } catch {
        return
      }
      if (cancelled) return
      surface.replaceChildren(canvas)
      drawing = true
    })()
    return () => {
      cancelled = true
      task?.cancel()
    }
  })

  const bounds = (rects: PageRect[]) => {
    const left = Math.min(...rects.map((r) => r.x))
    const top = Math.min(...rects.map((r) => r.y))
    const right = Math.max(...rects.map((r) => r.x + r.width))
    const bottom = Math.max(...rects.map((r) => r.y + r.height))
    return { left, top, width: right - left, height: bottom - top }
  }

  const percent = (value: number) => `${value * 100}%`
</script>

<svelte:window onpointerup={() => (selecting = false)} />

<div
  class="page"
  role="group"
  aria-label={`Page ${index + 1}`}
  data-page-index={index}
  style:left={`${box.left}px`}
  style:top={`${box.top}px`}
  style:width={`${box.width}px`}
  style:height={`${box.height}px`}
  style:--scale-factor={scale}
  style:--user-unit={userUnit}
>
  <div class="surface" bind:this={surface}></div>

  <div class="highlights" aria-hidden="true">
    {#each marks.filter((mark) => mark.kind === 'text') as mark, i (mark.id ?? `draft-${i}`)}
      {@const outer = bounds(mark.rects)}
      <div
        class="mark"
        class:open={mark.open}
        class:hover={mark.hover}
        class:draft={!mark.id}
        data-annotation={mark.id}
        data-kind="text"
        style:left={percent(outer.left)}
        style:top={percent(outer.top)}
        style:width={percent(outer.width)}
        style:height={percent(outer.height)}
      >
        {#each mark.rects as rect, line (line)}
          <span
            style:left={percent((rect.x - outer.left) / outer.width)}
            style:top={percent((rect.y - outer.top) / outer.height)}
            style:width={percent(rect.width / outer.width)}
            style:height={percent(rect.height / outer.height)}
          ></span>
        {/each}
      </div>
    {/each}
  </div>

  <div class="text" class:selecting role="presentation" bind:this={text} onpointerdown={() => (selecting = true)}></div>

  <div class="frames" aria-hidden="true">
    {#each marks.filter((mark) => mark.kind === 'region') as mark, i (mark.id ?? `draft-${i}`)}
      {@const [rect] = mark.rects}
      <div
        class="frame"
        class:open={mark.open}
        class:hover={mark.hover}
        class:draft={!mark.id}
        data-annotation={mark.id}
        data-kind="region"
        style:left={percent(rect.x)}
        style:top={percent(rect.y)}
        style:width={percent(rect.width)}
        style:height={percent(rect.height)}
      >
        <i class="corner nw"></i><i class="corner ne"></i><i class="corner sw"></i><i class="corner se"></i>
      </div>
    {/each}
  </div>
</div>

<style>
  .page {
    --user-unit: 1;
    --total-scale-factor: calc(var(--scale-factor) * var(--user-unit));
    --scale-round-x: 1px;
    --scale-round-y: 1px;
    --mark-ink: oklch(0.5 0.2 299);
    --mark-fill: oklch(0.86 0.1 299);
    --mark-fill-strong: oklch(0.82 0.12 299);
    position: absolute;
    overflow: hidden;
    background: white;
    box-shadow:
      0 1px 2px oklch(0 0 0 / 0.4),
      0 8px 28px oklch(0 0 0 / 0.28);
    contain: layout paint;
  }

  .surface,
  .surface :global(canvas) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .highlights,
  .frames {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .highlights {
    z-index: 1;
    mix-blend-mode: multiply;
  }

  .mark,
  .mark span,
  .frame {
    position: absolute;
  }

  .mark span {
    margin: -1px -2px;
    padding: 1px 2px;
    border-radius: 2px;
    background: var(--mark-fill);
    box-sizing: content-box;
    transition: background 160ms;
  }

  .mark.hover span,
  .mark.open span {
    background: var(--mark-fill-strong);
  }

  .mark.draft span {
    background: color-mix(in oklch, var(--mark-fill) 60%, white);
    outline: 1px dashed var(--mark-ink);
  }

  .text {
    position: absolute;
    inset: 0;
    z-index: 2;
  }

  /* pdf.js's text layer (see pdfjs-dist/web/pdf_viewer.css), above the highlights. */
  .text :global(.textLayer) {
    position: absolute;
    inset: 0;
    overflow: clip;
    line-height: 1;
    text-align: initial;
    text-size-adjust: none;
    forced-color-adjust: none;
    transform-origin: 0 0;
    color-scheme: only light;
    --min-font-size: 1;
    --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
    --min-font-size-inv: calc(1 / var(--min-font-size));
  }

  .text :global(.textLayer :is(span, br)) {
    position: absolute;
    color: transparent;
    white-space: pre;
    cursor: text;
    transform-origin: 0% 0%;
    user-select: text;
  }

  .text :global(.textLayer > :not(.markedContent)),
  .text :global(.textLayer .markedContent span:not(.markedContent)) {
    z-index: 1;
    --font-height: 0;
    font-size: calc(var(--text-scale-factor) * var(--font-height));
    --scale-x: 1;
    --rotate: 0deg;
    transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
  }

  .text :global(.textLayer .markedContent) {
    display: contents;
  }

  .text :global(.textLayer ::selection) {
    background: oklch(0.6 0.16 299 / 0.32);
    color: transparent;
  }

  .text :global(.textLayer br::selection) {
    background: transparent;
  }

  .text :global(.textLayer .endOfContent) {
    position: absolute;
    inset: 100% 0 0;
    z-index: 0;
    display: block;
    cursor: default;
    user-select: none;
  }

  .text.selecting :global(.textLayer .endOfContent) {
    top: 0;
  }

  .frames {
    z-index: 3;
  }

  .frame {
    border: 1.5px solid var(--mark-ink);
    background: oklch(0.5 0.2 299 / 0.04);
    transition:
      background 160ms,
      border-width 160ms;
  }

  .frame.hover,
  .frame.open {
    background: oklch(0.5 0.2 299 / 0.09);
  }

  .frame.open {
    border-width: 2px;
  }

  .frame.draft {
    border-style: dashed;
  }

  .corner {
    position: absolute;
    width: 6px;
    height: 6px;
    border: 1.5px solid var(--mark-ink);
    background: white;
  }

  .nw {
    top: -4px;
    left: -4px;
  }

  .ne {
    top: -4px;
    right: -4px;
  }

  .sw {
    bottom: -4px;
    left: -4px;
  }

  .se {
    right: -4px;
    bottom: -4px;
  }
</style>
