import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test, expect, type Page } from '@playwright/test'
import { FIXTURE_PAGES, writePdf } from '../support/pdf'
import { launchApp, type Launched } from './electron'

let launched: Launched
let page: Page
let folder: string

test.beforeEach(async () => {
  folder = mkdtempSync(join(tmpdir(), 'erisreader-documents-'))
  const fixture = writePdf(join(folder, 'fixture.pdf'), FIXTURE_PAGES)
  launched = await launchApp({ args: [fixture] })
  page = launched.page
  await expect(firstPage()).toContainText('On Reading Closely')
})

test.afterEach(async () => {
  await launched?.close()
  rmSync(folder, { recursive: true, force: true })
})

const reader = () => page.getByRole('main')
const firstPage = () => reader().getByRole('group', { name: 'Page 1', exact: true })
const list = () => page.getByRole('complementary', { name: 'Annotations' })
const button = (name: string) => page.getByRole('button', { name, exact: true })
const settingsDialog = () => page.getByRole('dialog', { name: 'Settings' })

test.describe('the reading view', () => {
  test('the document fills the window, its pages centred, with no frame around it', async () => {
    await expect(firstPage()).toContainText('On Reading Closely')
    const window = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }))
    expect(await reader().boundingBox()).toEqual({ x: 0, y: 0, ...window })
    const sheet = (await firstPage().boundingBox())!
    expect(Math.abs(sheet.x + sheet.width / 2 - window.width / 2)).toBeLessThanOrEqual(8)
  })

  test('the corner button opens the annotations on the left, the page moving clear of them', async () => {
    await button('Annotations').click()
    await expect(list()).toBeVisible()
    await expect(list().getByRole('list', { name: 'Annotations' })).toContainText('Select text and ask about it')
    await expect
      .poll(async () => {
        const [side, sheet] = [(await list().boundingBox())!, (await firstPage().boundingBox())!]
        return side.x <= 56 && side.x + side.width <= sheet.x
      })
      .toBe(true)

    await button('Annotations').click()
    await expect(list()).toBeHidden()
  })

  test('opening the annotations redraws each page once, at its new size', async () => {
    // The pages in reach have drawn.
    await page.waitForTimeout(500)
    await page.evaluate(() => {
      const counted = window as unknown as { drawn: number }
      counted.drawn = 0
      new MutationObserver((records) => {
        for (const record of records) for (const node of record.addedNodes) if (node.nodeName === 'CANVAS') counted.drawn++
      }).observe(document.querySelector('main')!, { childList: true, subtree: true })
    })
    const before = (await firstPage().boundingBox())!.width
    await button('Annotations').click()
    await expect(list()).toBeVisible()
    await expect.poll(async () => (await firstPage().boundingBox())!.width).toBeLessThan(before - 20)
    await page.waitForTimeout(600)
    const drawn = await page.evaluate(() => (window as unknown as { drawn: number }).drawn)
    // Each page in reach, once.
    expect(drawn).toBeGreaterThan(0)
    expect(drawn).toBe(await reader().locator('canvas').count())
  })

  test('Ctrl K (⌘K on macOS) opens and closes the annotations, and Escape closes them', async () => {
    await page.keyboard.press('ControlOrMeta+k')
    await expect(list()).toBeVisible()
    await page.keyboard.press('ControlOrMeta+k')
    await expect(list()).toBeHidden()
    await page.keyboard.press('ControlOrMeta+k')
    await page.keyboard.press('Escape')
    await expect(list()).toBeHidden()
  })

  test('zooming scales the pages, and the position follows the scroll', async () => {
    await expect(page.getByLabel('Document position')).toHaveText('1 / 3')
    const before = (await firstPage().boundingBox())!.width
    await button('Zoom in').click()
    await expect.poll(async () => (await firstPage().boundingBox())!.width).toBeGreaterThan(before * 1.05)
    await reader().getByRole('group', { name: 'Page 3', exact: true }).scrollIntoViewIfNeeded()
    await expect(page.getByLabel('Document position')).toHaveText('3 / 3')
  })
})

test.describe('settings', () => {
  test('open in a dialog; Escape closes it, returning to the settings button', async () => {
    await button('Settings').click()
    await expect(settingsDialog()).toBeVisible()
    await expect(settingsDialog().getByRole('group', { name: 'ChatGPT' }).getByRole('status')).toHaveText('Not signed in.')
    await expect(settingsDialog().getByRole('group', { name: 'Claude' }).getByRole('status')).toHaveText('Not signed in.')
    await page.keyboard.press('Escape')
    await expect(settingsDialog()).toBeHidden()
    await expect(button('Settings')).toBeFocused()
  })

  test('show the agents folder and the default agent type it starts with', async () => {
    await button('Settings').click()
    const types = settingsDialog().getByRole('list', { name: 'Agent types' })
    await expect(types).toContainText('Reader')
    await expect(types).toContainText('default.md · default')
    await expect(settingsDialog().getByText(`${launched.profile}/data/agents`)).toBeVisible()
  })
})
