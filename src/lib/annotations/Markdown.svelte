<script lang="ts">
  import 'katex/dist/katex.min.css'
  import { httpsUrl, renderMarkdown } from './markdown'

  /** A reply's Markdown, sanitized, with its formulas typeset; its https links open in the browser. */
  let { text, onlink }: { text: string; onlink: (url: string) => void } = $props()

  const html = $derived(renderMarkdown(text))

  function links(node: HTMLElement) {
    const open = (event: MouseEvent) => {
      const link = event.target instanceof Element ? event.target.closest('a') : null
      if (!link || !node.contains(link)) return
      event.preventDefault()
      const url = httpsUrl(link.getAttribute('href') ?? '')
      if (url) onlink(url)
    }
    node.addEventListener('click', open)
    return { destroy: () => node.removeEventListener('click', open) }
  }
</script>

<div class="markdown" use:links>{@html html}</div>

<style>
  .markdown {
    min-width: 0;
    color: var(--color-text);
    font: 14px/1.6 var(--font-ui);
    overflow-wrap: anywhere;
  }

  .markdown :global(:first-child) {
    margin-top: 0;
  }

  .markdown :global(:last-child) {
    margin-bottom: 0;
  }

  .markdown :global(:is(p, ul, ol, blockquote, pre, table)) {
    margin: 0 0 12px;
  }

  .markdown :global(:is(h1, h2, h3, h4, h5, h6)) {
    margin: 20px 0 8px;
    font-weight: 600;
    line-height: 1.3;
  }

  .markdown :global(h1) {
    font-size: 20px;
  }

  .markdown :global(h2) {
    font-size: 17px;
  }

  .markdown :global(:is(h3, h4, h5, h6)) {
    font-size: 15px;
  }

  .markdown :global(:is(ul, ol)) {
    padding-left: 22px;
  }

  .markdown :global(li + li) {
    margin-top: 4px;
  }

  .markdown :global(a) {
    color: var(--color-accent);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--color-accent) 40%, transparent);
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .markdown :global(code) {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--color-panel-3);
    font: 12.5px/1.5 var(--font-mono);
  }

  .markdown :global(pre) {
    overflow-x: auto;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--color-panel-0);
    scrollbar-width: thin;
  }

  .markdown :global(pre code) {
    padding: 0;
    background: none;
    white-space: pre;
  }

  .markdown :global(blockquote) {
    padding-left: 12px;
    border-left: 2px solid var(--color-panel-5);
    color: var(--color-subtext);
  }

  .markdown :global(hr) {
    margin: 16px 0;
    border: 0;
    border-top: 1px solid var(--color-panel-4);
  }

  .markdown :global(table) {
    display: block;
    overflow-x: auto;
    border-collapse: collapse;
    font-size: 13px;
  }

  .markdown :global(:is(th, td)) {
    padding: 5px 10px;
    border: 1px solid var(--color-panel-4);
    text-align: left;
  }

  .markdown :global(.katex) {
    font-size: 1.08em;
  }

  /* A displayed formula wider than the margin scrolls across rather than spilling out. */
  .markdown :global(.katex-display) {
    margin: 10px 0;
    padding: 2px 0;
    overflow: auto hidden;
    scrollbar-width: thin;
  }

  .markdown :global(.katex-error) {
    color: var(--color-red);
    font: 12.5px var(--font-mono);
  }

  .markdown :global(th) {
    background: var(--color-panel-2);
    font-weight: 600;
  }
</style>
