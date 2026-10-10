<script lang="ts">
  import FileText from '@lucide/svelte/icons/file-text'
  import type { ReaderStore } from '../annotations/reader-store.svelte'
  import { ago } from '../annotations/time'

  /** What the window shows with no document open: a way to open one, and the recent ones. */
  let { store }: { store: ReaderStore } = $props()

  const folder = (path: string) => path.slice(0, path.lastIndexOf('/')) || '/'
</script>

<section class="welcome" aria-labelledby="welcome-title">
  <h1 id="welcome-title">Open a PDF to read</h1>
  <p>Select a passage, or screenshot a region, and talk it through with an agent right there in the margin.</p>
  <div class="actions">
    <button class="ui-button primary" disabled={store.opening !== undefined} onclick={() => void store.pickDocument()}>Open a PDF…</button>
    <span class="hint">Ctrl O, or drop one on the window</span>
  </div>
  {#if store.opening}
    <p class="status" role="status">Opening {store.opening}…</p>
  {/if}
  {#if store.documentProblem}
    <p class="problem" role="alert">{store.documentProblem}</p>
  {/if}

  {#if store.recent.length}
    <h2>Recent</h2>
    <ul aria-label="Recent documents">
      {#each store.recent as recent (recent.path)}
        <li>
          <button onclick={() => void store.openDocument(recent.path)}>
            <FileText size={15} strokeWidth={1.5} aria-hidden="true" />
            <span class="name">{recent.name}</span>
            <span class="folder"><bdi>{folder(recent.path)}</bdi></span>
            <span class="when">{ago(recent.opened)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .welcome {
    display: grid;
    align-content: start;
    gap: 14px;
    width: min(560px, calc(100% - 48px));
    margin: 18vh auto 0;
  }

  h1 {
    margin: 0;
    color: var(--color-text);
    font: 500 28px/1.25 var(--font-ui);
  }

  p {
    margin: 0;
    color: var(--color-subtext);
    font: 14px/1.55 var(--font-ui);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 6px;
  }

  .hint {
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
  }

  .status {
    color: var(--color-subsubtext);
    font: 12px var(--font-ui);
  }

  .problem {
    color: var(--color-red);
    font: 12px/1.5 var(--font-mono);
    overflow-wrap: anywhere;
  }

  h2 {
    margin: 28px 0 0;
    color: var(--color-subsubtext);
    font: 500 11px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  ul {
    display: grid;
    gap: 2px;
    margin: 0 -10px;
    padding: 0;
    list-style: none;
  }

  li button {
    display: grid;
    grid-template-columns: 16px minmax(0, auto) minmax(0, 1fr) auto;
    align-items: baseline;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    cursor: pointer;
  }

  li button :global(svg) {
    align-self: center;
    color: var(--color-subsubtext);
  }

  li button:hover,
  li button:focus-visible {
    background: var(--color-overlay-soft);
    outline: none;
  }

  .name {
    overflow: hidden;
    font: 450 13px var(--font-ui);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder {
    overflow: hidden;
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .when {
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
  }
</style>
