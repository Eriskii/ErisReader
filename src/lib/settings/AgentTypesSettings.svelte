<script lang="ts">
  import { onMount } from 'svelte'
  import type { Folders } from '../../shared/reader'
  import type { ReaderStore } from '../annotations/reader-store.svelte'
  import SettingsSection from './SettingsSection.svelte'

  /** The agent types in the agents folder, and where ErisReader keeps its data. */
  let { store }: { store: ReaderStore } = $props()

  let folders = $state<Folders>()

  onMount(() => {
    void store.loadTypes()
    void store.api.folders().then((found) => (folders = found))
  })

  const modelName = (provider: string, id: string) => store.modelOf({ provider, id })?.name ?? `${provider}/${id}`
</script>

<SettingsSection
  title="Agent types"
  description="Each Markdown file in the agents folder is a type of agent an annotation can talk to, declared as ErisAE’s .ae/agents/*.md are: YAML frontmatter for its name, model and thinking, then its system prompt. Whatever tools a type declares, an annotation’s agent has only its document: it reads, searches and looks at the pages."
>
  <div class="folder">
    <code title={folders?.agents}><bdi>{folders?.agents ?? '…'}</bdi></code>
    <button type="button" class="ui-button quiet" onclick={() => void store.loadTypes()}>Reload</button>
    <button type="button" class="ui-button" onclick={() => void store.api.reveal('agents')}>Open folder</button>
  </div>
  {#if store.typesProblem}
    <p class="problem" role="alert">{store.typesProblem}</p>
  {:else if store.types}
    <ul class="types" aria-label="Agent types">
      {#each store.types.types as type (type.id)}
        <li>
          <div class="name">
            <strong>{type.name}</strong>
            <span class="id">{type.id}.md{type.id === store.types.defaultType ? ' · default' : ''}</span>
          </div>
          {#if type.description}<p>{type.description}</p>{/if}
          {#if type.model || type.thinkingLevel}
            <p class="details">
              {[
                type.model && modelName(type.model.provider, type.model.id),
                type.thinkingLevel && (type.thinkingLevel === 'off' ? 'no thinking' : `${type.thinkingLevel} thinking`),
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>

<SettingsSection title="Data" description="Agent types, logins, recent documents and the model list, in the system’s place for application data. Each document’s annotations live beside it, in a folder named for it with .erisreader after its name.">
  <div class="folder">
    <code title={folders?.data}><bdi>{folders?.data ?? '…'}</bdi></code>
    <button type="button" class="ui-button" onclick={() => void store.api.reveal('data')}>Open folder</button>
  </div>
</SettingsSection>

<style>
  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 0 8px;
  }

  code {
    overflow: hidden;
    flex: 1;
    min-width: 0;
    color: var(--color-subtext);
    font: 10px/1.5 var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
    /* A long path loses its start, not its end. */
    direction: rtl;
    text-align: left;
  }

  bdi {
    direction: ltr;
  }

  .types {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    gap: 3px;
    padding: 8px 0;
    border-bottom: 1px solid var(--color-panel-3);
  }

  .name {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  strong {
    color: var(--color-subtext);
    font: 450 12px/1.35 var(--font-ui);
  }

  .id,
  .details {
    color: var(--color-subsubtext);
    font: 10px/1.5 var(--font-mono);
  }

  p {
    margin: 0;
    color: var(--color-subtext);
    font: 11px/1.5 var(--font-ui);
  }

  .problem {
    color: var(--color-red);
    font: 10px/1.5 var(--font-mono);
    white-space: pre-wrap;
  }
</style>
