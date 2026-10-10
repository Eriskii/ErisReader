<script lang="ts">
  import { onMount } from 'svelte'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Minus from '@lucide/svelte/icons/minus'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import Plus from '@lucide/svelte/icons/plus'
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
  import SquareDashedMousePointer from '@lucide/svelte/icons/square-dashed-mouse-pointer'
  import type { Annotation } from '../../shared/reader'
  import AnnotationList from '../annotations/AnnotationList.svelte'
  import { ReaderStore } from '../annotations/reader-store.svelte'
  import DocumentView from '../document/DocumentView.svelte'
  import AccountsSettings from '../settings/AccountsSettings.svelte'
  import AgentTypesSettings from '../settings/AgentTypesSettings.svelte'
  import SettingsModal from '../settings/SettingsModal.svelte'
  import Welcome from './Welcome.svelte'

  /**
   * The window: the document filling it, or the welcome. A few quiet controls float in its
   * corners; the list of annotations opens down the left side, and an annotation's chat opens in
   * the margin beside it.
   */
  const store = new ReaderStore(window.erisreader)
  /** How wide the list of annotations is. */
  const LIST_WIDTH = 300
  /** The column down the left edge where the controls float; pages and the list keep clear of it. */
  const CONTROLS = 52
  /** The pages' margin beside the list. */
  const PAD = 24

  let view = $state<DocumentView>()
  let settingsOpen = $state(false)
  let snipping = $state(false)
  let dropping = $state(false)

  onMount(() => {
    const unsubscribe = store.api.onEvent((event) => store.handle(event))
    const unrender = store.api.onRender((request) => store.render(request.document, request.page, request.longest))
    void store.start()
    return () => {
      unsubscribe()
      unrender()
    }
  })

  $effect(() => {
    document.title = store.document ? `${store.document.name} — ErisReader` : 'ErisReader'
  })

  // A newly opened document takes the keys for scrolling.
  $effect(() => {
    if (store.document && view) view.focus()
  })

  function openFromList(annotation: Annotation): void {
    void store.open(annotation.id)
    view?.reveal(annotation.anchor, true)
  }

  function snip(): void {
    if (!store.document) return
    getSelection()?.removeAllRanges()
    snipping = !snipping
  }

  const command = (event: KeyboardEvent) => (event.metaKey || event.ctrlKey) && !event.altKey

  function keydown(event: KeyboardEvent): void {
    if (event.defaultPrevented || settingsOpen) return
    if (event.key === 'Escape') {
      const inChat = document.activeElement?.closest('[aria-label="Chat"]') != null
      if (snipping) snipping = false
      else if (inChat || (!store.listOpen && store.chatOpen)) {
        store.closeChat()
        view?.focus()
      } else if (store.listOpen) store.listOpen = false
      else return
      event.preventDefault()
      return
    }
    if (!command(event)) return
    const key = event.key.toLowerCase()
    if (event.shiftKey) {
      if (key === 's') snip()
      else return
    } else if (key === 'k') store.listOpen = !store.listOpen
    else if (key === 'o') void store.pickDocument()
    else if (key === 'w') void store.closeDocument()
    else if (key === 'l') view?.ask()
    else if (key === '=' || key === '+') view?.zoomIn()
    else if (key === '-') view?.zoomOut()
    else if (key === '0') view?.fit()
    else return
    event.preventDefault()
  }

  const isFiles = (event: DragEvent) => event.dataTransfer?.types.includes('Files') === true

  function drop(event: DragEvent): void {
    dropping = false
    if (!isFiles(event)) return
    event.preventDefault()
    const file = [...(event.dataTransfer?.files ?? [])].find((candidate) => /\.pdf$/i.test(candidate.name))
    if (file) void store.openDocument(store.api.pathOf(file))
  }
</script>

<svelte:window onkeydown={keydown} />

<main
  class="reader"
  class:dropping
  ondragover={(event) => {
    if (!isFiles(event)) return
    event.preventDefault()
    dropping = true
  }}
  ondragleave={(event) => {
    if (event.relatedTarget === null) dropping = false
  }}
  ondrop={drop}
>
  <div class="stage" style:left={store.listOpen ? `${CONTROLS + LIST_WIDTH}px` : '0'}>
    {#if store.document}
      {#key store.document}
        <DocumentView
          bind:this={view}
          {store}
          document={store.document}
          bind:snipping
          gutter={store.listOpen ? PAD : CONTROLS}
          onsettings={() => (settingsOpen = true)}
        />
      {/key}
    {:else}
      <Welcome {store} />
    {/if}
  </div>

  {#if store.listOpen}
    <div class="list" style:left={`${CONTROLS}px`} style:width={`${LIST_WIDTH}px`}>
      <AnnotationList {store} onopen={openFromList} />
    </div>
  {/if}

  <div class="corner top-left">
    <button
      class="control"
      aria-label="Annotations"
      aria-expanded={store.listOpen}
      title="Annotations · Ctrl K"
      onclick={() => (store.listOpen = !store.listOpen)}><PanelLeft size={16} strokeWidth={1.5} aria-hidden="true" /></button
    >
    {#if store.document}
      <button class="control" aria-label="Open another PDF" title="Open a PDF · Ctrl O" onclick={() => void store.pickDocument()}
        ><FolderOpen size={16} strokeWidth={1.5} aria-hidden="true" /></button
      >
      <button
        class="control"
        aria-label="Screenshot a region"
        aria-pressed={snipping}
        title="Screenshot a region · Ctrl Shift S"
        onclick={snip}><SquareDashedMousePointer size={16} strokeWidth={1.5} aria-hidden="true" /></button
      >
    {/if}
    <button class="control" aria-label="Settings" aria-haspopup="dialog" title="Settings" onclick={() => (settingsOpen = true)}
      ><SlidersHorizontal size={16} strokeWidth={1.5} aria-hidden="true" /></button
    >
  </div>

  {#if store.document && view}
    <div class="corner bottom-left">
      <button class="control" aria-label="Zoom in" title="Zoom in · Ctrl +" onclick={() => view?.zoomIn()}
        ><Plus size={14} strokeWidth={1.5} aria-hidden="true" /></button
      >
      <button class="control zoom" aria-label="Fit to the window" title="Fit to the window · Ctrl 0" onclick={() => view?.fit()}
        >{view.zoomLabel()}</button
      >
      <button class="control" aria-label="Zoom out" title="Zoom out · Ctrl −" onclick={() => view?.zoomOut()}
        ><Minus size={14} strokeWidth={1.5} aria-hidden="true" /></button
      >
      <span class="readout" aria-label="Document position" title="Page, of pages"
        ><span>{view.position()}</span><span class="visually-hidden">{' / '}</span><span class="of">{store.document.pdf.sizes.length}</span></span
      >
    </div>
  {/if}

  {#if snipping}
    <p class="snip-hint" role="status">Drag over a page to screenshot a region. Esc cancels.</p>
  {/if}

  {#if store.document && store.documentProblem}
    <p class="toast" role="alert">{store.documentProblem}</p>
  {/if}
</main>

{#if settingsOpen}
  <SettingsModal onclose={() => (settingsOpen = false)}>
    <AccountsSettings {store} />
    <AgentTypesSettings {store} />
  </SettingsModal>
{/if}

<style>
  .reader {
    position: fixed;
    inset: 0;
    overflow: hidden;
    background: var(--color-reader);
  }

  .reader.dropping::after {
    content: 'Drop the PDF to open it';
    position: absolute;
    inset: 12px;
    z-index: 20;
    display: grid;
    place-items: center;
    border: 1.5px dashed var(--color-accent);
    border-radius: 18px;
    background: oklch(0 0 0 / 0.45);
    color: var(--color-text);
    font: 500 15px var(--font-ui);
    pointer-events: none;
  }

  .stage {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
  }

  .list {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 8;
    display: flex;
    flex-direction: column;
  }

  .list > :global(*) {
    flex: 1;
  }

  .corner {
    position: absolute;
    left: 10px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 12px;
    opacity: 0.55;
    transition:
      opacity 200ms,
      background 200ms;
  }

  .corner:hover,
  .corner:focus-within {
    opacity: 1;
    background: oklch(0.18 0 0 / 0.85);
  }

  .top-left {
    top: 10px;
  }

  .bottom-left {
    bottom: 10px;
  }

  .control {
    display: grid;
    place-items: center;
    min-width: 28px;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--color-subtext);
    cursor: pointer;
    transition:
      color 140ms,
      background 140ms;
  }

  .control:hover,
  .control:focus-visible,
  .control[aria-expanded='true'],
  .control[aria-pressed='true'] {
    color: var(--color-text);
    background: var(--color-overlay-hover);
    outline: none;
  }

  .zoom,
  .readout {
    font: 10px var(--font-mono);
  }

  .zoom {
    padding: 0 2px;
  }

  .of {
    padding-top: 2px;
    border-top: 1px solid var(--color-panel-5);
    color: var(--color-subsubtext);
  }

  .readout {
    display: grid;
    justify-items: center;
    padding: 4px 0 6px;
    color: var(--color-subtext);
    line-height: 1.2;
  }

  .snip-hint,
  .toast {
    position: absolute;
    top: 14px;
    left: 50%;
    z-index: 12;
    margin: 0;
    padding: 7px 14px;
    border-radius: 999px;
    background: var(--color-text);
    color: var(--color-panel-0);
    font: 500 12px var(--font-ui);
    transform: translateX(-50%);
    pointer-events: none;
  }

  .toast {
    top: auto;
    bottom: 14px;
    background: color-mix(in srgb, var(--color-red) 30%, var(--color-panel-2));
    color: var(--color-text);
  }
</style>
