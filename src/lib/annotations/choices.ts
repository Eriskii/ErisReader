/** Which agent type, model and thinking level a new annotation starts with, kept between sessions. */
import type { AgentType, Model, ModelSelection } from '../../shared/reader'

export interface NewAnnotationChoice {
  type: string | null
  model: ModelSelection | null
  thinking: string | null
}

export const CHOICE_KEY = 'erisreader.new-annotation'

type ChoiceStorage = Pick<Storage, 'getItem' | 'setItem'>

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value)
const text = (value: unknown) => (typeof value === 'string' ? value : null)

export function loadChoice(storage: ChoiceStorage): NewAnnotationChoice {
  let saved: unknown
  try {
    saved = JSON.parse(storage.getItem(CHOICE_KEY) ?? 'null')
  } catch {
    saved = null
  }
  if (!isRecord(saved)) return { type: null, model: null, thinking: null }
  const model = isRecord(saved.model) && text(saved.model.provider) && text(saved.model.id) ? (saved.model as unknown as ModelSelection) : null
  return { type: text(saved.type), model: model && { provider: model.provider, id: model.id }, thinking: text(saved.thinking) }
}

export function saveChoice(storage: ChoiceStorage, choice: NewAnnotationChoice): void {
  try {
    storage.setItem(CHOICE_KEY, JSON.stringify(choice))
  } catch {
    // New annotations still start; they use the defaults next time.
  }
}

export const sameModel = (a: ModelSelection | null | undefined, b: ModelSelection | null | undefined): boolean =>
  !!a && !!b && a.provider === b.provider && a.id === b.id

/** A model as a select's value. */
export const modelKey = ({ provider, id }: ModelSelection): string => `${encodeURIComponent(provider)}/${encodeURIComponent(id)}`

export function parseModelKey(key: string): ModelSelection {
  const [provider = '', id = ''] = key.split('/')
  return { provider: decodeURIComponent(provider), id: decodeURIComponent(id) }
}

/** `off`, which every model takes, then the reasoning efforts the model lists. */
export const thinkingChoices = (model: Pick<Model, 'thinkingLevels'> | undefined): string[] =>
  model ? ['off', ...model.thinkingLevels.filter((level) => level !== 'off')] : []

/**
 * The model a new annotation of `type` gets: the type's own when an account offers it, else the
 * last one chosen, else the first offered.
 */
export function pickModel(models: readonly Model[], type: AgentType | undefined, chosen: ModelSelection | null): Model | undefined {
  return models.find((m) => sameModel(m, type?.model)) ?? models.find((m) => sameModel(m, chosen)) ?? models[0]
}

/** The thinking level for `model`: the preferred one if it takes it, else medium, else off. */
export function pickThinking(model: Model | undefined, ...preferred: (string | null | undefined)[]): string {
  const choices = thinkingChoices(model)
  return preferred.find((level) => level && choices.includes(level)) ?? (choices.includes('medium') ? 'medium' : 'off')
}
