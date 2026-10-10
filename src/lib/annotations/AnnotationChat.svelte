<script lang="ts">
  import { tick } from 'svelte'
  import Eye from '@lucide/svelte/icons/eye'
  import FileText from '@lucide/svelte/icons/file-text'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'
  import { anchorRects } from '../document/geometry'
  import { dataUrl, regionImage } from '../document/pdf'
  import Composer from './Composer.svelte'
  import Markdown from './Markdown.svelte'
  import type { OpenDocument, ReaderStore } from './reader-store.svelte'
  import { tokens } from './time'

  /**
   * The open annotation's chat, in the margin beside it: what it marks, the conversation, and
   * where to write. It has no frame of its own; it sits on the reading surface, a thin rule
   * down its side tying it to the page.
   */
  let { store, document: doc, onsettings }: { store: ReaderStore; document: OpenDocument; onsettings: () => void } = $props()

  let scroller: HTMLElement
  let composer: Composer
  /** Whether the view follows the conversation as it grows: until scrolled up from the end. */
  let following = true

  const annotation = $derived(store.annotation)
  const anchor = $derived(store.anchor!)
  const page = $derived(anchorRects(anchor)[0].page + 1)
  const stream = $derived(annotation ? store.streams[annotation.id] : undefined)
  const held = $derived(annotation ? store.held[annotation.id] : null)
  const used = $derived(annotation ? annotation.usage.input + annotation.usage.output : 0)
  const openLink = (url: string) => void store.api.openExternal(url)

  /** A region's picture: the draft's own, or drawn again from the page. */
  let picture = $state<string>()
  $effect(() => {
    if (anchor.kind !== 'region') {
      picture = undefined
      return
    }
    const draft = store.draft
    if (draft?.image) {
      picture = dataUrl(draft.image)
      return
    }
    const rect = anchor.rect
    let current = true
    void doc.pdf.proxy
      .getPage(rect.page + 1)
      .then((page) => regionImage(page, rect, 800))
      .then((image) => {
        if (current) picture = dataUrl(image)
      })
    return () => {
      current = false
    }
  })

  export function focus(): void {
    composer?.focus()
  }

  function scrolled(): void {
    following = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 60
  }

  $effect(() => {
    void store.current
    following = true
  })

  $effect(() => {
    void store.entries
    void stream
    void annotation?.state
    if (!following) return
    void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }))
  })

  const toolIcon = (text: string) => (text.startsWith('Searched') ? Search : text.startsWith('Looked') ? Eye : FileText)
</script>

<aside class="chat" aria-label="Chat" data-kind={anchor.kind}>
  <header>
    {#if anchor.kind === 'text'}
      <blockquote>{anchor.text}</blockquote>
    {:else if picture}
      <img src={picture} alt={`Region of page ${page}`} />
    {:else}
      <div class="picture-pending" role="img" aria-label={`Region of page ${page}`}></div>
    {/if}
    <div class="meta">
      <span>p. {page}{annotation ? ` · ${annotation.agentType.name}` : ''}</span>
      {#if annotation?.contextWindow && annotation.contextTokens}
        <span title={`${annotation.contextTokens.toLocaleString()} of ${annotation.contextWindow.toLocaleString()} tokens of context`}
          >{Math.round((annotation.contextTokens / annotation.contextWindow) * 100)}% context</span
        >
      {/if}
      {#if used}<span title="Tokens this annotation has used, in and out">{tokens(used)} tokens</span>{/if}
      <button class="ui-icon-button close" aria-label="Close chat" title="Close · Esc" onclick={() => store.closeChat()}
        ><X size={14} strokeWidth={1.5} aria-hidden="true" /></button
      >
    </div>
  </header>

  <div class="scroller" bind:this={scroller} onscroll={scrolled}>
    <ol class="entries" aria-label="Conversation" aria-busy={store.loading}>
      {#each store.entries as entry (entry.seq)}
        {#if entry.kind === 'user' || entry.kind === 'mail'}
          <li class="user" data-kind={entry.kind}>{entry.text}</li>
        {:else if entry.kind === 'assistant'}
          <li class="assistant"><Markdown text={entry.text} onlink={openLink} /></li>
        {:else if entry.kind === 'tool'}
          {@const Icon = toolIcon(entry.text)}
          <li class="tool"><Icon size={12} strokeWidth={1.5} aria-hidden="true" />{entry.text}</li>
        {:else if entry.kind === 'reasoning'}
          <li class="aside">
            <details>
              <summary>Thought</summary>
              <Markdown text={entry.text} onlink={openLink} />
            </details>
          </li>
        {:else}
          <li class="aside">
            <details>
              <summary>Earlier messages were summarized</summary>
              <Markdown text={entry.text} onlink={openLink} />
            </details>
          </li>
        {/if}
      {/each}
      {#if annotation?.state === 'running'}
        {#if stream}
          <li class="assistant streaming" aria-live="polite"><Markdown text={stream} onlink={openLink} /></li>
        {:else}
          <li class="status" role="status">
            {held === 'rate-limited' ? 'Waiting out the account’s rate limit…' : held === 'queued' ? 'Waiting for a turn…' : 'Thinking…'}
          </li>
        {/if}
      {:else if annotation?.state === 'failed'}
        <li class="failure" role="alert">
          <strong>The reply failed.</strong>
          <span>{annotation.error ?? 'Something went wrong.'}</span>
          <span class="note">Send a message to try again.</span>
        </li>
      {/if}
    </ol>
  </div>

  {#if store.problem}
    <p class="banner" role="alert">
      <span>{store.problem}</span>
      <button class="ui-icon-button" aria-label="Dismiss" onclick={() => (store.problem = undefined)}
        ><X size={13} strokeWidth={1.5} aria-hidden="true" /></button
      >
    </p>
  {/if}
  <Composer bind:this={composer} {store} {onsettings} />
</aside>

<style>
  .chat {
    position: sticky;
    top: 16px;
    display: flex;
    flex: none;
    flex-direction: column;
    gap: 10px;
    max-height: min(100%, var(--chat-room, calc(100vh - 32px)));
    padding: 2px 0 2px 16px;
    border-left: 2px solid oklch(0.62 0.14 299 / 0.55);
    color: var(--color-text);
    animation: arrive 260ms var(--ease-standard);
  }

  header {
    display: grid;
    gap: 6px;
  }

  blockquote {
    display: -webkit-box;
    overflow: hidden;
    margin: 0;
    color: var(--color-subtext);
    font: italic 13px/1.5 var(--font-ui);
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
  }

  img,
  .picture-pending {
    display: block;
    max-width: 100%;
    max-height: 180px;
    border-radius: 3px;
    background: white;
    object-fit: contain;
    object-position: left;
  }

  .picture-pending {
    height: 64px;
    width: 60%;
    opacity: 0.1;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 22px;
    color: var(--color-subsubtext);
    font: 10.5px var(--font-mono);
  }

  .close {
    margin-left: auto;
  }

  .scroller {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--color-panel-5) transparent;
  }

  .entries {
    display: grid;
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entries:empty {
    display: none;
  }

  .user {
    justify-self: end;
    max-width: 88%;
    padding: 7px 12px;
    border-radius: 14px 14px 4px 14px;
    background: var(--color-overlay-hover);
    color: var(--color-text);
    font: 13.5px/1.5 var(--font-ui);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  [data-kind='mail'].user {
    color: var(--color-subtext);
  }

  .assistant :global(.markdown) {
    font-size: 13.5px;
    line-height: 1.6;
  }

  .tool {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
  }

  .tool + .tool {
    margin-top: -8px;
  }

  .streaming :global(.markdown > :last-child)::after {
    content: '▍';
    margin-left: 2px;
    color: var(--color-accent);
    animation: blink 1s steps(2) infinite;
  }

  details summary {
    width: fit-content;
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
    cursor: pointer;
  }

  details :global(.markdown) {
    margin-top: 6px;
    padding-left: 10px;
    border-left: 1px solid var(--color-panel-4);
    color: var(--color-subtext);
    font-size: 12.5px;
  }

  .status {
    color: var(--color-subsubtext);
    font: 12px var(--font-ui);
    animation: pulse 1.6s ease-in-out infinite;
  }

  .failure {
    display: grid;
    gap: 3px;
    color: var(--color-text);
    font: 12.5px/1.45 var(--font-ui);
  }

  .failure strong {
    color: var(--color-red);
    font-weight: 550;
  }

  .note {
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--color-red);
    font: 12px/1.4 var(--font-ui);
  }

  .banner span {
    flex: 1;
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateX(-8px);
    }
  }

  @keyframes blink {
    to {
      visibility: hidden;
    }
  }

  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .chat,
    .status,
    .streaming :global(.markdown > :last-child)::after {
      animation: none;
    }
  }
</style>
