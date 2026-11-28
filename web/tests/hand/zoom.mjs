// The page view's zoom by hand (plan "Doc usable by hand", stage 0A, section 2 rows 34, 35;
// walkthrough A5, A6, A8, A9; WPS W-U1–W-U4): Ctrl or Cmd with a wheel notch goes 10 points, the
// status bar's − and + go to the next multiple of 10, the percentage opens presets and a field;
// Cmd+0, Cmd+= and Cmd+- are formatting keys, not zoom keys; the caret going on to another page
// keeps the zoom.
// node tests/hand/zoom.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Picks a preset (`200`, `100`, `75`) under the status bar's percentage. */
async function preset(e, value) {
  await e.kit.getByTestId('kit-zoom').click()
  await e.kit.getByTestId(`doc-zoom-menu-${value}`).click()
  await e.until(async () => (await e.zoom()) === value)
}
/** One wheel notch over the middle of the pages with `modifier` held. */
async function notch(e, modifier, deltaY) {
  const box = await e.surface()
  await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await e.page.keyboard.down(modifier)
  await e.page.mouse.wheel(0, deltaY)
  await e.page.keyboard.up(modifier)
  await e.wait(250)
}
const caretIn = async (e, index, off = 0) => {
  await e.setup('caret.set', { pos: { story: 'body', path: [index], off } })
  await e.wait(200)
}

const list = [
  ['a8-wheel-zoom-step', async (e) => {
    await preset(e, 100)
    await notch(e, 'Control', -100)
    assert.equal(await e.zoom(), 110, 'Ctrl+wheel one notch in: 100% → 110%')
    await notch(e, 'Control', -100)
    assert.equal(await e.zoom(), 120, 'and again: 120%')
    await notch(e, 'Control', 100)
    assert.equal(await e.zoom(), 110, 'one notch out: 110%')
    await notch(e, 'Meta', -100)
    assert.equal(await e.zoom(), 120, 'Cmd+wheel goes the same 10 points')
    // About the pointer: the point under it stays (within a pixel).
    const box = await e.surface()
    const at = { x: box.x + box.width * 0.3, y: box.y + box.height * 0.4 }
    const before = await e.view()
    await e.page.mouse.move(at.x, at.y)
    await e.page.keyboard.down('Control')
    await e.page.mouse.wheel(0, -100)
    await e.page.keyboard.up('Control')
    await e.until(async () => (await e.zoom()) === 130)
    const after = await e.view()
    const docBefore = { x: before.x + (at.x - box.x) / before.zoom, y: before.y + (at.y - box.y) / before.zoom }
    const docAfter = { x: after.x + (at.x - box.x) / after.zoom, y: after.y + (at.y - box.y) / after.zoom }
    // Up and down; across, the page narrower than the window stays centred.
    assert(Math.abs(docBefore.y - docAfter.y) * after.zoom < 2, `the point under the pointer stays (${JSON.stringify(docBefore)} → ${JSON.stringify(docAfter)})`)
  }],
  ['a6-status-bar-steps', async (e) => {
    await e.kit.getByTestId('kit-zoom').click()
    const field = e.kit.getByTestId('doc-zoom-menu-field')
    await field.fill('113')
    await field.press('Enter')
    await e.until(async () => (await e.zoom()) === 113)
    assert.equal(await e.zoom(), 113, 'a percentage typed in')
    await e.kit.getByTestId('doc-zoom-in').click()
    await e.until(async () => (await e.zoom()) === 120)
    assert.equal(await e.zoom(), 120, '+ goes to the next multiple of 10')
    await e.kit.getByTestId('doc-zoom-in').click()
    await e.until(async () => (await e.zoom()) === 130)
    assert.equal(await e.zoom(), 130, 'and on by 10')
    await e.kit.getByTestId('doc-zoom-out').click()
    await e.until(async () => (await e.zoom()) === 120)
    assert.equal(await e.zoom(), 120, '− goes back 10')
    await e.kit.getByTestId('kit-zoom').click()
    await field.fill('85')
    await field.press('Enter')
    await e.until(async () => (await e.zoom()) === 85)
    assert.equal(await e.zoom(), 85, 'typed 85')
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the text has the keyboard after a zoom choice')
    await e.kit.getByTestId('kit-zoom').click()
    await field.fill('5')
    await field.press('Enter')
    await e.until(async () => (await e.zoom()) === 10)
    assert.equal(await e.zoom(), 10, 'the zoom stops at 10%')
    await preset(e, 200)
    assert.equal(await e.zoom(), 200, 'the 200% preset')
    // One Page and Page Width (the percentage's choices and the status bar's Fit) show the current page.
    await e.kit.getByTestId('doc-zoom-fit').click()
    await e.wait(300)
    const { pages, page } = await e.details()
    const view = await e.view()
    const box = await e.surface()
    const shown = pages[page].area
    assert(shown.height * view.zoom <= box.height + 1 && shown.height * view.zoom > box.height * 0.8, `Fit shows the current page whole, not every page (${Math.round(view.zoom * 100)}%)`)
  }],
  ['a5-cmd-0', async (e) => {
    await preset(e, 100)
    await caretIn(e, 2)
    const spaceBefore = async () => (await e.paragraph(2)).resolved?.spaceBefore ?? (await e.paragraph(2)).paragraph?.props?.spaceBefore ?? 0
    const before = await spaceBefore()
    await e.page.keyboard.press('ControlOrMeta+0')
    await e.until(async () => (await spaceBefore()) !== before)
    assert.equal(await e.zoom(), 100, 'Cmd+0 keeps the zoom')
    assert.notEqual(await spaceBefore(), before, 'Cmd+0 adds (or removes) the space before the paragraph')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await spaceBefore()) === before)

    // Cmd+= is Subscript, Cmd+Shift+= Superscript, Cmd+- nothing.
    await e.page.keyboard.press('End')
    await e.page.keyboard.down('Shift')
    await e.page.keyboard.press('ArrowLeft')
    await e.page.keyboard.press('ArrowLeft')
    await e.page.keyboard.up('Shift')
    await e.until(async () => (await e.selection()).text?.length === 2)
    await e.page.keyboard.press('ControlOrMeta+Equal')
    await e.until(async () => (await e.read('format.state'))?.subscript === true)
    assert.equal((await e.read('format.state'))?.subscript, true, 'Cmd+= makes the selection subscript')
    await e.page.keyboard.press('ControlOrMeta+Shift+Equal')
    await e.until(async () => (await e.read('format.state'))?.superscript === true)
    assert.equal((await e.read('format.state'))?.superscript, true, 'Cmd+Shift+= makes it superscript')
    const text = (await e.paragraphs()).join('\n')
    await e.page.keyboard.press('ControlOrMeta+Minus')
    await e.wait(300)
    assert.equal(await e.zoom(), 100, 'none of the three zooms')
    assert.equal((await e.paragraphs()).join('\n'), text, 'Cmd+- inserts nothing')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.page.keyboard.press('ControlOrMeta+z')
  }],
  ['a9-page-change-keeps-zoom', async (e) => {
    await e.pagesOf(4)
    await preset(e, 100)
    await caretIn(e, 0)
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    // Setup: the view back at the first page's top.
    for (let index = 0; index < 6; index += 1) await e.page.mouse.wheel(0, -2000)
    await e.wait(300)
    const pageOf = async () => (await e.details()).page
    let seen = 0
    for (let index = 0; index < 30 && (await pageOf()) < 2; index += 1) {
      await e.page.keyboard.press('ArrowDown')
      await e.wait(60)
      seen = Math.max(seen, await pageOf())
    }
    await e.until(async () => (await pageOf()) === 2)
    assert.equal(await pageOf(), 2, 'the arrows took the caret to page 3')
    await e.wait(300)
    assert.equal(await e.zoom(), 100, 'the zoom stays 100% all the way')
    const view = await e.view()
    const page = (await e.details()).pages[2].area
    assert(page.y < view.y + box.height / view.zoom && page.y + page.height > view.y, `page 3 is in view (view ${Math.round(view.y)}, page ${Math.round(page.y)})`)
    const caret = await e.caretDrawn()
    assert(caret && caret.y >= box.y && caret.y + caret.height <= box.y + box.height, `the caret is in view (${JSON.stringify(caret)})`)
  }],
  ['c10-select-all-keeps-view', async (e) => {
    if ((await e.details()).pages.length < 3) await e.pagesOf(4)
    await preset(e, 100)
    await caretIn(e, 0)
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    for (let index = 0; index < 6; index += 1) await e.page.mouse.wheel(0, -2000)
    await e.wait(300)
    const before = await e.view()
    await e.page.keyboard.press('ControlOrMeta+a')
    await e.until(async () => ((await e.selection()).text?.length ?? 0) > 50)
    await e.wait(400)
    const after = await e.view()
    assert(Math.abs(after.y - before.y) < 1 && after.zoom === before.zoom, `Cmd+A leaves the view where it was (${Math.round(before.y)} → ${Math.round(after.y)})`)
  }],
  ['agent-push-keeps-view', async (e) => {
    // The person's caret pushed to another page by an agent's edit before it: the view stays.
    if ((await e.details()).pages.length < 3) await e.pagesOf(4)
    await preset(e, 100)
    await caretIn(e, 1)
    const box = await e.surface()
    await e.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    for (let index = 0; index < 6; index += 1) await e.page.mouse.wheel(0, -2000)
    await e.wait(300)
    const before = await e.view()
    const page = (await e.details()).page
    await e.agent([{ command: 'caret.docStart', params: {} }, { command: 'page.insertBreak', params: {} }, { command: 'page.insertBreak', params: {} }])
    await e.until(async () => (await e.details()).page === page + 2)
    assert.equal((await e.details()).page, page + 2, "the agent's breaks pushed the person's caret two pages on")
    await e.wait(400)
    const after = await e.view()
    assert(Math.abs(after.y - before.y) < 1, `the person's view stays (${Math.round(before.y)} → ${Math.round(after.y)})`)
  }],
]

await segments('zoom', list, (which) => open({ which }))
