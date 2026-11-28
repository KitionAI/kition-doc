// Dragging selected text (plan "Doc usable by hand", stage 1, section 2 row 16; walkthrough B9;
// WPS W-K1-W-K4): a press on the selection dragged to the end of the paragraph moves the word
// there, a dashed bar showing where it lands while it is carried, the word still selected after,
// one Undo taking it back; with Cmd held a copy goes and the original stays; a press on the
// selection let go where it went down puts the caret there.
// node tests/hand/drag-text.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const TEXT = 'Alpha bravo charlie delta.'
async function oneParagraph(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: TEXT } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'Second paragraph.' } },
  ])
  await e.until(async () => (await e.paragraphs())[0] === TEXT)
}
/** Selects "bravo " with a double click, as a person does; answers a point on it. */
async function selectBravo(e) {
  const on = await e.textPoint(0, 8)
  await e.wait(600)
  await e.page.mouse.move(on.x, on.y)
  await e.page.mouse.down({ clickCount: 1 })
  await e.page.mouse.up({ clickCount: 1 })
  await e.page.mouse.down({ clickCount: 2 })
  await e.page.mouse.up({ clickCount: 2 })
  await e.until(async () => (await e.selection()).text === 'bravo ')
  assert.equal((await e.selection()).text, 'bravo ')
  await e.wait(600)
  return on
}
/** Presses at `from` and drags to `to` in steps, as a hand does; `hold` before letting go. */
async function drag(e, from, to, hold = async () => undefined) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(from.x + ((to.x - from.x) * step) / 8, from.y + ((to.y - from.y) * step) / 8)
  await e.wait(300)
  await hold()
  await e.page.mouse.up()
  await e.wait(300)
}

const list = [
  ['b9-drag-text', async (e) => {
    await oneParagraph(e)
    const end = await e.textPoint(0, TEXT.length)
    const on = await selectBravo(e)
    let bar = null
    await drag(e, on, end, async () => {
      await e.until(async () => (bar = await e.dashedDrawn()) !== null, 30)
    })
    assert(bar, 'a dashed bar shows where the text lands')
    assert(Math.abs(bar.x - end.x) < 3, `at the place under the pointer (${bar.x} vs ${end.x})`)
    await e.until(async () => (await e.paragraphs())[0] !== TEXT)
    assert.equal((await e.paragraphs())[0], 'Alpha charlie delta.bravo ', 'the word moved to the end')
    assert.equal((await e.selection()).text, 'bravo ', 'and is still selected')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraphs())[0] === TEXT)
    assert.equal((await e.paragraphs())[0], TEXT, 'one Undo takes it back')
  }],
  ['cmd-drag-copies', async (e) => {
    await oneParagraph(e)
    const second = await e.textPoint(1, 0)
    const on = await selectBravo(e)
    await e.page.mouse.move(on.x, on.y)
    await e.page.mouse.down()
    await e.page.keyboard.down('ControlOrMeta')
    for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(on.x + ((second.x - on.x) * step) / 8, on.y + ((second.y - on.y) * step) / 8)
    await e.wait(300)
    assert.equal(await e.cursor(), 'copy', 'the pointer says a copy goes')
    await e.page.mouse.up()
    await e.page.keyboard.up('ControlOrMeta')
    await e.until(async () => (await e.paragraphs())[1] !== 'Second paragraph.')
    const [first, other] = await e.paragraphs()
    assert.equal(first, TEXT, 'the original stays')
    assert.equal(other, 'bravo Second paragraph.', 'a copy goes where it was let go')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraphs())[1] === 'Second paragraph.')
  }],
  ['press-on-selection-places-caret', async (e) => {
    await oneParagraph(e)
    const on = await selectBravo(e)
    await e.page.mouse.click(on.x, on.y)
    await e.until(async () => (await e.selection()).text === '')
    const { focus } = await e.selection()
    assert.deepEqual([focus.path[0], focus.off], [0, 8], 'a press let go where it went down: the caret there')
    assert.equal((await e.paragraphs())[0], TEXT)
  }],
]

await segments('drag-text', list, (which) => open({ which }))
