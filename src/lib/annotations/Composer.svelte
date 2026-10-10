<script lang="ts">
  import ArrowUp from '@lucide/svelte/icons/arrow-up'
  import Square from '@lucide/svelte/icons/square'
  import { thinkingChoices } from './choices'
  import ModelSelect from './ModelSelect.svelte'
  import type { ReaderStore } from './reader-store.svelte'

  /** Where a message is written, and the agent type, model and thinking it goes to. */
  let { store, onsettings }: { store: ReaderStore; onsettings: () => void } = $props()

  const id = $props.id()
  let field: HTMLTextAreaElement

  const annotation = $derived(store.annotation)
  const key = $derived(store.current ?? '')
  const draft = $derived(store.drafts[key] ?? '')
  const model = $derived(annotation ? store.modelOf(annotation.model) : store.newModel)
  const selection = $derived(annotation?.model ?? store.newModel)
  const thinking = $derived(annotation?.thinking ?? store.newThinking)
  const running = $derived(annotation?.state === 'running')
  const ready = $derived(store.signedIn && (annotation !== undefined || (store.newModel !== undefined && store.newType !== undefined)))
  const type = $derived(annotation ? undefined : store.newType)

  export function focus(): void {
    field?.focus({ preventScroll: true })
  }

  $effect(() => {
    void key
    void store.draft
    field?.focus({ preventScroll: true })
  })

  /** The field grows with what is written, up to a third of the window. */
  $effect(() => {
    void draft
    if (!field) return
    field.style.height = 'auto'
    field.style.height = `${Math.min(field.scrollHeight, window.innerHeight / 3)}px`
  })

  async function send(): Promise<void> {
    const text = draft
    if (!text.trim() || !ready) return
    const sentFrom = key
    store.drafts[sentFrom] = ''
    if (!(await store.send(text)) && !store.drafts[sentFrom]) store.drafts[sentFrom] = text
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return
    event.preventDefault()
    void send()
  }
</script>

<form
  class="composer"
  aria-label="Message"
  onsubmit={(event) => {
    event.preventDefault()
    void send()
  }}
>
  {#if store.accounts && !store.signedIn}
    <div class="notice">
      <p>Sign in with ChatGPT or add a Claude login to ask about this.</p>
      <button type="button" class="ui-button" onclick={onsettings}>Open settings</button>
    </div>
  {/if}
  <label class="visually-hidden" for={`${id}-text`}>Message</label>
  <textarea
    id={`${id}-text`}
    bind:this={field}
    rows="1"
    placeholder={annotation ? 'Reply…' : 'Ask about this…'}
    value={draft}
    oninput={(event) => (store.drafts[key] = event.currentTarget.value)}
    onkeydown={keydown}
  ></textarea>
  <div class="controls">
    {#if type && store.types && store.types.types.length > 1}
      <select
        class="ui-input"
        aria-label="Agent type"
        title={type.description || 'Agent type'}
        value={type.id}
        onchange={(event) => store.chooseType(event.currentTarget.value)}
      >
        {#each store.types.types as option (option.id)}<option value={option.id}>{option.name}</option>{/each}
      </select>
    {/if}
    {#if store.models?.length}
      <ModelSelect models={store.models} value={selection} label="Model" onchange={(next) => void store.choose(next, thinking)} />
      {#if model && thinkingChoices(model).length > 1}
        <select
          class="ui-input thinking"
          aria-label="Thinking"
          title="Thinking"
          value={thinking}
          onchange={(event) => void store.choose(model, event.currentTarget.value)}
        >
          {#each thinkingChoices(model) as level (level)}<option value={level}>{level === 'off' ? 'no thinking' : `${level} thinking`}</option>{/each}
        </select>
      {/if}
    {:else if store.modelsProblem}
      <span class="problem" title={store.modelsProblem}>Models unavailable</span>
    {/if}
    <span class="spacer"></span>
    {#if running}
      <button type="button" class="send ui-icon-button" aria-label="Stop" title="Stop" onclick={() => void store.stop()}
        ><Square size={12} strokeWidth={2} fill="currentColor" aria-hidden="true" /></button
      >
    {:else}
      <button type="submit" class="send ui-icon-button" aria-label="Send" title="Send · Enter" disabled={!ready || !draft.trim()}
        ><ArrowUp size={15} strokeWidth={2} aria-hidden="true" /></button
      >
    {/if}
  </div>
</form>

<style>
  .composer {
    display: grid;
    gap: 6px;
    padding: 8px 8px 8px 12px;
    border-radius: 14px;
    background: var(--color-overlay-soft);
    transition: background 160ms;
  }

  .composer:focus-within {
    background: var(--color-overlay-hover);
  }

  textarea {
    width: 100%;
    min-height: 22px;
    max-height: 33vh;
    padding: 2px 0 0;
    border: 0;
    outline: none;
    resize: none;
    background: transparent;
    color: var(--color-text);
    font: 13.5px/1.5 var(--font-ui);
    scrollbar-width: thin;
  }

  textarea::placeholder {
    color: var(--color-subsubtext);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .controls :global(.ui-input) {
    min-width: 0;
    max-width: 150px;
    height: 24px;
    padding: 0 4px;
    border: 0;
    background: transparent;
    color: var(--color-subsubtext);
    font: 11px var(--font-mono);
    cursor: pointer;
  }

  .controls :global(.ui-input:hover),
  .controls :global(.ui-input:focus-visible) {
    color: var(--color-text);
  }

  .spacer {
    flex: 1;
  }

  .send {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--color-text);
    color: var(--color-panel-0);
  }

  .send:hover:not(:disabled) {
    background: var(--color-accent);
    color: var(--color-panel-0);
  }

  .send:disabled {
    background: var(--color-overlay-active);
    color: var(--color-subsubtext);
  }

  .notice {
    display: grid;
    gap: 8px;
    justify-items: start;
    padding: 4px 0 6px;
  }

  .notice p {
    margin: 0;
    color: var(--color-text);
    font: 12px/1.45 var(--font-ui);
  }

  .problem {
    color: var(--color-red);
    font: 11px var(--font-mono);
  }
</style>
