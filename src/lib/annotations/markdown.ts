import DOMPurify, { type Config } from 'dompurify'
import katex from 'katex'
import { Marked, type TokenizerAndRendererExtension, type Tokens } from 'marked'

/** The complete HTML capability granted to a reply's Markdown, as ErisDE grants an agent's. */
export const MARKDOWN_SANITIZE_OPTIONS: Config = {
  ALLOWED_TAGS: [
    'p',
    'br',
    'strong',
    'em',
    'del',
    'code',
    'pre',
    'blockquote',
    'ul',
    'ol',
    'li',
    'h1',
    'h2',
    'h3',
    'h4',
    'h5',
    'h6',
    'hr',
    'table',
    'thead',
    'tbody',
    'tr',
    'th',
    'td',
    'a',
    'input',
    'span',
  ],
  ALLOWED_ATTR: ['href', 'title', 'start', 'class', 'type', 'checked', 'disabled'],
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP: /^https:\/\//i,
}

/**
 * What KaTeX writes for a formula: spans placed with inline styles, MathML for screen readers
 * and copying, with the TeX as its annotation, and SVG for roots and stretched symbols.
 */
const MATH_SANITIZE_OPTIONS: Config = {
  USE_PROFILES: { html: true, mathMl: true, svg: true },
  ADD_TAGS: ['semantics', 'annotation'],
  ADD_ATTR: ['encoding'],
}

interface MathToken extends Tokens.Generic {
  type: 'math'
  text: string
  display: boolean
}

/**
 * Formulas, inline as `$…$` or `\(…\)` and displayed as `$$…$$` or `\[…\]`. A single `$` opens
 * only before a non-space and closes only after one, not before a digit, as Pandoc reads them,
 * and holds no backtick, so prices stay prices and a `$` in code stays there.
 */
const INLINE = [
  { pattern: /^\$\$([\s\S]+?)\$\$/, display: true },
  { pattern: /^\\\[([\s\S]+?)\\\]/, display: true },
  { pattern: /^\\\(([\s\S]+?)\\\)/, display: false },
  { pattern: /^\$(?!\s)((?:\\.|[^\\$\n`])+?)(?<!\s)\$(?!\d)/, display: false },
]

const BLOCK = [/^ {0,3}\$\$([\s\S]+?)\$\$[ \t]*(?:\n+|$)/, /^ {0,3}\\\[([\s\S]+?)\\\][ \t]*(?:\n+|$)/]

/** The formulas of the reply being rendered, in order; their places in the HTML hold their index. */
let formulas: string[] = []

function slot(token: MathToken): string {
  formulas.push(katex.renderToString(token.text.trim(), { displayMode: token.display, throwOnError: false, output: 'htmlAndMathml' }))
  return `<span class="math-slot">${formulas.length - 1}</span>`
}

const inlineMath: TokenizerAndRendererExtension = {
  name: 'math',
  level: 'inline',
  start: (src) => {
    const at = src.search(/\$|\\\(|\\\[/)
    return at < 0 ? undefined : at
  },
  tokenizer(src) {
    for (const { pattern, display } of INLINE) {
      const match = pattern.exec(src)
      if (match) return { type: 'math', raw: match[0], text: match[1], display } satisfies MathToken
    }
    return undefined
  },
  renderer: (token) => slot(token as MathToken),
}

const blockMath: TokenizerAndRendererExtension = {
  name: 'mathBlock',
  level: 'block',
  start: (src) => src.match(/^ {0,3}(?:\$\$|\\\[)/m)?.index,
  tokenizer(src) {
    for (const pattern of BLOCK) {
      const match = pattern.exec(src)
      if (match) return { type: 'mathBlock', raw: match[0], text: match[1], display: true }
    }
    return undefined
  },
  renderer: (token) => `<p>${slot({ ...(token as MathToken), type: 'math' })}</p>`,
}

const markdown = new Marked({ gfm: true, extensions: [blockMath, inlineMath] })

/**
 * Parses and sanitizes untrusted reply text before it reaches Svelte's HTML. The Markdown is
 * sanitized under its own narrow allowance; each formula's KaTeX output, under the allowance
 * math needs, takes its place afterwards.
 */
export function renderMarkdown(text: string): string {
  formulas = []
  const html = markdown.parse(text, { async: false })
  const rendered = formulas
  formulas = []
  const body = DOMPurify.sanitize(html, { ...MARKDOWN_SANITIZE_OPTIONS, RETURN_DOM_FRAGMENT: true })
  for (const place of body.querySelectorAll('span.math-slot')) {
    const formula = rendered[Number(place.textContent)]
    place.replaceWith(formula === undefined ? '' : DOMPurify.sanitize(formula, { ...MATH_SANITIZE_OPTIONS, RETURN_DOM_FRAGMENT: true }))
  }
  const out = document.createElement('div')
  out.append(body)
  return out.innerHTML
}

/** A link's address when it is https, the only kind replies may open. */
export function httpsUrl(value: string): string | undefined {
  try {
    const parsed = new URL(value)
    return parsed.protocol === 'https:' ? parsed.toString() : undefined
  } catch {
    return undefined
  }
}
