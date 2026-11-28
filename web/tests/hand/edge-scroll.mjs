// Scrolling at the window's edge during a drag (plan "Doc usable by hand", stage 1, section 2 row
// 4; walkthrough B8; WPS W-B5): a drag held 20 pixels below the canvas scrolls the view with the
// pointer still and the selection follows into text that was out of view; held inside the canvas
// nothing scrolls; a drag goes across pages that way; the zoom stays.
// node tests/hand/edge-scroll.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Setup: `count` short paragraphs from the agent in place of the sample's text. */
async function paragraphsOf(e, count) {
  const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
  for (let index = 0; index < count; index += 1) {
    if (index) commands.push({ command: 'text.newParagraph', params: {} })
    commands.push({ command: 'text.insert', params: { text: `Paragraph ${index + 1} of the edge test.` } })
  }
  await e.agent(commands)
  await e.until(async () => (await e.paragraphs()).length >= count)
}
async function hundred(e) {
  await e.kit.getByTestId('kit-zoom').click()
  await e.kit.getByTestId('doc-zoom-menu-100').click()
  await e.until(async () => (await e.zoom()) === 100)
}
/** The selection's ends in order, paragraph indexes. */
async function ends(e) {
  const { anchor, focus } = await e.selection()
  return [anchor, focus].sort((a, b) => a.path[0] - b.path[0] || a.off - b.off).map((pos) => pos.path[0])
}
const list = [
  ['b8-edge-auto-scroll', async (e) => {
    await paragraphsOf(e, 80)
    await hundred(e)
    const start = await e.textPoint(1, 3)
    const box = await e.surface()
    const before = await e.view()
    await e.wait(600)
    await e.page.mouse.move(start.x, start.y)
    await e.page.mouse.down()
    // Down to 20 pixels below the canvas, then held still for two seconds.
    for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(start.x, start.y + ((box.y + box.height + 20 - start.y) * step) / 8)
    await e.wait(2000)
    const during = await e.view()
    assert(during.y > before.y + 300, `the view scrolled with the pointer held still (${before.y} → ${during.y})`)
    await e.page.mouse.up()
    await e.wait(300)
    const [from, to] = await ends(e)
    assert.equal(from, 1, 'the selection starts where the press was')
    assert(to > 40, `the selection reaches text that was out of view (paragraph ${to + 1})`)
    assert.equal(await e.zoom(), 100, 'the zoom stays')
  }],
  ['inside-no-scroll', async (e) => {
    if ((await e.paragraphs()).length < 40) await paragraphsOf(e, 80)
    await hundred(e)
    await e.setup('caret.set', { pos: { story: 'body', path: [0], off: 0 } })
    await e.page.mouse.move(800, 500)
    await e.page.mouse.wheel(0, -20000)
    await e.wait(400)
    const start = await e.textPoint(2, 3)
    const box = await e.surface()
    const before = await e.view()
    await e.wait(600)
    await e.page.mouse.move(start.x, start.y)
    await e.page.mouse.down()
    for (let step = 1; step <= 5; step += 1) await e.page.mouse.move(start.x, start.y + ((box.y + box.height - 6 - start.y) * step) / 5)
    await e.wait(1000)
    const during = await e.view()
    await e.page.mouse.up()
    assert(Math.abs(during.y - before.y) < 1, `held inside the canvas, nothing scrolls (${before.y} → ${during.y})`)
  }],
  ['across-pages', async (e) => {
    await e.pagesOf(3)
    await hundred(e)
    await e.page.mouse.move(800, 500)
    await e.page.mouse.wheel(0, -20000)
    await e.wait(400)
    const start = await e.textPoint(1, 2)
    const box = await e.surface()
    await e.wait(600)
    await e.page.mouse.move(start.x, start.y)
    await e.page.mouse.down()
    for (let step = 1; step <= 6; step += 1) await e.page.mouse.move(start.x, start.y + ((box.y + box.height + 40 - start.y) * step) / 6)
    // Held until the third page's lines come into view.
    const blocks = (await e.paragraphs()).length
    await e.until(async () => {
      const shown = await e.view()
      const pages = (await e.details()).pages
      return pages.length >= 3 && shown.y + box.height / shown.zoom > pages[2].area.y + 120
    }, 80, 100)
    await e.page.mouse.up()
    await e.wait(300)
    const [from, to] = await ends(e)
    assert.equal(from, 1)
    const page3 = (await e.paragraphs()).findIndex((text) => text.includes('page 3'))
    assert(to >= page3 && to < blocks, `the selection went on to the third page (paragraph ${to}, the page starts at ${page3})`)
  }],
]

await segments('edge-scroll', list, (which) => open({ which }))
