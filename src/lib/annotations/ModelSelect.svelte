<script lang="ts">
  import type { Model, ModelSelection } from '../../shared/reader'
  import { modelKey, parseModelKey } from './choices'

  /** ErisDE's model picker: the signed-in accounts' models, grouped by account. */
  let {
    models,
    value,
    label,
    disabled = false,
    onchange,
  }: {
    models: readonly Model[]
    value: ModelSelection | undefined
    label: string
    disabled?: boolean
    onchange: (model: ModelSelection) => void
  } = $props()

  const PROVIDERS: Record<string, string> = { 'openai-codex': 'ChatGPT', anthropic: 'Claude' }

  const groups = $derived.by(() => {
    const groups = new Map<string, Model[]>()
    for (const model of models) groups.set(model.provider, [...(groups.get(model.provider) ?? []), model])
    return [...groups]
  })

  const contextWindow = (tokens: number) =>
    tokens >= 1_000_000 ? `${Number((tokens / 1_000_000).toFixed(1))}m` : `${Math.round(tokens / 1_000)}k`
</script>

<select
  class="ui-input"
  aria-label={label}
  title={label}
  {disabled}
  value={value ? modelKey(value) : undefined}
  onchange={(event) => onchange(parseModelKey(event.currentTarget.value))}
>
  {#each groups as [provider, options] (provider)}
    <optgroup label={PROVIDERS[provider] ?? provider}>
      {#each options as model (modelKey(model))}
        <option value={modelKey(model)}
          >{model.contextWindow ? `${model.name} · ${contextWindow(model.contextWindow)}` : model.name}</option
        >
      {/each}
    </optgroup>
  {/each}
</select>
