import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test, expect, type Page } from '@playwright/test'
import { calls, lastToolOutput, says, startCodexServer, userImages, userTexts, type CodexCall } from '../support/codex-server'
import { FIXTURE_PAGES, writePdf } from '../support/pdf'
import { launchApp, makeProfile, type Launched } from './electron'

let launched: Launched | undefined
let page: Page
let codex: Awaited<ReturnType<typeof startCodexServer>>
let folder: string
let profile: string
let fixture: string

/** Starts the app against the stand-in ChatGPT on this test's profile, opening `document` if given. */
async function launch(document?: string) {
  launched = await launchApp({ env: codex.env, profile, args: document ? [document] : [] })
  page = launched.page
  // Sign-in pages open in the system browser; the tests answer them themselves.
  await launched.app.evaluate(({ shell }) => {
    shell.openExternal = async () => {}
  })
}

test.beforeEach(async () => {
  codex = await startCodexServer()
  folder = mkdtempSync(join(tmpdir(), 'erisreader-documents-'))
  profile = makeProfile()
  fixture = writePdf(join(folder, 'fixture.pdf'), FIXTURE_PAGES)
})

test.afterEach(async () => {
  await launched?.close()
  launched = undefined
  await codex?.close()
  rmSync(folder, { recursive: true, force: true })
  rmSync(profile, { recursive: true, force: true })
})

const work = () => page.getByRole('main')
const pdfPage = (n: number) => work().getByRole('group', { name: `Page ${n}`, exact: true })
const settingsDialog = () => page.getByRole('dialog', { name: 'Settings' })
const chatGpt = () => settingsDialog().getByRole('group', { name: 'ChatGPT' })
const chat = () => page.getByRole('complementary', { name: 'Chat' })
const message = () => chat().getByRole('textbox', { name: 'Message' })
const conversation = () => chat().getByRole('list', { name: 'Conversation' })
const annotationsSheet = () => page.getByRole('complementary', { name: 'Annotations' })
const annotationList = () => annotationsSheet().getByRole('list', { name: 'Annotations' })
const tool = (name: string) => page.getByRole('button', { name, exact: true })
const highlights = () => work().locator('[data-annotation]')
const sidecar = () => `${fixture}.erisreader`

/** Signs in with ChatGPT from settings, answering the browser's return as ChatGPT would. */
async function signIn() {
  await tool('Settings').click()
  await chatGpt().getByRole('button', { name: 'Sign in with ChatGPT' }).click()
  const link = chatGpt().getByRole('link', { name: 'Open the sign-in page' })
  const state = new URL((await link.getAttribute('href'))!).searchParams.get('state')!
  const callback = await fetch(`http://127.0.0.1:1455/auth/callback?state=${state}&code=fixture-code`)
  expect(callback.status).toBe(200)
  await expect(chatGpt().getByRole('status')).toHaveText('Signed in as fixture@example.test (plus).')
  await page.keyboard.press('Escape')
  await expect(settingsDialog()).toBeHidden()
}

/** Brings `text` into view and selects it in the page's text layer, as dragging over it would. */
async function select(n: number, text: string) {
  await expect(pdfPage(n)).toContainText(text)
  await pdfPage(n).evaluate((element, wanted) => {
    const walker = document.createTreeWalker(element.querySelector('.textLayer')!, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const at = node.textContent!.indexOf(wanted)
      if (at < 0) continue
      node.parentElement!.scrollIntoView({ block: 'center' })
      const range = document.createRange()
      range.setStart(node, at)
      range.setEnd(node, at + wanted.length)
      getSelection()!.removeAllRanges()
      getSelection()!.addRange(range)
      return
    }
    throw new Error(`no text node holds ${wanted}`)
  }, text)
}

/** Clicks the middle of an element, as a person would, whatever lies on top of it. */
async function clickOn(locator: ReturnType<Page['locator']>) {
  const box = (await locator.boundingBox())!
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2)
}

async function say(text: string) {
  await message().fill(text)
  await message().press('Enter')
}

/** Asks about `text` on page `n`, starting an annotation with `question`. */
async function annotate(n: number, text: string, question: string) {
  await select(n, text)
  await clickOn(work().getByRole('button', { name: 'Ask about this' }))
  await expect(chat().getByRole('blockquote')).toHaveText(text)
  await say(question)
}

/** A PNG's width and height, from its header. */
function pngSize(dataUrl: string) {
  expect(dataUrl).toMatch(/^data:image\/png;base64,/)
  const bytes = Buffer.from(dataUrl.split(',')[1], 'base64')
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
}

test('a PDF opens from the welcome, its pages rendered with their text, and reopens on the next start', async () => {
  await launch()
  await expect(work().getByRole('heading', { name: 'Open a PDF to read' })).toBeVisible()
  await launched!.app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = (async () => ({ canceled: false, filePaths: [path] })) as typeof dialog.showOpenDialog
  }, fixture)
  await work().getByRole('button', { name: 'Open a PDF…' }).click()

  await expect(pdfPage(1)).toContainText('The quick brown fox jumps over the lazy dog.')
  await expect(work().getByRole('group', { name: /^Page \d+$/ })).toHaveCount(3)
  await expect(page.getByLabel('Document position')).toHaveText('1 / 3')
  await expect(page).toHaveTitle('fixture.pdf — ErisReader')
  // Reading a document leaves nothing beside it.
  expect(existsSync(sidecar())).toBe(false)

  await launched!.close()
  await launch()
  await expect(pdfPage(1)).toContainText('The quick brown fox jumps over the lazy dog.')

  // Closing it returns to the welcome, which lists it to open again.
  await page.keyboard.press('ControlOrMeta+w')
  await expect(page).toHaveTitle('ErisReader')
  const recent = work().getByRole('list', { name: 'Recent documents' })
  await expect(recent).toContainText(folder)
  await recent.getByRole('button', { name: /fixture\.pdf/ }).click()
  await expect(pdfPage(1)).toContainText('On Reading Closely')
})

test('asking about a selection makes it an annotation, kept beside the PDF with its chat', async () => {
  await launch(fixture)
  await signIn()
  codex.script((call) => says(`About that: ${userTexts(call).at(-1)}`))
  await annotate(1, 'brown fox jumps', 'Why does the fox jump?')

  await expect(conversation()).toContainText('About that: Why does the fox jump?')
  await expect(highlights()).toHaveCount(1)
  expect(existsSync(sidecar())).toBe(true)

  // The chat sits in the page's right margin, level with the passage.
  const [sheet, mark, beside] = [(await pdfPage(1).boundingBox())!, (await highlights().first().boundingBox())!, (await chat().boundingBox())!]
  expect(beside.x).toBeGreaterThanOrEqual(sheet.x + sheet.width)
  expect(Math.abs(beside.y - mark.y)).toBeLessThan(24)

  const [first] = codex.calls
  expect(first.body.instructions).toMatch(/^You help a reader understand/)
  expect(first.body.instructions).toContain('"fixture.pdf", a PDF of 3 pages')
  expect(first.body.instructions).toContain('page 1')
  expect(first.body.instructions).toContain('<passage>\nbrown fox jumps\n</passage>')
  expect(first.body.tools.map((t) => t.name).sort()).toEqual(['read_pages', 'search', 'view_page'])
  expect(userTexts(first)).toEqual(['Why does the fox jump?'])

  await say('And the dog?')
  await expect(conversation()).toContainText('About that: And the dog?')

  await tool('Annotations').click()
  const listed = annotationList().getByRole('button', { name: /^p\. 1 .*Why does the fox jump\?/ })
  await expect(listed).toContainText('brown fox jumps')
  await expect(listed).toContainText('p. 1')
  // The list and the chat sit on either side, both open.
  await expect(chat()).toBeVisible()
})

test('the agent reads, searches and looks at the document through its tools', async () => {
  await launch(fixture)
  await signIn()
  const steps: ((call: CodexCall) => ReturnType<typeof says>)[] = [
    () => calls('read_pages', { from: 2, to: 3 }),
    () => calls('search', { query: 'LAZY   dog' }),
    () => calls('view_page', { page: 2 }),
    () => says('I have read it.'),
  ]
  codex.script((call) => steps[codex.calls.indexOf(call)](call))
  await annotate(1, 'quick brown fox', 'Where else does the dog appear?')
  await expect(conversation()).toContainText('I have read it.')

  const read = lastToolOutput(codex.calls[1]) as string
  expect(read).toContain('[Page 2 of 3]')
  expect(read).toContain('Margins hold the questions of a reader.')
  expect(read).toContain('[Page 3 of 3]')
  expect(read).toContain('The lazy dog sleeps through the afternoon.')
  expect(read).not.toContain('quick brown fox')

  const found = lastToolOutput(codex.calls[2]) as string
  expect(found).toContain('2 matches')
  expect(found).toMatch(/Page 1:.*over the lazy dog/)
  expect(found).toMatch(/Page 3:.*The lazy dog sleeps/)

  const seen = lastToolOutput(codex.calls[3]) as { type: string; text?: string; image_url?: string }[]
  expect(seen.find((part) => part.type === 'input_text')?.text).toContain('Page 2 of 3')
  const image = seen.find((part) => part.type === 'input_image')!.image_url!
  // A letter page, its longer side as long as a model looks at closely.
  expect(pngSize(image)).toEqual({ width: 1212, height: 1568 })
})

test('a screenshot of a region becomes a wireframe annotation, its image sent with the first message', async () => {
  await launch(fixture)
  await signIn()
  await page.keyboard.press('ControlOrMeta+Shift+S')
  await expect(work().getByRole('status')).toHaveText('Drag over a page to screenshot a region. Esc cancels.')

  const box = (await pdfPage(1).boundingBox())!
  await page.mouse.move(box.x + box.width * 0.1, box.y + box.height * 0.05)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.15, { steps: 8 })
  await page.mouse.up()

  await expect(work().getByRole('status')).toBeHidden()
  await expect(chat().getByRole('img', { name: 'Region of page 1' })).toBeVisible()
  // The wireframe frames what was dragged over, where the page now is.
  const now = (await pdfPage(1).boundingBox())!
  const frame = (await work().locator('[data-kind="region"]').first().boundingBox())!
  const near = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThan(3)
  near(frame.x - now.x, box.width * 0.1)
  near(frame.y - now.y, box.height * 0.05)
  near(frame.width, box.width * 0.5)
  near(frame.height, box.height * 0.1)
  await say('What does this region say?')
  await expect(conversation()).toContainText('You said: What does this region say?')

  const [first] = codex.calls
  const [image] = userImages(first)
  const size = pngSize(image)
  expect(size.width).toBeGreaterThan(size.height * 3)
  expect(first.body.instructions).toContain('a region of page 1')
  expect(first.body.instructions).toContain('On Reading Closely')
  await expect(work().locator('[data-annotation][data-kind="region"]')).toHaveCount(1)

  // Escape leaves screenshot mode without drawing.
  await page.keyboard.press('ControlOrMeta+Shift+S')
  await expect(work().getByRole('status')).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(work().getByRole('status')).toBeHidden()
})

/** Draws a region over page `n`, from and to fractions of the page. */
async function drawRegion(n: number, from: { x: number; y: number }, to: { x: number; y: number }) {
  await page.keyboard.press('ControlOrMeta+Shift+S')
  const box = (await pdfPage(n).boundingBox())!
  await page.mouse.move(box.x + box.width * from.x, box.y + box.height * from.y)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width * to.x, box.y + box.height * to.y, { steps: 8 })
  await page.mouse.up()
  await expect(chat().getByRole('img', { name: `Region of page ${n}` })).toBeVisible()
}

test('the chat stays within its page, rising as it grows, however long the conversation', async () => {
  await launch(fixture)
  await signIn()
  const long = Array.from({ length: 30 }, (_, i) => `Paragraph ${i + 1} of a long answer, which goes on for a while to fill the margin.`).join('\n\n')
  codex.script(() => says(long))
  await pdfPage(1).evaluate((element) => element.scrollIntoView({ block: 'end' }))
  await drawRegion(1, { x: 0.1, y: 0.82 }, { x: 0.6, y: 0.94 })
  /** Within its page, and within the window too, while that much of the page is in view. */
  const within = async () => {
    const [sheet, beside] = [(await pdfPage(1).boundingBox())!, (await chat().boundingBox())!]
    const { height } = await page.evaluate(() => ({ height: innerHeight }))
    const onPage = beside.y >= sheet.y - 1 && beside.y + beside.height <= sheet.y + sheet.height + 1
    const inView = beside.y >= -1 && beside.y + beside.height <= height + 1
    return onPage && inView
  }
  // Too near the bottom for the chat to sit level with it, the chat rises to fit the page.
  await expect.poll(within).toBe(true)
  await say('Tell me everything.')
  await expect(conversation()).toContainText('Paragraph 30 of a long answer')
  await expect.poll(within).toBe(true)
  // The conversation scrolls inside it.
  expect(await conversation().evaluate((list) => list.parentElement!.scrollHeight > list.parentElement!.clientHeight)).toBe(true)
  // Scrolled partway down the page, it keeps within it still.
  await page.mouse.move(400, 400)
  await page.mouse.wheel(0, -300)
  await expect.poll(within).toBe(true)
})

test('replies render LaTeX, inline and displayed, and leave prices and code alone', async () => {
  await launch(fixture)
  await signIn()
  codex.script(() =>
    says(
      [
        'Inline, $E = mc^2$ and \\(\\frac{a}{b}\\) both render.',
        '',
        '$$\\sum_{t=0}^{\\infty} \\gamma^t R_t$$',
        '',
        '\\[ G^\\pi(x) \\overset{\\mathcal{D}}{=} R + \\gamma G^\\pi(X\') \\]',
        '',
        'It costs $5 and $10, and `$x$` stays code.',
      ].join('\n'),
    ),
  )
  await annotate(1, 'quick brown fox', 'Show me some math.')
  await expect(conversation()).toContainText('stays code')
  const reply = conversation().locator('.assistant')
  await expect(reply.locator('.katex')).toHaveCount(4)
  await expect(reply.locator('.katex-display')).toHaveCount(2)
  await expect(reply.locator('annotation')).toHaveText(['E = mc^2', '\\frac{a}{b}', '\\sum_{t=0}^{\\infty} \\gamma^t R_t', "G^\\pi(x) \\overset{\\mathcal{D}}{=} R + \\gamma G^\\pi(X')"])
  await expect(reply).toContainText('It costs $5 and $10')
  await expect(reply.locator('code')).toHaveText('$x$')
})

test('annotations reopen with the PDF: from the page, from the list, and are deleted from the list', async () => {
  await launch(fixture)
  await signIn()
  await annotate(2, 'A lantern burns', 'What is the lantern?')
  await expect(conversation()).toContainText('You said: What is the lantern?')
  await launched!.close()

  await launch()
  // The document reopens where it was left, at the annotation.
  await expect(page.getByLabel('Document position')).toHaveText('2 / 3')
  await expect(highlights()).toHaveCount(1)
  await expect(chat()).toBeHidden()
  await clickOn(highlights().first())
  await expect(conversation()).toContainText('You said: What is the lantern?')
  await expect(chat().getByRole('blockquote')).toHaveText('A lantern burns')

  // Escape puts the chat away; the list brings it back.
  await message().press('Escape')
  await expect(chat()).toBeHidden()
  await tool('Annotations').click()
  await annotationList().getByRole('button', { name: /^p\. 2 .*What is the lantern\?/ }).click()
  await expect(conversation()).toContainText('You said: What is the lantern?')

  await annotationList().getByRole('button', { name: 'Delete What is the lantern?' }).click()
  await annotationList().getByRole('group', { name: 'Delete What is the lantern?' }).getByRole('button', { name: 'Delete' }).click()
  await expect(annotationList()).toContainText('Select text and ask about it')
  await expect(highlights()).toHaveCount(0)
  // The last annotation takes its sidecar with it.
  await expect.poll(() => existsSync(sidecar())).toBe(false)
})

test('sign in with ChatGPT from settings, and out again', async () => {
  await launch(fixture)
  await signIn()
  await tool('Settings').click()
  await chatGpt().getByRole('button', { name: 'Sign out' }).click()
  await expect(chatGpt().getByRole('status')).toHaveText('Not signed in.')
})
