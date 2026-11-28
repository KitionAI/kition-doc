// The pointer's shape and the format painter (plan "Doc usable by hand", stage 1, section 2 rows
// 2, 16, 19; walkthrough B11, G14; WPS W-M1, W-M2): the I-beam over text, the selection bar's
// arrow in the left margin, the arrow beside the page and over the selected text, `copy` while the
// format painter is held. The painter's button is pressed while it is held; one click paints one
// selection and lets go, a double click paints until Escape.
// node tests/hand/cursors.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const TEXT = 'Alpha bravo charlie delta echo.'
async function setupText(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: TEXT } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'Second paragraph here.' } },
  ])
  await e.until(async () => (await e.paragraphs())[0] === TEXT)
}
/** The cursor the canvas shows with the pointer resting at `at`. */
async function cursorAt(e, at) {
  await e.page.mouse.move(at.x, at.y)
  await e.wait(250)
  await e.page.mouse.move(at.x + 1, at.y)
  await e.wait(250)
  return e.cursor()
}
/** Double-clicks the word at `at`, as a person does. */
async function doubleClick(e, at) {
  await e.wait(600)
  await e.page.mouse.move(at.x, at.y)
  await e.page.mouse.down({ clickCount: 1 })
  await e.page.mouse.up({ clickCount: 1 })
  await e.page.mouse.down({ clickCount: 2 })
  await e.page.mouse.up({ clickCount: 2 })
  await e.wait(400)
}
/** Whether character `ch` (ASCII text: a byte offset too) of paragraph `index` is bold. */
const bold = async (e, index, ch) => {
  const { paragraph } = await e.paragraph(index)
  let at = 0
  for (const run of paragraph.runs ?? []) {
    const start = at
    const end = start + (run.len ?? 0)
    at = end
    if (ch >= start && ch < end) return (run.props ?? run).bold === true
  }
  return false
}
const painterButton = (e) => e.kit.getByTestId('doc-cmd-edit.formatPainter')

const list = [
  ['b11-pointer-shapes', async (e) => {
    await setupText(e)
    const word = await e.textPoint(0, 8)
    const info = await e.details()
    const page = info.pages[0].area
    const margin = await e.toScreen(page.x + 20, 0)
    const beside = await e.toScreen(page.x + page.width + 20, 0)
    assert.equal(await cursorAt(e, word), 'text', 'the I-beam over text')
    assert.match(await cursorAt(e, { x: margin.x, y: word.y }), /url\(/, "the selection bar's arrow in the left margin")
    assert.equal(await cursorAt(e, { x: beside.x, y: word.y }), 'default', 'the arrow beside the page')
    await doubleClick(e, word)
    assert.equal((await e.selection()).text, 'bravo ')
    assert.equal(await cursorAt(e, word), 'default', 'the arrow over the selected text (it can be dragged)')
  }],
  ['g14-format-painter', async (e) => {
    await setupText(e)
    const alpha = await e.textPoint(0, 2)
    const charlie = await e.textPoint(0, 14)
    const delta = await e.textPoint(0, 22)
    const echo = await e.textPoint(0, 27)
    // "Alpha" bold, the caret in it: the painter picks bold up.
    await e.setup('select.range', { anchor: { story: 'body', path: [0], off: 0 }, focus: { story: 'body', path: [0], off: 5 } })
    await e.setup('format.bold', {})
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 2 } })
    void alpha
    await painterButton(e).click()
    await e.until(async () => (await painterButton(e).getAttribute('aria-pressed')) === 'true', 30)
    assert.equal(await painterButton(e).getAttribute('aria-pressed'), 'true', 'the button is pressed while the painter is held')
    assert.equal(await cursorAt(e, charlie), 'copy', 'the pointer shows the painter')
    await doubleClick(e, charlie)
    await e.until(async () => bold(e, 0, 14), 30)
    assert(await bold(e, 0, 14), 'the selection got the formatting')
    await e.until(async () => (await painterButton(e).getAttribute('aria-pressed')) !== 'true', 30)
    assert.notEqual(await painterButton(e).getAttribute('aria-pressed'), 'true', 'one click paints once and lets go')
    await doubleClick(e, delta)
    assert(!(await bold(e, 0, 22)), 'the next selection is not painted')
    // A double click on the button: it paints until Escape.
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 2 } })
    await painterButton(e).dblclick()
    await e.until(async () => (await painterButton(e).getAttribute('aria-pressed')) === 'true', 30)
    await doubleClick(e, delta)
    await e.until(async () => bold(e, 0, 22), 30)
    assert(await bold(e, 0, 22), 'the first selection painted')
    assert.equal(await painterButton(e).getAttribute('aria-pressed'), 'true', 'still held')
    await doubleClick(e, echo)
    await e.until(async () => bold(e, 0, 27), 30)
    assert(await bold(e, 0, 27), 'and the second')
    await e.page.keyboard.press('Escape')
    await e.until(async () => (await painterButton(e).getAttribute('aria-pressed')) !== 'true', 30)
    assert.notEqual(await painterButton(e).getAttribute('aria-pressed'), 'true', 'Escape puts it down')
  }],
]

// Classic: the Format Painter button shows on the Home tab (Single line keeps it in its group's popover).
await segments('cursors', list, (which) => open({ which, query: 'modes=classic&locale=en-US' }))
