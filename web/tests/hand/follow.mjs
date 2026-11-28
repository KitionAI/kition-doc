// The view follows the caret (plan "Doc usable by hand", stage 0B, section 2 row 10; walkthrough
// A10, D5, A11, C7, Q2; WPS W-D6, W-U6): typing past the bottom of the window, the arrows going on
// to another page, typing after a scroll, Undo of an edit out of view, and Find going to another
// page all bring the caret's line into view at the same zoom; scrolling alone leaves the caret.
// node tests/hand/follow.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Picks the status bar's 100% preset. */
async function hundred(e) {
  await e.kit.getByTestId('kit-zoom').click()
  await e.kit.getByTestId('doc-zoom-menu-100').click()
  await e.until(async () => (await e.zoom()) === 100)
}
/** Setup: `count` short paragraphs from the agent in place of the sample's text. */
async function paragraphsOf(e, count) {
  const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
  for (let index = 0; index < count; index += 1) {
    if (index) commands.push({ command: 'text.newParagraph', params: {} })
    commands.push({ command: 'text.insert', params: { text: `Paragraph ${index + 1} of the follow test.` } })
  }
  await e.agent(commands)
  await e.until(async () => (await e.paragraphs()).length >= count)
}
/** Scrolls with the wheel over the pages (a person's scroll, no key). */
async function scroll(e, deltaY) {
  const box = await e.surface()
  await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  for (let left = deltaY; Math.abs(left) > 0.5; ) {
    const step = Math.sign(left) * Math.min(Math.abs(left), 1000)
    await e.page.mouse.wheel(0, step)
    left -= step
  }
  await e.wait(300)
}
/** The caret is drawn inside the canvas. */
async function drawnInView(e) {
  const caret = await e.caretDrawn()
  const box = await e.surface()
  return caret !== null && caret.y >= box.y - 0.5 && caret.y + caret.height <= box.y + box.height + 0.5
}

const list = [
  ['a10-view-follows-caret', async (e) => {
    await paragraphsOf(e, 60)
    await hundred(e)
    await scroll(e, -20000)
    // The caret goes to paragraph 40, below the window: the view brings its line in, just.
    await e.setup('caret.set', { pos: { story: 'body', path: [39], off: 0 } })
    await e.page.keyboard.press('End')
    await e.until(() => e.caretInView(), 30)
    assert(await e.caretInView(), 'the caret placed below the window is brought into view')
    const box = await e.surface()
    const caret = await e.caretDrawn()
    assert(caret && caret.y > box.y + box.height / 2, `not centred: the line shows near the bottom (${Math.round(caret?.y ?? 0)} in ${Math.round(box.y)}–${Math.round(box.y + box.height)})`)
    // Twelve lines typed from there: every line's caret is in the canvas, the zoom stays.
    for (let line = 0; line < 12; line += 1) {
      await e.page.keyboard.press('Enter')
      await e.page.keyboard.type(`Typed line ${line + 1}`, { delay: 5 })
      await e.until(() => drawnInView(e), 30)
      assert(await drawnInView(e), `the caret of typed line ${line + 1} is in the canvas (${JSON.stringify(await e.caretDrawn())})`)
    }
    assert.equal(await e.zoom(), 100, 'the zoom stays 100%')
  }],
  ['d5-typing-scrolls-back', async (e) => {
    if ((await e.paragraphs()).length < 40) await paragraphsOf(e, 60)
    await hundred(e)
    await e.setup('caret.set', { pos: { story: 'body', path: [5], off: 3 } })
    await e.until(() => e.caretInView(), 30)
    // The wheel takes the view 3000 pixels away: the caret stays where it was, out of view.
    await scroll(e, 3000)
    assert(!(await e.caretInView()), 'scrolling alone leaves the caret out of view')
    const before = (await e.paragraph(5)).paragraph.text
    await e.page.keyboard.type('x')
    await e.until(async () => (await e.paragraph(5)).paragraph.text !== before)
    await e.until(() => e.caretInView(), 30)
    assert(await e.caretInView(), 'one typed character brings the view back to the caret')
    assert.equal(await e.zoom(), 100, 'at the same zoom')
    await e.page.keyboard.press('ControlOrMeta+z')
  }],
  ['c7-down-to-page-3', async (e) => {
    await e.pagesOf(4)
    await hundred(e)
    await scroll(e, -20000)
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 0 } })
    await e.wait(300)
    const pageOf = async () => (await e.details()).page
    for (let index = 0; index < 30 && (await pageOf()) < 2; index += 1) {
      await e.page.keyboard.press('ArrowDown')
      await e.until(() => drawnInView(e), 20)
      assert(await drawnInView(e), `after Down ${index + 1} the caret is in view (${JSON.stringify(await e.caretDrawn())})`)
    }
    assert.equal(await pageOf(), 2, 'the arrows took the caret to page 3')
    assert.equal(await e.zoom(), 100, 'the zoom stays 100% all the way')
  }],
  ['a11-undo-shows-place', async (e) => {
    if ((await e.details()).pages.length < 4) await e.pagesOf(4)
    await hundred(e)
    await scroll(e, -20000)
    await e.setup('caret.set', { pos: { story: 'body', path: [1], off: 0 } })
    await e.wait(200)
    const before = (await e.paragraph(1)).paragraph.text
    await e.page.keyboard.type('Z')
    await e.until(async () => (await e.paragraph(1)).paragraph.text === `Z${before}`)
    // To page 3 with the wheel, then Cmd+Z: the text comes back and the view goes to it.
    await scroll(e, 2400)
    assert(!(await e.caretInView()), 'page 1 is out of view')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraph(1)).paragraph.text === before)
    assert.equal((await e.paragraph(1)).paragraph.text, before, 'Cmd+Z took the typing back')
    await e.until(() => e.caretInView(), 30)
    assert(await e.caretInView(), 'and the view shows the place it was taken back')
  }],
  ['q2-find-scrolls-to-match', async (e) => {
    if ((await e.details()).pages.length < 4) await e.pagesOf(4)
    await hundred(e)
    await scroll(e, -20000)
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 0 } })
    await e.wait(200)
    await e.page.keyboard.press('ControlOrMeta+f')
    await e.until(() => e.kit.getByTestId('doc-find-text').isVisible(), 30)
    await e.page.keyboard.type('of page 4', { delay: 10 })
    await e.until(async () => (await e.selection()).text === 'of page 4')
    assert.equal((await e.details()).page, 3, 'the match is on page 4')
    await e.until(() => e.caretInView(), 30)
    assert(await e.caretInView(), 'the match is in view')
    assert.equal(await e.zoom(), 100, 'at the same zoom')
    await e.page.keyboard.press('Escape')
  }],
]

await segments('follow', list, (which) => open({ which }))
