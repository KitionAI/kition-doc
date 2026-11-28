// Pictures by hand (plan "Doc usable by hand", stage 5A; walkthrough P2-P9; WPS W-O1-W-O7, W-W4,
// W-F4, W-O6): a click selects a picture with eight round handles ringed in a colour that shows on
// the page, no tint over it; the pointer is a double arrow on a handle and move on the picture; a
// corner keeps the ratio (Shift lets it go); a handle takes a press just outside it; an inline
// picture dragged into other text stays inline there; Layout Options beside it sets the wrapping;
// Size sets its width with the height following; its tab comes and goes with it; Backspace after
// it selects it first.
// node tests/hand/picture.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** A 1×1 PNG, sized 160 × 100 pt once inserted. */
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
const at = (path, off = 0) => ({ story: 'body', path, off })

/** Setup (the agent's, and the picture the editor's): three paragraphs, a 160 × 100 pt picture inline after "Before " in the second, the caret at the start. */
async function document(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: 'The first paragraph of text.' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'Before ' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'The third paragraph, where it lands.' } },
  ])
  await e.until(async () => (await e.paragraphs()).length === 3)
  await e.setup('caret.set', { pos: at([1], 7) })
  await e.setup('image.insert', { data: PNG })
  await e.setup('select.range', { anchor: at([1], 7), focus: at([1], 10) })
  await e.setup('image.size', { width: 160, height: 100, lockAspect: false })
  await e.setup('caret.set', { pos: at([0]) })
  await e.until(async () => (await e.details()).picture == null)
}

/** The picture of paragraph `index` (its size and wrapping), or null. */
async function objectOf(e, index) {
  const o = (await e.paragraph(index))?.paragraph?.objects?.[0]
  return o ? { w: o.w, h: o.h, wrap: o.float?.wrap ?? 'inline' } : null
}

/** The selected picture's box on the screen: its corners and middle. */
async function box(e) {
  let picture = null
  await e.until(async () => (picture = (await e.details()).picture) != null, 40)
  assert(picture, 'a picture is selected')
  const a = await e.toScreen(picture.x, picture.y)
  const b = await e.toScreen(picture.x + picture.width, picture.y + picture.height)
  return { x0: a.x, y0: a.y, x1: b.x, y1: b.y, cx: (a.x + b.x) / 2, cy: (a.y + b.y) / 2, wrap: picture.wrap }
}

/** Selects the picture with a click on its middle (the agent's caret first, off it). */
async function selectByHand(e) {
  await e.setup('caret.set', { pos: at([1], 10) })
  await e.setup('select.range', { anchor: at([1], 7), focus: at([1], 10) })
  const b = await box(e)
  await e.setup('caret.set', { pos: at([0]) })
  await e.until(async () => (await e.details()).picture == null)
  await e.page.mouse.click(b.cx, b.cy)
  return box(e)
}

/** A drag by the person's mouse, in steps. */
async function drag(e, from, to, steps = 8) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  for (let index = 1; index <= steps; index += 1) {
    await e.page.mouse.move(from.x + ((to.x - from.x) * index) / steps, from.y + ((to.y - from.y) * index) / steps)
    await e.wait(20)
  }
  await e.page.mouse.up()
}

const activeTab = (e) => e.frame().evaluate(() => document.querySelector('[data-testid^=ribbon-tab-][aria-selected=true]')?.dataset.testid?.replace('ribbon-tab-', '') ?? null)
const white = (style) => /^(#fff(fff)?|white|rgba?\(255, 255, 255(, 1)?\))$/i.test(String(style ?? '').trim())

const list = [
  ['p2-visible-handles', async (e) => {
    await document(e)
    const b = await selectByHand(e)
    let rings = []
    await e.until(async () => {
      rings = (await e.circlesDrawn()).filter((c) => c.ring && c.r >= 3)
      return rings.length >= 8
    }, 40)
    assert(rings.length >= 8, `eight round handles at least: ${JSON.stringify(rings)}`)
    assert(rings.every((c) => !white(c.ring)), `their rings are not the paper's colour: ${rings.map((c) => c.ring).join(', ')}`)
    assert(rings.every((c) => c.fill == null || white(c.fill)), 'paper inside')
    const corner = rings.find((c) => Math.hypot(c.x - b.x1, c.y - b.y1) < 2)
    assert(corner, `a handle on the bottom-right corner: ${JSON.stringify(rings.map((c) => [Math.round(c.x), Math.round(c.y)]))}`)
    const tint = (await e.fillsDrawn()).filter((f) => f.x0 < b.cx && f.x1 > b.cx && f.y0 < b.cy && f.y1 > b.cy)
    assert.equal(tint.length, 0, 'no selection tint over the picture')
    // The pointer: a double arrow on a corner, on an edge; move on the picture.
    await e.page.mouse.move(b.x1, b.y1)
    assert(await e.until(async () => (await e.cursor()) === 'nwse-resize', 30), `on the corner: ${await e.cursor()}`)
    await e.page.mouse.move(b.x1, b.cy)
    assert(await e.until(async () => (await e.cursor()) === 'ew-resize', 30), `on the right edge: ${await e.cursor()}`)
    await e.page.mouse.move(b.cx, b.cy)
    assert(await e.until(async () => (await e.cursor()) === 'move', 30), `on the picture: ${await e.cursor()}`)
  }],
  ['p4-corner-keeps-ratio', async (e) => {
    await document(e)
    const b = await selectByHand(e)
    await drag(e, { x: b.x1, y: b.y1 }, { x: b.x1 + 60, y: b.y1 + 40 })
    let after = null
    await e.until(async () => (after = await objectOf(e, 1))?.w > 161, 40)
    assert(after && after.w > 161, `it grew: ${JSON.stringify(after)}`)
    assert(Math.abs(after.w / after.h - 1.6) < 0.01, `the ratio stays 1.6: ${after.w} × ${after.h}`)
    assert.equal(after.wrap, 'inline', 'still inline')
    // Shift lets the ratio go.
    const c = await box(e)
    await e.page.keyboard.down('Shift')
    await drag(e, { x: c.x1, y: c.y1 }, { x: c.x1 + 60, y: c.y1 + 6 })
    await e.page.keyboard.up('Shift')
    let free = null
    await e.until(async () => (free = await objectOf(e, 1))?.w > after.w + 1, 40)
    assert(free && Math.abs(free.w / free.h - 1.6) > 0.05, `Shift: free, ${free?.w} × ${free?.h}`)
  }],
  ['p5-handle-reach', async (e) => {
    await document(e)
    const b = await selectByHand(e)
    // 6 screen pixels outside the corner, along its diagonal.
    const d = 6 / Math.SQRT2
    await drag(e, { x: b.x1 + d, y: b.y1 + d }, { x: b.x1 + d + 40, y: b.y1 + d + 25 })
    let after = null
    await e.until(async () => (after = await objectOf(e, 1))?.w > 161, 40)
    assert(after && after.w > 161, `the handle was taken: ${JSON.stringify(after)}`)
    assert((await e.details()).picture, 'the picture is still selected')
  }],
  ['p6-inline-stays-inline', async (e) => {
    await document(e)
    const target = await e.textPoint(2, 21)
    assert(target, 'the third paragraph is in view')
    const b = await selectByHand(e)
    await e.page.mouse.move(b.cx, b.cy)
    await e.page.mouse.down()
    let bar = null
    for (let index = 1; index <= 12; index += 1) {
      await e.page.mouse.move(b.cx + ((target.x + 1 - b.cx) * index) / 12, b.cy + ((target.y - b.cy) * index) / 12)
      await e.wait(25)
    }
    await e.until(async () => (bar = await e.dashedDrawn()) != null, 30)
    assert(bar, 'a dashed bar shows where it lands')
    assert(Math.abs(bar.x - target.x) < 3, `the bar at the place: ${bar.x} vs ${target.x}`)
    await e.page.mouse.up()
    let moved = null
    await e.until(async () => (moved = await objectOf(e, 2)) != null, 40)
    assert(moved, 'the picture is in the third paragraph')
    assert.equal(moved.wrap, 'inline', 'still inline')
    assert.equal(await objectOf(e, 1), null, 'it left the second')
    assert.equal((await e.paragraph(2)).paragraph.text, 'The third paragraph, ￼where it lands.')
    // One undo step back.
    await e.page.keyboard.press('ControlOrMeta+z')
    assert(await e.until(async () => (await objectOf(e, 1)) != null), 'undo puts it back')
  }],
  ['p8-wrap-options', async (e) => {
    await document(e)
    await selectByHand(e)
    const button = e.kit.getByTestId('doc-layout-options')
    assert(await e.until(() => button.isVisible(), 30), 'Layout Options beside the picture')
    await button.click()
    const panel = e.kit.getByTestId('doc-layout-options-panel')
    assert(await e.until(() => panel.isVisible(), 30), 'its panel opens')
    for (const wrap of ['inline', 'square', 'tight', 'through', 'topAndBottom', 'behindText', 'inFrontOfText']) assert(await e.kit.getByTestId(`doc-layout-wrap-${wrap}`).isVisible(), `${wrap} is in it`)
    // Escape closes it and leaves the picture selected.
    await e.page.keyboard.press('Escape')
    assert(await e.until(async () => !(await panel.isVisible()), 30), 'Escape closes the panel')
    assert((await e.details()).picture, 'the picture is still selected')
    await button.click()
    assert(await e.until(() => panel.isVisible(), 30))
    await e.kit.getByTestId('doc-layout-wrap-square').click()
    let after = null
    await e.until(async () => (after = await objectOf(e, 1))?.wrap === 'square', 40)
    assert.equal(after?.wrap, 'square', 'Square: the text wraps around it')
    // Picture Format's Wrap Text opens the same panel.
    await e.kit.getByTestId('ribbon-tab-pictureFormat').click()
    await e.kit.getByTestId('doc-cmd-arrange.wrap').first().click()
    assert(await e.until(() => panel.isVisible(), 30), 'Wrap Text opens the panel')
    await e.kit.getByTestId('doc-layout-wrap-inline').click()
    await e.until(async () => (after = await objectOf(e, 1))?.wrap === 'inline', 40)
    assert.equal(after?.wrap, 'inline')
  }],
  ['p8-size-dialog', async (e) => {
    await document(e)
    await selectByHand(e)
    await e.kit.getByTestId('ribbon-tab-pictureFormat').click()
    await e.kit.getByTestId('doc-cmd-image.size').first().click()
    const width = e.kit.getByTestId('doc-picture-size-width')
    assert(await e.until(() => width.isVisible(), 30), 'Size opens its dialog')
    await width.fill('200')
    assert.equal(await e.kit.getByTestId('doc-picture-size-height').inputValue(), '125', 'the height follows')
    assert(await e.kit.getByTestId('doc-picture-size-rotation').isVisible(), 'and the angle is there')
    await e.kit.getByTestId('doc-picture-size-ok').click()
    let after = null
    await e.until(async () => (after = await objectOf(e, 1))?.w === 200, 40)
    assert.deepEqual([after?.w, after?.h], [200, 125])
    // Its angle, typed: the picture turns.
    await e.kit.getByTestId('doc-cmd-image.size').first().click()
    const rotation = e.kit.getByTestId('doc-picture-size-rotation')
    assert(await e.until(() => rotation.isVisible(), 30))
    await rotation.fill('30')
    await e.kit.getByTestId('doc-picture-size-ok').click()
    const angle = async () => (await e.paragraph(1))?.paragraph?.objects?.[0]?.float?.rot ?? 0
    assert(await e.until(async () => (await angle()) === 30, 40), `turned: ${await angle()}`)
    assert.deepEqual([(await objectOf(e, 1)).w, (await objectOf(e, 1)).h], [200, 125], 'the size as it was')
  }],
  ['p9-tab-follows', async (e) => {
    await document(e)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    const b = await selectByHand(e)
    assert(await e.until(async () => (await activeTab(e)) === 'pictureFormat', 40), `selecting it shows its tab: ${await activeTab(e)}`)
    // A click in the text lets it go: the tab it came from.
    const text = await e.textPoint(0, 4)
    await e.setup('select.range', { anchor: at([1], 7), focus: at([1], 10) })
    await e.page.mouse.click(text.x, text.y)
    assert(await e.until(async () => (await activeTab(e)) === 'insert', 40), `back to Insert: ${await activeTab(e)}`)
    // Selected again on another tab, a double click shows its tab.
    await e.page.mouse.click(b.cx, b.cy)
    assert(await e.until(async () => (await activeTab(e)) === 'pictureFormat', 40))
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.page.mouse.dblclick(b.cx, b.cy)
    assert(await e.until(async () => (await activeTab(e)) === 'pictureFormat', 40), `a double click: ${await activeTab(e)}`)
    assert((await e.details()).picture, 'still selected')
  }],
  ['f4-backspace-selects-first', async (e) => {
    await document(e)
    const b = await selectByHand(e)
    // The caret just after the picture, by a click on its line's end.
    await e.page.mouse.click(b.x1 + 30, b.y1 - 4)
    await e.until(async () => (await e.details()).picture == null, 30)
    await e.setup('caret.set', { pos: at([1], 10) })
    await e.page.keyboard.press('Backspace')
    assert(await e.until(async () => (await e.details()).picture != null, 30), 'the first Backspace selects it')
    assert(await objectOf(e, 1), 'and keeps it')
    await e.page.keyboard.press('Backspace')
    assert(await e.until(async () => (await objectOf(e, 1)) == null, 30), 'the second deletes it')
    // Typing on a selected picture puts the letters before it.
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await objectOf(e, 1)) != null, 30)
    await selectByHand(e)
    await e.page.keyboard.type('ab')
    assert(await e.until(async () => (await e.paragraph(1)).paragraph.text === 'Before ab￼', 30), `typed before it: ${(await e.paragraph(1)).paragraph.text}`)
  }],
]

await segments('picture', list, async (which) => open({ which }))
