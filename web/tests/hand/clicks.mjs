// Clicks and the selection bar (plan "Doc usable by hand", stage 1, section 2 rows 2, 5, 6;
// walkthrough B4, B10, B12, B13; WPS W-A8, W-C1, W-C4, W-C5, W-C7): three clicks select the
// paragraph with its mark, a double click held and dragged grows by words, the left margin selects
// a line, a paragraph or everything and drags by lines, Cmd+click selects the paragraph's text, and
// three clicks still work zoomed in and scrolled to another page.
// node tests/hand/clicks.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const WORDS = ['Alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel']
/** Setup: `count` paragraphs of words from the agent in place of the sample's text. */
async function paragraphsOf(e, count) {
  const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
  for (let index = 0; index < count; index += 1) {
    if (index) commands.push({ command: 'text.newParagraph', params: {} })
    commands.push({ command: 'text.insert', params: { text: `${WORDS.join(' ')} ${index + 1}.` } })
  }
  await e.agent(commands)
  await e.until(async () => (await e.paragraphs()).length >= count)
}
/** Where the selection runs: `path/off` of each end, in order, and its text. */
async function range(e) {
  const { anchor, focus, text } = await e.selection()
  const [a, b] = [anchor, focus].sort((x, y) => x.path[0] - y.path[0] || x.off - y.off)
  return { from: `${a.path.join('.')}/${a.off}`, to: `${b.path.join('.')}/${b.off}`, text }
}
/** Presses `count` times at `at` as a person does (one multi-click), apart from any press before. */
async function clicks(e, at, count, options = {}) {
  await e.wait(600)
  await e.page.mouse.move(at.x, at.y)
  for (let index = 1; index <= count; index += 1) {
    await e.page.mouse.down({ clickCount: index, ...options })
    await e.page.mouse.up({ clickCount: index, ...options })
  }
  await e.wait(250)
}
/** A point in the selection bar beside the line through `at`: 20 points into the left margin. */
async function barBeside(e, at) {
  const info = await e.details()
  const page = info.pages[0].area
  const left = await e.toScreen(page.x + 20, 0)
  return { x: left.x, y: at.y }
}
/** Byte length of paragraph `index`'s text. */
const lengthOf = async (e, index) => Buffer.from((await e.paragraph(index)).paragraph.text, 'utf8').length

const list = [
  ['b4-triple-click', async (e) => {
    await paragraphsOf(e, 5)
    const at = await e.textPoint(1, 9)
    await clicks(e, at, 3)
    await e.until(async () => (await range(e)).to === '2/0')
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: '1/0', to: '2/0' }, 'three clicks: the paragraph with its mark')
    await clicks(e, await e.textPoint(3, 2), 2)
    await e.until(async () => (await range(e)).text === 'Alpha ')
    assert.equal((await range(e)).text, 'Alpha ', 'two clicks: the word with its space')
  }],
  ['b12-double-click-drag', async (e) => {
    if ((await e.paragraphs()).length < 5) await paragraphsOf(e, 5)
    const bravo = await e.textPoint(2, 8)
    const delta = await e.textPoint(2, 22)
    await e.wait(600)
    await e.page.mouse.move(bravo.x, bravo.y)
    await e.page.mouse.down({ clickCount: 1 })
    await e.page.mouse.up({ clickCount: 1 })
    await e.page.mouse.down({ clickCount: 2 })
    for (let step = 1; step <= 6; step += 1) await e.page.mouse.move(bravo.x + ((delta.x - bravo.x) * step) / 6, bravo.y)
    await e.page.mouse.up({ clickCount: 2 })
    await e.until(async () => (await range(e)).text === 'bravo charlie delta ')
    assert.equal((await range(e)).text, 'bravo charlie delta ', 'the words from the first to the one under the pointer, whole')
  }],
  ['b10-selection-bar', async (e) => {
    if ((await e.paragraphs()).length < 5) await paragraphsOf(e, 5)
    const line1 = await barBeside(e, await e.textPoint(1, 0))
    const line3 = await barBeside(e, await e.textPoint(3, 0))
    await clicks(e, line1, 1)
    await e.until(async () => (await range(e)).from === '1/0' && (await range(e)).to === '2/0')
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: '1/0', to: '2/0' }, 'one click in the margin: the line')
    await clicks(e, line1, 2)
    await e.until(async () => (await range(e)).to === '2/0')
    assert.equal((await range(e)).from, '1/0', 'two clicks: the paragraph')
    await clicks(e, line1, 3)
    const last = (await e.paragraphs()).length - 1
    const end = await lengthOf(e, last)
    await e.until(async () => (await range(e)).to === `${last}/${end}`)
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: '0/0', to: `${last}/${end}` }, 'three clicks: the whole document')
    // Down the margin from line 1 to line 3: three lines.
    await e.wait(600)
    await e.page.mouse.move(line1.x, line1.y)
    await e.page.mouse.down()
    for (let step = 1; step <= 5; step += 1) await e.page.mouse.move(line1.x, line1.y + ((line3.y - line1.y) * step) / 5)
    await e.page.mouse.up()
    await e.until(async () => (await range(e)).to === '4/0')
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: '1/0', to: '4/0' }, 'a drag down the margin selects whole lines')
    assert.match(await e.cursor(), /url\(/, 'the selection bar shows its own arrow')
  }],
  ['b13-cmd-click', async (e) => {
    if ((await e.paragraphs()).length < 5) await paragraphsOf(e, 5)
    const at = await e.textPoint(2, 12)
    await e.wait(600)
    await e.page.keyboard.down('ControlOrMeta')
    await e.page.mouse.click(at.x, at.y)
    await e.page.keyboard.up('ControlOrMeta')
    const length = await lengthOf(e, 2)
    await e.until(async () => (await range(e)).to === `2/${length}`)
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: '2/0', to: `2/${length}` }, "Cmd+click: the paragraph's text, without its mark")
  }],
  ['b4-zoomed-and-scrolled', async (e) => {
    await e.pagesOf(3)
    await e.kit.getByTestId('kit-zoom').click()
    const field = e.kit.getByTestId('doc-zoom-menu-field')
    await field.fill('150')
    await field.press('Enter')
    await e.until(async () => (await e.zoom()) === 150)
    // The second page's second line (paragraph 5: "Line 1 of page 2.").
    const index = (await e.paragraphs()).findIndex((text) => text === 'Line 1 of page 2.')
    assert(index > 0, 'the second page has its lines')
    const at = await e.textPoint(index, 5)
    assert(at, 'the line is in view at 150%')
    await clicks(e, at, 3)
    await e.until(async () => (await range(e)).from === `${index}/0`)
    assert.deepEqual(await range(e).then(({ from, to }) => ({ from, to })), { from: `${index}/0`, to: `${index + 1}/0` }, 'three clicks at 150% on page 2: that paragraph')
    assert.equal(await e.zoom(), 150, 'the zoom stays')
  }],
]

await segments('clicks', list, (which) => open({ which }))
