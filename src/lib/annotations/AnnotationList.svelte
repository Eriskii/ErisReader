<script lang="ts">
  import Scan from '@lucide/svelte/icons/scan'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import type { Annotation } from '../../shared/reader'
  import { anchorRects } from '../document/geometry'
  import type { ReaderStore } from './reader-store.svelte'
  import { ago } from './time'

  /** Every annotation of the open document, in the order they come in it: open one's chat, or delete one. */
  let { store, onopen }: { store: ReaderStore; onopen: (annotation: Annotation) => void } = $props()

  const id = $props.id()
  let filter = $state('')
  let deleting = $state<string>()
  let now = $state(Date.now())

  const shown = $derived.by(() => {
    const query = filter.trim().toLowerCase()
    if (!query) return store.annotations
    return store.annotations.filter(
      (annotation) => annotation.title.toLowerCase().includes(query) || annotation.anchor.text.toLowerCase().includes(query),
    )
  })

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000)
    return () => clearInterval(timer)
  })

  const pageOf = (annotation: Annotation) => anchorRects(annotation.anchor)[0].page + 1

  function confirmKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return
    event.preventDefault()
    deleting = undefined
  }

  const focusOnMount = (node: HTMLElement) => node.focus()
</script>

<aside class="annotation-list" aria-label="Annotations">
  <h2>Annotations{#if store.annotations.length}<span class="count">{store.annotations.length}</span>{/if}</h2>

  {#if store.annotations.length > 5}
    <label class="visually-hidden" for={`${id}-filter`}>Find an annotation</label>
    <input id={`${id}-filter`} class="filter" type="search" placeholder="Find an annotation" bind:value={filter} />
  {/if}

  <ul aria-label="Annotations" tabindex="-1">
    {#each shown as annotation (annotation.id)}
      <li class:current={annotation.id === store.current} data-state={annotation.state}>
        {#if deleting === annotation.id}
          <div class="confirm" role="group" aria-label={`Delete ${annotation.title}`}>
            <span>Delete this annotation and its chat?</span>
            <div class="choices">
              <button class="ui-button quiet" onclick={() => (deleting = undefined)} onkeydown={confirmKeydown}>Cancel</button>
              <button
                class="ui-button danger"
                onclick={() => {
                  deleting = undefined
                  void store.delete(annotation.id)
                }}
                onkeydown={confirmKeydown}
                use:focusOnMount>Delete</button
              >
            </div>
          </div>
        {:else}
          <button class="open" aria-current={annotation.id === store.current ? 'true' : undefined} onclick={() => onopen(annotation)}>
            <span class="where">
              <span class="dot" aria-hidden="true"></span>
              p. {pageOf(annotation)}
              {#if annotation.anchor.kind === 'region'}<Scan size={11} strokeWidth={1.5} aria-label="region" />{/if}
              <span class="when">{ago(annotation.updated, now)}{annotation.state === 'failed' ? ' · failed' : ''}</span>
            </span>
            {#if annotation.anchor.text.trim()}
              <span class="excerpt">{annotation.anchor.text}</span>
            {/if}
            <span class="title">{annotation.title}</span>
          </button>
          <button
            class="ui-icon-button delete"
            aria-label={`Delete ${annotation.title}`}
            title="Delete"
            onclick={() => (deleting = annotation.id)}><Trash2 size={13} strokeWidth={1.5} aria-hidden="true" /></button
          >
        {/if}
      </li>
    {:else}
      <li class="empty">
        {#if !store.document}
          Open a PDF to annotate it.
        {:else if store.annotations.length}
          No annotation has that in it.
        {:else}
          Select text and ask about it, or screenshot a region with Ctrl Shift S. Its chat is kept here.
        {/if}
      </li>
    {/each}
  </ul>
</aside>

<style>
  .annotation-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    padding: 16px 12px 16px 4px;
    background: var(--color-reader);
    animation: slide 240ms var(--ease-standard);
  }

  h2 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 0 8px;
    color: var(--color-text);
    font: 550 13px var(--font-ui);
  }

  .count {
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
  }

  .filter {
    margin: 0 4px 0 8px;
    padding: 6px 0;
    border: 0;
    border-bottom: 1px solid var(--color-panel-4);
    outline: none;
    background: transparent;
    color: var(--color-text);
    font: 12px var(--font-ui);
  }

  .filter:focus {
    border-bottom-color: var(--color-accent);
  }

  ul {
    display: grid;
    align-content: start;
    gap: 2px;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    outline: none;
    scrollbar-width: thin;
    scrollbar-color: var(--color-panel-5) transparent;
  }

  li {
    position: relative;
    display: flex;
    align-items: flex-start;
    border-radius: 10px;
    transition: background 140ms;
  }

  li:hover,
  li:focus-within {
    background: var(--color-overlay-subtle);
  }

  li.current {
    background: var(--color-overlay-soft);
  }

  .open {
    display: grid;
    gap: 3px;
    flex: 1;
    min-width: 0;
    padding: 8px 4px 9px 8px;
    border: 0;
    background: transparent;
    color: var(--color-subtext);
    text-align: left;
    cursor: pointer;
  }

  .open:focus-visible {
    outline: none;
  }

  .where {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-subsubtext);
    font: 10.5px var(--font-mono);
  }

  .when {
    margin-left: auto;
  }

  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: oklch(0.62 0.14 299 / 0.6);
  }

  [data-state='running'] .dot {
    background: var(--color-accent);
    animation: pulse 1.4s ease-in-out infinite;
  }

  [data-state='failed'] .dot {
    background: var(--color-red);
  }

  .excerpt {
    display: -webkit-box;
    overflow: hidden;
    color: var(--color-subtext);
    font: italic 12px/1.45 var(--font-ui);
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .title {
    overflow: hidden;
    color: var(--color-text);
    font: 450 12.5px/1.4 var(--font-ui);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .delete {
    margin: 6px 4px 0 0;
    opacity: 0;
  }

  li:hover .delete,
  li:focus-within .delete {
    opacity: 1;
  }

  .confirm {
    display: grid;
    gap: 8px;
    width: 100%;
    padding: 10px;
    color: var(--color-text);
    font: 12px/1.4 var(--font-ui);
  }

  .choices {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .empty {
    padding: 4px 8px;
    color: var(--color-subsubtext);
    font: 12px/1.55 var(--font-ui);
  }

  .empty:hover {
    background: none;
  }

  @keyframes slide {
    from {
      opacity: 0;
      transform: translateX(-12px);
    }
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .annotation-list,
    [data-state='running'] .dot {
      animation: none;
    }
  }
</style>
