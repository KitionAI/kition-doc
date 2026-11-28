// The right-click menu by hand (plan "Doc usable by hand", stage 2A, section 2 row 31; walkthrough
// R1; WPS W-S1-W-S4): a right-click on the selection keeps it and Copy puts it on the system
// clipboard; elsewhere the caret goes where it was pressed; on a misspelled word the first
// suggestion replaces it in one undo step; in a table's cell Insert › Rows Below adds a row; on a
// picture Delete takes it out; Shift+F10 opens the menu at the caret and Escape gives the keyboard
// back to the text.
// node tests/hand/context-menu.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** A 1×1 PNG, a picture for the menu. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
const FIRST = 'Alpha bravo charlie delta.'
const SECOND = 'This line has a wrnog word.'

/** Setup: two paragraphs of the person's own text. */
async function twoParagraphs(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: FIRST } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: SECOND } },
  ])
  await e.until(async () => (await e.paragraphs()).join('\n') === `${FIRST}\n${SECOND}`)
}
const menu = (e) => e.kit.getByTestId('doc-context-menu')
const menuOpen = (e) => e.until(() => menu(e).isVisible(), 40)
const menuClosed = (e) => e.until(async () => !(await menu(e).isVisible()), 40)
const row = (e, name) => menu(e).getByRole('menuitem', { name, exact: true }).first()
/** A right-click as a hand makes it: the right button pressed and let go. */
async function rightClick(e, at) {
  await e.page.mouse.move(at.x, at.y)
  await e.page.mouse.down({ button: 'right' })
  await e.page.mouse.up({ button: 'right' })
}
/** The person's caret on the canvas, on the screen (null when none is reported). */
async function caretOnScreen(e) {
  const caret = await e.caretRect()
  return caret ? e.toScreen(caret.x, caret.top + caret.height / 2) : null
}

const list = [
  ['r1-selection-kept-copy', async (e) => {
    await twoParagraphs(e)
    const word = await e.textPoint(0, 8)
    // "bravo" selected with the person's keys.
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 6 } })
    await e.wait(200)
    await e.page.keyboard.down('Shift')
    for (let index = 0; index < 5; index += 1) await e.page.keyboard.press('ArrowRight')
    await e.page.keyboard.up('Shift')
    await e.until(async () => (await e.selection()).text === 'bravo')
    await rightClick(e, word)
    assert(await menuOpen(e), 'a right-click opens the menu')
    assert.equal((await e.selection()).text, 'bravo', 'the selection stays (WPS W-S2)')
    for (const id of ['doc-menu-cut', 'doc-menu-copy', 'doc-menu-paste']) assert(await e.kit.getByTestId(id).isVisible(), `${id} is in the menu`)
    assert(await e.kit.getByTestId('doc-menu-copy').isEnabled(), 'Copy is there for the selection')
    await e.kit.getByTestId('doc-menu-copy').click()
    assert(await menuClosed(e), 'a choice closes the menu')
    if (e.which === 'chromium') {
      await e.until(async () => (await e.clipboardText()) === 'bravo')
      assert.equal(await e.clipboardText(), 'bravo', 'Copy puts the selected text on the system clipboard')
    }
    assert.equal((await e.selection()).text, 'bravo', 'still selected after Copy')
  }],
  ['r1-outside-moves-caret', async (e) => {
    await twoParagraphs(e)
    const away = await e.textPoint(1, 5)
    await e.setup('select.range', { anchor: { story: 'body', path: [0], off: 0 }, focus: { story: 'body', path: [0], off: 5 } })
    await e.until(async () => (await e.selection()).text === 'Alpha')
    await rightClick(e, away)
    assert(await menuOpen(e))
    await e.until(async () => (await e.selection()).text === '')
    assert.equal((await e.selection()).text, '', 'a right-click off the selection drops it')
    const caret = await caretOnScreen(e)
    assert(caret && Math.abs(caret.x - away.x) < 3 && Math.abs(caret.y - away.y) < 6, `the caret went where it was pressed (${JSON.stringify(caret)} vs ${JSON.stringify(away)})`)
    assert(!(await e.kit.getByTestId('doc-menu-copy').isEnabled()), 'Copy is grey with nothing selected')
    await e.page.keyboard.press('Escape')
    assert(await menuClosed(e), 'Escape closes the menu')
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the keyboard is the text\'s again')
  }],
  ['r1-spelling-suggestion', async (e) => {
    await twoParagraphs(e)
    const word = await e.textPoint(1, 18)
    await e.setup('caret.docStart')
    await e.wait(200)
    await rightClick(e, word)
    assert(await menuOpen(e))
    const first = e.kit.getByTestId('doc-menu-suggestion-0')
    assert(await first.isVisible(), 'the misspelled word has suggestions in the menu')
    const suggestion = (await first.textContent()).trim()
    assert(suggestion && suggestion !== 'wrnog', `a suggestion (${suggestion})`)
    await first.click()
    await e.until(async () => (await e.paragraphs())[1] !== SECOND)
    assert.equal((await e.paragraphs())[1], SECOND.replace('wrnog', suggestion), 'the word is replaced')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraphs())[1] === SECOND)
    assert.equal((await e.paragraphs())[1], SECOND, 'one Undo takes it back')
  }],
  ['r1-table-insert-row', async (e) => {
    await twoParagraphs(e)
    await e.agent([
      { command: 'caret.set', params: { pos: { story: 'body', path: [1], off: Buffer.byteLength(SECOND) } } },
      { command: 'text.newParagraph', params: {} },
      { command: 'table.insert', params: { rows: 2, cols: 2 } },
    ])
    const tableAt = async () => ((await e.ask('file.inspect', { text: true })).blocks ?? []).findIndex((block) => block.type === 'table')
    await e.until(async () => (await tableAt()) >= 0)
    const index = await tableAt()
    const rows = async () => ((await e.ask('file.inspect', { text: true })).blocks ?? [])[index]?.rows
    assert.equal(await rows(), 2)
    await e.setup('caret.set', { pos: { story: 'body', path: [index, 0, 0, 0], off: 0 } })
    await e.wait(300)
    const cell = await caretOnScreen(e)
    await e.setup('caret.docStart')
    await e.wait(200)
    await rightClick(e, { x: cell.x + 4, y: cell.y })
    assert(await menuOpen(e))
    const insert = e.kit.getByTestId('doc-menu-table-insert')
    assert(await insert.isVisible(), 'a cell\'s menu has Insert')
    await insert.hover()
    const below = e.kit.getByRole('menuitem', { name: 'Insert Rows Below', exact: true })
    await e.until(() => below.isVisible(), 30)
    await below.click()
    await e.until(async () => (await rows()) === 3)
    assert.equal(await rows(), 3, 'Insert Rows Below adds a row')
  }],
  ['r1-picture-delete', async (e) => {
    await twoParagraphs(e)
    // The agent is given no pictures by path; the editor inserts one as data (setup).
    await e.setup('caret.set', { pos: { story: 'body', path: [1], off: Buffer.byteLength(SECOND) } })
    await e.setup('text.newParagraph')
    await e.setup('image.insert', { data: PNG, width: 60 })
    await e.until(async () => (await e.paragraphs()).length === 3)
    // Where the picture is: the caret just after it, on its line.
    await e.setup('caret.set', { pos: { story: 'body', path: [2], off: 3 } })
    await e.wait(300)
    const caret = await e.caretRect()
    const on = await e.toScreen(caret.x - 20, caret.top + caret.height / 2)
    await e.setup('caret.docStart')
    await e.wait(200)
    await rightClick(e, on)
    assert(await menuOpen(e))
    const remove = e.kit.getByTestId('doc-menu-delete-object')
    assert(await remove.isVisible(), 'a picture\'s menu has Delete')
    assert(await e.kit.getByTestId('doc-menu-wrap').isVisible(), 'and Wrap Text')
    await remove.click()
    await e.until(async () => !((await e.paragraph(2))?.paragraph?.text ?? '').includes('￼'))
    const left = (await e.paragraph(2))?.paragraph?.text ?? ''
    assert(!left.includes('￼'), 'the picture is gone')
  }],
  ['r1-shift-f10', async (e) => {
    await twoParagraphs(e)
    await e.textPoint(0, 3)
    await e.wait(300)
    await e.page.keyboard.press('Shift+F10')
    assert(await menuOpen(e), 'Shift+F10 opens the menu')
    const caret = await caretOnScreen(e)
    const box = await menu(e).boundingBox()
    assert(box && caret && Math.abs(box.x - caret.x) < 40 && box.y >= caret.y - 4, `at the caret (${JSON.stringify(box)} vs ${JSON.stringify(caret)})`)
    await e.page.keyboard.press('Escape')
    assert(await menuClosed(e))
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the keyboard is the text\'s again')
    await e.page.keyboard.type('X')
    await e.until(async () => (await e.paragraphs())[0] === `AlpX${FIRST.slice(3)}`)
    assert.equal((await e.paragraphs())[0], `AlpX${FIRST.slice(3)}`, 'typing goes on at the caret')
  }],
]

await segments('context menu', list, (which) => open({ which }))
