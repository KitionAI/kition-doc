// The button census (plan "Doc usable by hand", stage 6A item 7; the walkthrough's probe14, "what
// we say we do, we do"): every button with a test id in each tab's Single line row and in its More
// panels, clicked once with the caret in the text (the table tabs with it in a table, Picture
// Format with a picture selected), either changes what a person sees (the pages, panes and status
// bar under the ribbon, or the ribbon's own state: pressed buttons, the tab, the font boxes), opens
// something (a menu, a dialog, a notice), is grey, or is in EXEMPT with the reason it rightly shows
// nothing there. Anything else fails, named.
// node tests/hand/census.mjs [chromium|webkit] [tab…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Buttons that rightly show nothing in the census's document, and why. */
const EXEMPT = {
  'doc-cmd-format.alignLeft': 'the paragraph is already left-aligned (its button is pressed)',
  'doc-cmd-edit.copy': 'copying shows nothing (WPS neither); the clipboard has it',
  'doc-cmd-edit.formatPainter': 'held, it shows on the pointer over the text and its button, inside the closed More panel',
  'doc-cmd-format.clear': 'the text has no formatting to clear',
  'doc-cmd-paragraph.outdent': 'the paragraph has no indent to take away',
  'doc-style-Normal': 'the paragraph is Normal already',
  'doc-picture': 'the host opens its file picker (dev.html has none; Kition does)',
  'doc-cmd-ink.hide': 'the document has no ink to hide',
  'doc-cmd-table.borderPainter': 'the table already has all its borders',
  'doc-cmd-table.viewGridlines': 'gridlines show on a table without borders; this one has them',
  'doc-cmd-table.distributeColumns': 'the columns are equal already',
  'doc-cmd-table.repeatHeader': 'the table fits on one page: there is no next page to repeat it on',
  'doc-cmd-image.compress': 'the picture is smaller than the compression size',
  'doc-cmd-image.reset': 'the picture has no changes to reset',
  'doc-cmd-view.printLayout': 'the view is Print Layout already (its button is pressed)',
}
/** Buttons not clicked: they leave the editor or take the machine's speech. */
const SKIP = /file\.(print|close|new|open|share)|mailMerge|readAloud|view\.(newWindow|immersive|focus)/

const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
const at = (path, off = 0) => ({ story: 'body', path, off })

/** Setup (the agent's): two paragraphs of text, a picture, a 2 × 2 table, a last paragraph. */
async function census(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: 'alpha beta gamma' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'second paragraph here' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'after' } },
    { command: 'select.range', params: { anchor: at([3]), focus: at([3]) } },
    { command: 'table.insert', params: { rows: 2, cols: 2 } },
    { command: 'select.range', params: { anchor: at([3, 0, 0, 0]), focus: at([3, 0, 0, 0]) } },
    { command: 'text.insert', params: { text: 'b', raw: true } },
    { command: 'select.range', params: { anchor: at([3, 1, 0, 0]), focus: at([3, 1, 0, 0]) } },
    { command: 'text.insert', params: { text: 'a', raw: true } },
  ])
  await e.until(async () => (await e.ask('file.paragraph', { path: [3, 0, 0, 0] }))?.paragraph != null)
  // A picture is the person's to insert (an agent gives none by data in a hosted session).
  await e.setup('caret.set', { pos: at([2]) })
  await e.setup('image.insert', { data: PNG })
  await e.setup('select.range', { anchor: at([2]), focus: at([2], 3) })
  await e.setup('image.size', { width: 120, height: 80, lockAspect: false })
}
/** The census's document and view as at the start. */
async function reset(e) {
  const view = await e.read('view.state')
  if (view?.mode !== 'print' || view?.readMode) await e.setup('view.printLayout')
  if (view?.marks) await e.setup('view.marks', { value: false })
  await e.setup('change.track', { value: false })
  await census(e)
}
const PLACES = {
  body: (e) => e.setup('caret.set', { pos: at([0], 3) }),
  table: (e) => e.setup('caret.set', { pos: at([3, 0, 0, 0]) }),
  picture: (e) => e.setup('select.range', { anchor: at([2]), focus: at([2], 3) }),
}
const TABS = [
  ['home', 'body'],
  ['insert', 'body'],
  ['design', 'body'],
  ['layout', 'body'],
  ['references', 'body'],
  ['review', 'body'],
  ['view', 'body'],
  ['tableDesign', 'table'],
  ['tableLayout', 'table'],
  ['pictureFormat', 'picture'],
]

/** What a person sees: the frame under the ribbon (pages, panes, status bar), and the ribbon's state. */
async function look(e) {
  await e.page.mouse.move(2, 2)
  await e.wait(220)
  const frame = await e.page.locator('#kit').boundingBox()
  const surface = await e.surface()
  const png = await e.page.screenshot({ clip: { x: frame.x, y: surface.y, width: frame.width, height: frame.y + frame.height - surface.y } })
  const ribbon = await e.frame().evaluate(() =>
    JSON.stringify([
      [...document.querySelectorAll('[data-testid^=ribbon-tab-]')].map((tab) => tab.dataset.testid + (tab.getAttribute('aria-selected') ?? '')),
      [...document.querySelectorAll('.kui-app button[aria-pressed]')].filter((button) => button.offsetParent !== null).map((button) => `${button.dataset.testid ?? button.getAttribute('aria-label')}=${button.getAttribute('aria-pressed')}`),
      [...document.querySelectorAll('.kui-app select')].filter((select) => select.offsetParent !== null).map((select) => select.value),
      document.querySelector('[data-testid=kit-zoom]')?.textContent,
    ]),
  )
  return { png, ribbon }
}
/** What opened: menus, dialogs and notices. */
const opened = (e) =>
  e.frame().evaluate(() => [...document.querySelectorAll('[role=menu], [role=listbox], .kui-popover, [role=dialog], .kui-toast')].filter((item) => item.getBoundingClientRect().width > 0 && !item.classList.contains('kui-line-popover')).length)
const closeAll = async (e) => {
  for (let index = 0; index < 3; index += 1) {
    await e.page.keyboard.press('Escape')
    await e.wait(60)
  }
  await e.frame().evaluate(() => {
    for (const toast of document.querySelectorAll('.kui-toast-close')) toast.click()
  })
}

async function tabCensus(e, tab, place) {
  await e.kit.getByTestId(`ribbon-tab-home`).click()
  await PLACES[place](e)
  await e.until(async () => (await e.kit.getByTestId(`ribbon-tab-${tab}`).count()) > 0, 30)
  await e.kit.getByTestId(`ribbon-tab-${tab}`).click()
  await e.wait(300)
  const row = await e.frame().evaluate(() =>
    [...new Set([...document.querySelectorAll('.kui-ribbon button, .kui-line button, [class*=ribbon] button')].filter((button) => button.offsetParent !== null && button.dataset.testid && !button.dataset.testid.startsWith('ribbon-tab-') && !['doc-undo', 'doc-redo', 'doc-save', 'doc-mode', 'doc-ask-agent'].includes(button.dataset.testid)).map((button) => button.dataset.testid))],
  )
  const more = row.filter((id) => id.startsWith('kui-line-more-') || id.startsWith('kui-line-group-'))
  const targets = row.filter((id) => !more.includes(id)).map((id) => ({ id, via: null }))
  for (const panel of more) {
    // The place again for each panel: the Escapes that close the one before may have let the
    // picture go, and its tab with it (stage 5A: Picture Format follows the picture).
    await PLACES[place](e)
    await e.kit.getByTestId(`ribbon-tab-${tab}`).click()
    await e.kit.getByTestId(panel).click()
    await e.wait(250)
    const inside = await e.frame().evaluate(() => [...document.querySelectorAll('.kui-line-popover button')].filter((button) => button.getBoundingClientRect().height > 0 && button.dataset.testid).map((button) => button.dataset.testid))
    for (const id of inside) if (!targets.some((target) => target.id === id)) targets.push({ id, via: panel })
    await closeAll(e)
  }
  const silent = []
  let counted = 0
  const tally = { shows: 0, grey: 0, exempt: 0, silent: 0, skipped: 0 }
  for (const target of targets) {
    if (SKIP.test(target.id)) {
      tally.skipped += 1
      continue
    }
    counted += 1
    await PLACES[place](e)
    await e.kit.getByTestId(`ribbon-tab-${tab}`).click()
    const before = await look(e)
    if (target.via) {
      await e.kit.getByTestId(target.via).click()
      await e.wait(220)
    }
    const scope = target.via ? e.kit.locator('.kui-line-popover') : e.kit.locator('.kui-app')
    const button = scope.locator(`[data-testid="${target.id}"]:visible`).first()
    if (!(await button.count()) || (await button.isDisabled())) {
      tally.grey += 1
      await closeAll(e)
      continue
    }
    await button.click({ timeout: 2000 }).catch(() => undefined)
    // A notice may come a moment after the engine's answer.
    let shown = await e.until(async () => (await opened(e)) > 0, 8, 100)
    if (!shown && target.via && (await e.kit.locator('.kui-line-popover').count())) {
      await e.page.mouse.click(2, 2)
      await e.wait(200)
    }
    if (!shown) {
      const after = await look(e)
      shown = !after.png.equals(before.png) || after.ribbon !== before.ribbon
    }
    if (shown) tally.shows += 1
    else if (EXEMPT[target.id]) tally.exempt += 1
    else {
      tally.silent += 1
      silent.push(`${target.id}${target.via ? ' (More)' : ''}`)
    }
    await closeAll(e)
    // What a button changed is put back for the next one: the document, the view.
    if (shown) await reset(e)
  }
  assert(counted > 0, `${tab}: the census found its buttons`)
  console.log(`  ${tab}: ${counted + tally.skipped} buttons: ${tally.shows} show something, ${tally.grey} grey, ${tally.exempt} exempt, ${tally.silent} show nothing, ${tally.skipped} not clicked`)
  return silent
}

const list = TABS.map(([tab, place]) => [
  tab,
  async (e) => {
    const silent = await tabCensus(e, tab, place)
    assert.deepEqual(silent, [], `${tab}: buttons that show nothing: ${silent.join(', ')}`)
  },
])

await segments('census', list, async (which) => {
  const e = await open({ which })
  await census(e)
  return e
})
