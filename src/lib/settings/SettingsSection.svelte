<script lang="ts">
  import type { Snippet } from 'svelte'

  let { title, description, children }: { title: string; description?: string; children: Snippet } = $props()

  const id = $props.id()
</script>

<section class="settings-section" role="group" aria-labelledby={id}>
  <header>
    <h2 {id}>{title}</h2>
    {#if description}<p>{description}</p>{/if}
  </header>
  <div>{@render children()}</div>
</section>

<style>
  .settings-section {
    width: 100%;
    margin-bottom: 22px;
  }

  header {
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0;
    color: var(--color-subtext);
    font: 550 13px/1.4 var(--font-ui);
  }

  p {
    margin: 5px 0 0;
    color: var(--color-subsubtext);
    font: 10px/1.5 var(--font-mono);
  }
</style>
