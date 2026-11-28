// The ribbon in the default Single line mode (plan "Doc usable by hand", stage 0A; walkthrough G1,
// J4, the timed task's second and third stops; WPS W-W1): each of the sixteen menus a row button
// opens shows its items, and choosing one does what it says: the first bullet makes the paragraph
// a list, the table grid's third row and column insert a 3 × 3 table; a style chosen from the
// Styles panel closes the panel and leaves the keyboard in the text.
// node tests/hand/menus.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** The sixteen menus of Single line's rows: tab, test id. */
const MENUS = [
  ['home', 'doc-paste'],
  ['home', 'doc-highlight'],
  ['home', 'doc-font-color'],
  ['home', 'doc-bullets'],
  ['home', 'doc-numbering'],
  ['insert', 'doc-table'],
  ['insert', 'doc-chart-insert'],
  ['insert', 'doc-drawing-shapes'],
  ['insert', 'doc-symbol'],
  ['insert', 'doc-header'],
  ['insert', 'doc-page-number'],
  ['layout', 'doc-margins'],
  ['layout', 'doc-orientation'],
  ['layout', 'doc-page-size'],
  ['layout', 'doc-columns'],
  ['layout', 'doc-breaks'],
]

/** Opens the menu of the row button `testId` on tab `tab`; answers it after checking its rows show. */
async function openMenu(e, tab, testId) {
  const { kit, wait } = e
  await kit.getByTestId(`ribbon-tab-${tab}`).click()
  await wait(250)
  assert.equal(await kit.locator('.kui-app').getAttribute('data-mode'), 'line', 'the editor is in Single line')
  const trigger = kit.locator('.kui-line-inline').getByTestId(testId)
  assert.equal(await trigger.count(), 1, `${testId} is in the row itself`)
  await trigger.click()
  const menu = kit.locator('.kui-line-inline .kui-popover').first()
  await e.until(() => menu.isVisible(), 30)
  const box = await menu.boundingBox()
  assert(box && box.height > 20, `${testId}: the menu has a height (${box?.height})`)
  const rows = await menu.locator('button, [role=menuitem], input').evaluateAll((items) => items.map((item) => item.getBoundingClientRect().height))
  assert(rows.length > 0 && rows.every((height) => height > 0), `${testId}: every item shows (${rows.length} items, heights ${[...new Set(rows)].join('/')})`)
  return menu
}
const closeMenus = async (e) => {
  await e.page.keyboard.press('Escape')
  await e.until(async () => (await e.kit.locator('.kui-popover').count()) === 0, 20)
}
/** Setup: the person's caret in body paragraph `index`, at its start. */
const caretIn = async (e, index) => {
  await e.setup('caret.set', { pos: { story: 'body', path: [index], off: 0 } })
  await e.wait(200)
}

const list = [
  ['g1-single-line-menus', async (e) => {
    await caretIn(e, 2)
    for (const [tab, testId] of MENUS) {
      await openMenu(e, tab, testId)
      await closeMenus(e)
    }
  }],
  ['g1-bullets-from-the-row', async (e) => {
    await caretIn(e, 2)
    const before = await e.paragraph(2)
    assert(!before.paragraph.props?.numbering, 'paragraph 3 is no list yet')
    const menu = await openMenu(e, 'home', 'doc-bullets')
    await menu.getByRole('menuitem').first().click()
    await e.until(async () => Boolean((await e.paragraph(2)).paragraph.props?.numbering))
    assert((await e.paragraph(2)).paragraph.props?.numbering, 'the first bullet makes paragraph 3 a list')
    assert.equal(await e.kit.locator('.kui-popover').count(), 0, 'the menu closed')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => !(await e.paragraph(2)).paragraph.props?.numbering)
  }],
  ['g1-table-from-the-row', async (e) => {
    await caretIn(e, 2)
    const tables = async () => ((await e.ask('file.inspect', { text: true })).blocks ?? []).filter((block) => block.type === 'table')
    const before = (await tables()).length
    const menu = await openMenu(e, 'insert', 'doc-table')
    await menu.getByTestId('kui-table-3x3').click()
    await e.until(async () => (await tables()).length === before + 1)
    const added = await tables()
    assert.equal(added.length, before + 1, 'a table was inserted')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await tables()).length === before)
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['g7-styles-panel-closes', async (e) => {
    await caretIn(e, 2)
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.kit.getByTestId('kui-line-group-Styles').click()
    const heading = e.kit.getByTestId('doc-style-Heading1')
    await e.until(() => heading.isVisible(), 30)
    await heading.click()
    await e.until(async () => (await e.read('format.state'))?.styleName === 'Heading 1')
    assert.equal((await e.read('format.state'))?.styleName, 'Heading 1', 'Heading 1 applies')
    await e.until(async () => (await e.kit.locator('.kui-drop-panel').count()) === 0, 30)
    assert.equal(await e.kit.locator('.kui-drop-panel').count(), 0, 'the Styles panel closed by itself')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the text has the keyboard')
    await e.page.keyboard.press('ControlOrMeta+z')
  }],
]

await segments('menus', list, (which) => open({ which }))
