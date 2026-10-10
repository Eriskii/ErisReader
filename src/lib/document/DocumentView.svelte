<script lang="ts">
  import { tick } from 'svelte'
  import type { Anchor, PageRect } from '../../shared/reader'
  import AnnotationChat from '../annotations/AnnotationChat.svelte'
  import type { OpenDocument, ReaderStore } from '../annotations/reader-store.svelte'
  import { anchorRects, annotationAt, dragRect, pageRects, type Point } from './geometry'
  import PageView, { type Mark } from './PageView.svelte'
  import { CSS_PER_POINT, regionImage, regionText } from './pdf'

  /**
   * The open document, filling the window: its pages in a column down the middle, and the open
   * annotation's chat in the right margin, level with it. Opening a chat moves the pages over to
   * make room. Selecting text offers to ask about it; in screenshot mode a drag over a page
   * draws a region to ask about. A click on an annotation opens its chat.
   */
  let {
    store,
    document: doc,
    snipping = $bindable(false),
    gutter,
    onsettings,
  }: {
    store: ReaderStore
    document: OpenDocument
    snipping?: boolean
    /** Room the pages leave on the left, for the controls floating there. */
    gutter: number
    onsettings: () => void
  } = $props()

  /** Space around the pages, and between them. */
  const PAD = 24
  const PAGE_GAP = 16
  const CHAT_WIDTH = 400
  const CHAT_GAP = 28
  /** The largest zoom fitting the window picks. */
  const MAX_FIT = 1.25 * CSS_PER_POINT
  const MIN_SCALE = 0.25 * CSS_PER_POINT
  const ZOOMS = [0.5, 0.67, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4].map((zoom) => zoom * CSS_PER_POINT)
  /** How large a region's picture is for its agent. */
  const REGION_IMAGE = 1568
  /** A region smaller than this on screen is a slip, not a screenshot. */
  const MIN_REGION = 8

  let scroller: HTMLElement
  let pagesElement: HTMLElement
  let width = $state(0)
  let height = $state(0)
  let scrollTop = $state(0)
  /** `fit` fits the widest page to the window, leaving room for a chat. */
  let zoom = $state<'fit' | number>('fit')

  const widest = $derived(Math.max(...doc.pdf.sizes.map((size) => size.width)))
  const fitScale = $derived(Math.max(MIN_SCALE, Math.min(MAX_FIT, (width - gutter - PAD - CHAT_GAP - CHAT_WIDTH) / widest)))
  const scale = $derived(zoom === 'fit' ? fitScale : zoom)

  const layout = $derived.by(() => {
    const pageWidth = widest * scale
    const group = pageWidth + (store.chatOpen ? CHAT_GAP + CHAT_WIDTH : 0)
    const contentWidth = Math.max(width, gutter + group + PAD)
    const groupLeft = Math.max(gutter, (contentWidth - group) / 2)
    let top = PAD
    const boxes = doc.pdf.sizes.map((size) => {
      const box = { left: groupLeft + (pageWidth - size.width * scale) / 2, top, width: size.width * scale, height: size.height * scale }
      top += box.height + PAGE_GAP
      return box
    })
    return { scale, boxes, contentWidth, contentHeight: top - PAGE_GAP + PAD, chatLeft: groupLeft + pageWidth + CHAT_GAP }
  })

  /** The page at a third of the way down the view, counting from 1. */
  export function position(): number {
    const line = scrollTop + height / 3
    const index = layout.boxes.findIndex((box) => box.top + box.height + PAGE_GAP > line)
    return (index < 0 ? layout.boxes.length - 1 : index) + 1
  }

  export function zoomLabel(): string {
    return zoom === 'fit' ? 'Fit' : `${Math.round((scale / CSS_PER_POINT) * 100)}%`
  }

  export function zoomIn(): void {
    zoom = ZOOMS.find((step) => step > scale + 0.001) ?? ZOOMS.at(-1)!
  }

  export function zoomOut(): void {
    zoom = ZOOMS.findLast((step) => step < scale - 0.001) ?? ZOOMS[0]
  }

  export function fit(): void {
    zoom = 'fit'
  }

  export function focus(): void {
    scroller?.focus({ preventScroll: true })
  }

  /** A place in the document: a page's index, and how far down it the top of the view is. */
  interface Place {
    index: number
    fraction: number
  }

  /** Where the reader is in a document, kept between visits. */
  const placeKey = $derived(`erisreader.place:${doc.path}`)

  function placeIn(laid: typeof layout, top: number): Place {
    const index = Math.max(0, laid.boxes.findIndex((box) => box.top + box.height > top))
    return { index, fraction: (top - laid.boxes[index].top) / laid.boxes[index].height }
  }

  function savedPlace(): Place | undefined {
    try {
      const saved: Partial<Place> | null = JSON.parse(localStorage.getItem(placeKey) ?? 'null')
      if (typeof saved?.index === 'number' && typeof saved.fraction === 'number') return { index: saved.index, fraction: saved.fraction }
    } catch {
      // Open at the start.
    }
    return undefined
  }

  /**
   * Where to scroll once the pages are laid out: where the document was left, and after a zoom,
   * wherever the view was before it.
   */
  let pending = savedPlace()
  /** Whether the view has been placed since the document opened. */
  let placed = false
  let shown: typeof layout | undefined

  $effect.pre(() => {
    const next = layout
    const before = shown
    shown = next
    if (placed && before && scroller && before.scale !== next.scale) pending = placeIn(before, scroller.scrollTop)
  })

  $effect(() => {
    const laid = layout
    if (width === 0 || !scroller || !pagesElement) return
    // Until the pages have their size on screen, a scroll would be cut short.
    if (Math.abs(pagesElement.offsetHeight - laid.contentHeight) > 1) return
    const place = pending
    pending = undefined
    placed = true
    const box = place && laid.boxes[place.index]
    if (box) scroller.scrollTop = box.top + place.fraction * box.height
  })

  let saving = 0
  function scrolled(): void {
    scrollTop = scroller.scrollTop
    if (!placed) return
    cancelAnimationFrame(saving)
    saving = requestAnimationFrame(() => {
      try {
        localStorage.setItem(placeKey, JSON.stringify(placeIn(layout, scroller.scrollTop)))
      } catch {
        // The document opens at its start next time.
      }
    })
  }

  /** Pages within a view's height of the view draw. */
  const near = (box: { top: number; height: number }) => box.top + box.height > scrollTop - height && box.top < scrollTop + 2 * height

  /** Scrolls an annotation into the upper part of the view, unless it is there. */
  export function reveal(anchor: Anchor, always = false): void {
    const [first] = anchorRects(anchor)
    const box = layout.boxes[first.page]
    if (!box || !scroller) return
    const top = box.top + first.y * box.height
    if (!always && top >= scroller.scrollTop + PAD && top <= scroller.scrollTop + height * 0.55) return
    scroller.scrollTo({ top: Math.max(0, top - height * 0.2), behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  }

  // Opening a chat brings its annotation into view, the chat beside it.
  let revealed: unknown
  $effect(() => {
    const opened = store.draft ?? store.current
    const anchor = store.anchor
    if (opened === revealed) return
    revealed = opened
    if (anchor) void tick().then(() => reveal(anchor))
  })

  /** The least room a chat keeps, however little of its page is in view; below it, the chat scrolls away with the page. */
  const MIN_CHAT = 220
  /** Space between the chat and the edges of the view. */
  const CHAT_INSET = 16

  /**
   * Where the chat may be: beside its annotation's page, from the page's top to its bottom. It
   * sits level with the annotation, rises as it grows rather than run past the page, and stays
   * in view while the page does, no taller than the part of the page in view.
   */
  const chatTrack = $derived.by(() => {
    const anchor = store.anchor
    if (!anchor) return undefined
    const [first] = anchorRects(anchor)
    const box = layout.boxes[first.page]
    const shown = Math.min(box.top + box.height, scrollTop + height - CHAT_INSET) - Math.max(box.top, scrollTop + CHAT_INSET)
    return { top: box.top, left: layout.chatLeft, height: box.height, lead: first.y * box.height, room: Math.max(MIN_CHAT, shown) }
  })

  let hovered = $state<string>()

  /** Pages slide aside as a chat opens or closes, and nowhere else: a zoom or a new window size moves them at once. */
  let sliding = $state(false)
  let wasOpen: boolean | undefined
  $effect(() => {
    const open = store.chatOpen
    if (wasOpen === undefined || open === wasOpen) {
      wasOpen = open
      return
    }
    wasOpen = open
    sliding = true
    const timer = setTimeout(() => (sliding = false), 400)
    return () => clearTimeout(timer)
  })

  /** A page under a point on screen, and the point as fractions of it. */
  function pageAt(clientX: number, clientY: number): { index: number; point: Point } | undefined {
    for (const element of pagesElement.querySelectorAll<HTMLElement>('[data-page-index]')) {
      const bounds = element.getBoundingClientRect()
      if (clientX < bounds.left || clientX > bounds.right || clientY < bounds.top || clientY > bounds.bottom) continue
      return {
        index: Number(element.dataset.pageIndex),
        point: { x: (clientX - bounds.left) / bounds.width, y: (clientY - bounds.top) / bounds.height },
      }
    }
    return undefined
  }

  function annotationUnder(event: MouseEvent): string | undefined {
    const at = pageAt(event.clientX, event.clientY)
    return at && annotationAt(store.annotations, at.index, at.point)
  }

  function click(event: MouseEvent): void {
    if (snipping || event.button !== 0 || getSelection()?.isCollapsed === false) return
    const id = annotationUnder(event)
    if (id) void store.open(id)
  }

  // The selection, as an offer to ask about it.
  let selection = $state<{ rects: PageRect[]; text: string; at: Point }>()

  function readSelection(): void {
    const chosen = getSelection()
    if (!chosen || chosen.isCollapsed || !chosen.rangeCount || !pagesElement) {
      selection = undefined
      return
    }
    const range = chosen.getRangeAt(0)
    const onPage = (node: Node) =>
      pagesElement.contains(node) && (node instanceof Element ? node : node.parentElement)?.closest('.textLayer') != null
    if (!onPage(range.startContainer) || !onPage(range.endContainer)) {
      selection = undefined
      return
    }
    const pages = [...pagesElement.querySelectorAll<HTMLElement>('[data-page-index]')].map((element) => ({
      index: Number(element.dataset.pageIndex),
      box: element.getBoundingClientRect(),
    }))
    const rects = pageRects([...range.getClientRects()], pages)
    const text = chosen.toString().replace(/\s+/g, ' ').trim()
    const last = rects.at(-1)
    if (!last || !text) {
      selection = undefined
      return
    }
    const box = layout.boxes[last.page]
    selection = { rects, text, at: { x: box.left + (last.x + last.width) * box.width, y: box.top + (last.y + last.height) * box.height } }
  }

  let pendingRead = 0
  function selectionChanged(): void {
    cancelAnimationFrame(pendingRead)
    pendingRead = requestAnimationFrame(readSelection)
  }

  /** Starts an annotation of the selected passage. */
  export function ask(): boolean {
    readSelection()
    if (!selection) return false
    store.startDraft($state.snapshot({ kind: 'text' as const, rects: selection.rects, text: selection.text }))
    getSelection()?.removeAllRanges()
    selection = undefined
    return true
  }

  // Screenshot mode: a drag over a page draws a region.
  let drawing = $state<{ page: number; from: Point; to: Point }>()

  function snipDown(event: PointerEvent): void {
    if (event.button !== 0) return
    const at = pageAt(event.clientX, event.clientY)
    if (!at) return
    event.preventDefault()
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    drawing = { page: at.index, from: at.point, to: at.point }
  }

  function snipMove(event: PointerEvent): void {
    if (!drawing) return
    const element = pagesElement.querySelector<HTMLElement>(`[data-page-index="${drawing.page}"]`)
    if (!element) return
    const bounds = element.getBoundingClientRect()
    drawing.to = { x: (event.clientX - bounds.left) / bounds.width, y: (event.clientY - bounds.top) / bounds.height }
  }

  async function snipUp(): Promise<void> {
    const drawn = drawing
    if (!drawn) return
    drawing = undefined
    const rect = { page: drawn.page, ...dragRect(drawn.from, drawn.to) }
    const box = layout.boxes[drawn.page]
    if (rect.width * box.width < MIN_REGION || rect.height * box.height < MIN_REGION) return
    snipping = false
    const page = await doc.pdf.proxy.getPage(drawn.page + 1)
    const image = await regionImage(page, rect, REGION_IMAGE)
    store.startDraft({ kind: 'region', rect, text: regionText(page, doc.pdf.contents[drawn.page], rect) }, image)
  }

  $effect(() => {
    if (!snipping) drawing = undefined
  })

  /** Each page's annotations, the open chat's draft, and a region being drawn. */
  const marks = $derived.by(() => {
    const byPage: Mark[][] = doc.pdf.sizes.map(() => [])
    const add = (id: string | undefined, anchor: Anchor, open: boolean) => {
      const rects = anchorRects(anchor)
      for (const page of new Set(rects.map((rect) => rect.page))) {
        byPage[page]?.push({
          id,
          kind: anchor.kind,
          rects: rects.filter((rect) => rect.page === page),
          open,
          hover: id !== undefined && id === hovered,
        })
      }
    }
    for (const annotation of store.annotations) add(annotation.id, annotation.anchor, annotation.id === store.current)
    if (store.draft) add(undefined, store.draft.anchor, true)
    if (drawing) add(undefined, { kind: 'region', rect: { page: drawing.page, ...dragRect(drawing.from, drawing.to) }, text: '' }, true)
    return byPage
  })

  function wheel(event: WheelEvent): void {
    if (!event.ctrlKey) return
    event.preventDefault()
    if (event.deltaY < 0) zoomIn()
    else if (event.deltaY > 0) zoomOut()
  }

  function wheelZoom(node: HTMLElement) {
    node.addEventListener('wheel', wheel, { passive: false })
    return { destroy: () => node.removeEventListener('wheel', wheel) }
  }
</script>

<svelte:document onselectionchange={selectionChanged} />

<div
  class="scroller"
  class:snipping
  tabindex="-1"
  bind:this={scroller}
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={scrolled}
  use:wheelZoom
>
  <div
    class="content"
    style:width={`${layout.contentWidth}px`}
    style:height={`${layout.contentHeight}px`}
    bind:this={pagesElement}
    role="presentation"
    onclick={click}
    onpointermove={(event) => (hovered = snipping ? undefined : annotationUnder(event))}
    onpointerleave={() => (hovered = undefined)}
    class:pointing={hovered !== undefined}
    class:sliding
  >
    {#each layout.boxes as box, index (index)}
      <PageView
        proxy={doc.pdf.proxy}
        {index}
        content={doc.pdf.contents[index]}
        scale={layout.scale}
        {box}
        visible={near(box)}
        marks={marks[index]}
      />
    {/each}

    {#if selection && !snipping}
      <button
        class="ask"
        style:left={`${selection.at.x}px`}
        style:top={`${selection.at.y + 8}px`}
        title="Ask about the selection · Ctrl L"
        onpointerdown={(event) => event.preventDefault()}
        onclick={(event) => {
          event.stopPropagation()
          ask()
        }}>Ask about this</button
      >
    {/if}

    {#if snipping}
      <div
        class="snip-surface"
        role="presentation"
        onpointerdown={snipDown}
        onpointermove={snipMove}
        onpointerup={() => void snipUp()}
        onpointercancel={() => (drawing = undefined)}
      ></div>
    {/if}

    {#if chatTrack}
      <div
        class="chat-track"
        style:top={`${chatTrack.top}px`}
        style:left={`${chatTrack.left}px`}
        style:width={`${CHAT_WIDTH}px`}
        style:height={`${chatTrack.height}px`}
        style:--chat-room={`${chatTrack.room}px`}
      >
        <div class="chat-lead" style:flex-basis={`${chatTrack.lead}px`}></div>
        <AnnotationChat {store} document={doc} {onsettings} />
      </div>
    {/if}
  </div>
</div>

<style>
  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
    /* The view moves only when the reader scrolls or a chat brings its annotation into view. */
    overflow-anchor: none;
    outline: none;
    scrollbar-width: thin;
    scrollbar-color: var(--color-panel-5) transparent;
  }

  .content {
    position: relative;
  }

  .sliding :global(.page),
  .sliding .chat-track {
    transition: left 320ms var(--ease-standard);
  }

  .pointing :global(.textLayer span) {
    cursor: pointer;
  }

  .snipping .content {
    user-select: none;
  }

  .snip-surface {
    position: absolute;
    inset: 0;
    z-index: 5;
    cursor: crosshair;
  }

  .ask {
    position: absolute;
    z-index: 6;
    padding: 6px 12px;
    border: 0;
    border-radius: 999px;
    background: var(--color-text);
    color: var(--color-panel-0);
    font: 550 12px/1 var(--font-ui);
    box-shadow: 0 6px 18px var(--color-shadow-strong);
    cursor: pointer;
    transform: translateX(-50%);
    animation: rise 180ms var(--ease-standard);
  }

  .ask:hover,
  .ask:focus-visible {
    background: var(--color-accent);
    outline: none;
  }

  .chat-track {
    position: absolute;
    z-index: 4;
    display: flex;
    flex-direction: column;
  }

  /* Holds the chat level with its annotation, and gives way as the chat grows. */
  .chat-lead {
    flex: 0 1 auto;
    min-height: 0;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translate(-50%, 4px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sliding :global(.page),
    .sliding .chat-track {
      transition: none;
    }

    .ask {
      animation: none;
    }
  }
</style>
