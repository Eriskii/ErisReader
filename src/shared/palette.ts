/**
 * ErisDE's colour table: neutral surfaces and text in OKLCH lightness steps,
 * and accents sharing the accent's lightness and chroma. The renderer writes
 * each token to the document as `--color-<token>`.
 */

export interface AccentColor {
  colorSpace: 'oklch'
  lightness: number
  chroma: number
  hue: number
}

export interface ColorValue extends AccentColor {
  alpha: number
}

type AccentChannel = 'accent'

interface ColorDefinition {
  lightness: number | AccentChannel
  chroma: number | AccentChannel
  hue: number | AccentChannel
  alpha?: number
}

export const ACCENT_ANCHOR = {
  colorSpace: 'oklch',
  lightness: 0.748855276704438,
  chroma: 0.10189711449361627,
  hue: 299.68672581256214,
} as const satisfies AccentColor

const grayscale = (lightness: number, alpha?: number): ColorDefinition => ({
  lightness,
  chroma: 0,
  hue: 0,
  ...(alpha === undefined ? {} : { alpha }),
})

const accent = (hue: number | AccentChannel = 'accent'): ColorDefinition => ({ lightness: 'accent', chroma: 'accent', hue })

export const COLOR_TABLE = {
  reader: grayscale(0.18),
  'panel-0': grayscale(0.15),
  'panel-1': grayscale(0.18),
  'panel-2': grayscale(0.21),
  'panel-3': grayscale(0.24),
  'panel-4': grayscale(0.29),
  'panel-5': grayscale(0.36),
  'panel-6': grayscale(0.47),
  text: grayscale(0.95),
  subtext: grayscale(0.76),
  subsubtext: grayscale(0.58),
  accent: accent(),
  green: accent(145),
  blue: accent(255),
  red: accent(25),
  teal: accent(195),
  yellow: accent(100),
  orange: accent(65),
  'overlay-subtle': grayscale(1, 0.025),
  'overlay-soft': grayscale(1, 0.04),
  'overlay-hover': grayscale(1, 0.07),
  'overlay-active': grayscale(1, 0.1),
  'overlay-border': grayscale(1, 0.16),
  'overlay-focus': grayscale(1, 0.25),
  shadow: grayscale(0, 0.38),
  'shadow-strong': grayscale(0, 0.55),
} as const satisfies Record<string, ColorDefinition>

export type ColorToken = keyof typeof COLOR_TABLE

/** The reading surface in sRGB, for the window's background before the renderer paints. */
export const READER_HEX = '#121212'

export function resolveColors(accentColor: AccentColor = ACCENT_ANCHOR): Record<ColorToken, ColorValue> {
  return Object.fromEntries(
    Object.entries(COLOR_TABLE).map(([token, definition]: [string, ColorDefinition]) => [
      token,
      {
        colorSpace: 'oklch',
        lightness: definition.lightness === 'accent' ? accentColor.lightness : definition.lightness,
        chroma: definition.chroma === 'accent' ? accentColor.chroma : definition.chroma,
        hue: definition.hue === 'accent' ? accentColor.hue : definition.hue,
        alpha: definition.alpha ?? 1,
      },
    ]),
  ) as Record<ColorToken, ColorValue>
}

const number = (value: number): string => Number(value.toFixed(6)).toString()

export function formatColor(color: ColorValue): string {
  const alpha = color.alpha === 1 ? '' : ` / ${number(color.alpha)}`
  return `oklch(${number(color.lightness)} ${number(color.chroma)} ${number(color.hue)}${alpha})`
}
