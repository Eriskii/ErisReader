import { ACCENT_ANCHOR, formatColor, resolveColors, type AccentColor, type ColorValue } from '../../shared/palette'

// Chromium rounds OKLCH differently in painted tiles and composited layers.
// Resolving display channels once keeps layer promotion from shifting the palette.
function paintColor(sample: OffscreenCanvasRenderingContext2D, color: ColorValue): string {
  sample.fillStyle = formatColor({ ...color, alpha: 1 })
  sample.fillRect(0, 0, 1, 1)
  const [red, green, blue] = sample.getImageData(0, 0, 1, 1).data
  return `rgba(${red}, ${green}, ${blue}, ${color.alpha})`
}

/** Writes every palette token to the document as a `--color-<token>` property. */
export function applyColors(accent: AccentColor = ACCENT_ANCHOR): void {
  const sample = new OffscreenCanvas(1, 1).getContext('2d', { alpha: false, colorSpace: 'srgb', willReadFrequently: true })!
  for (const [token, color] of Object.entries(resolveColors(accent))) {
    document.documentElement.style.setProperty(`--color-${token}`, paintColor(sample, color))
  }
}
