// Selecting in a table by hand (plan "Doc usable by hand", stage 4, section 2 rows 11, 21-24;
// walkthrough H7, H8, D10, D13; WPS W-N6-W-N11, W-F2): a drag from one cell into another selects
// the block of cells between them, shaded whole; a letter typed then goes into the first cell
// alone, Delete empties the block; the margin left of a row selects the row; the handle by the
// table's corner selects the table, Backspace then empties it and Delete removes it; Backspace at
// the start of the paragraph after the table changes nothing; Cmd+A in a cell selects the table,
// again everything.
// node tests/hand/table-select.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const NAMES = ['A', 'B', 'C']
/** Setup: "Before", a 3x3 table whose cells say A1..C3, "After" (from the agent). */
async function table(e) {
  const at = (path) => ({ story: 'body', path, off: 0 })
  const commands = [
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: 'Before' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'After' } },
    { command: 'select.range', params: { anchor: at([1]), focus: at([1]) } },
    { command: 'table.insert', params: { rows: 3, cols: 3 } },
  ]
  for (let r = 0; r < 3; r += 1) {
    for (let c = 0; c < 3; c += 1) {
      commands.push({ command: 'select.range', params: { anchor: at([1, r, c, 0]), focus: at([1, r, c, 0]) } })
      commands.push({ command: 'text.insert', params: { text: `${NAMES[c]}${r + 1}`, raw: true } })
    }
  }
  await e.agent(commands)
  await e.until(async () => (await cell(e, 2, 2)) === 'C3')
  await e.setup('caret.docStart')
  await e.wait(300)
}
const cell = async (e, r, c) => (await e.ask('file.paragraph', { path: [1, r, c, 0] }))?.paragraph?.text ?? null
const cells = async (e) => (await e.details()).cells ?? null
/** A point in cell (r, c), `off` bytes into its text. */
const inCell = (e, r, c, off = 1) => e.pathPoint([1, r, c, 0], off)
async function drag(e, from, to) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(from.x + ((to.x - from.x) * step) / 8, from.y + ((to.y - from.y) * step) / 8)
  await e.page.mouse.up()
  await e.wait(300)
}
/** The corner of the table on the page: left of A1's text by its cell margin, at its line's top. */
async function corner(e) {
  const caret = (await e.pathPoint([1, 0, 0, 0], 0)) && (await e.caretRect())
  return { x: caret.x - 5.4, y: caret.top }
}

const selectedTab = (e) => e.frame().evaluate(() => document.querySelector('[role=tab][aria-selected=true]')?.dataset.tab ?? null)

const list = [
  ['h7-cell-block', async (e) => {
    await table(e)
    const a1 = await inCell(e, 0, 0)
    const b2 = await inCell(e, 1, 1)
    await e.setup('caret.docStart')
    await e.wait(600)
    await drag(e, a1, b2)
    await e.until(async () => (await cells(e))?.count === 4)
    const block = await cells(e)
    assert.deepEqual([block.rows, block.cols, block.count], [[0, 1], [0, 1], 4], `2x2 cells, C1 not among them: ${JSON.stringify(block)}`)
    const shown = await e.view()
    const wide = (await e.fillsDrawn()).filter((f) => f.x1 - f.x0 > 100 * shown.zoom && f.y1 - f.y0 > 8)
    assert(wide.length >= 4, `each cell shaded whole, not its text (${JSON.stringify(wide)})`)
    await e.page.keyboard.type('Z')
    await e.until(async () => (await cell(e, 0, 0)) === 'Z')
    assert.deepEqual([await cell(e, 0, 0), await cell(e, 0, 1), await cell(e, 1, 0), await cell(e, 1, 1), await cell(e, 0, 2)], ['Z', 'B1', 'A2', 'B2', 'C1'], 'only the first cell changed')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await cell(e, 0, 0)) === 'A1')
    await e.wait(600)
    await drag(e, await inCell(e, 0, 0), await inCell(e, 1, 1))
    await e.until(async () => (await cells(e))?.count === 4)
    await e.page.keyboard.press('Delete')
    await e.until(async () => (await cell(e, 1, 1)) === '')
    assert.deepEqual([await cell(e, 0, 0), await cell(e, 0, 1), await cell(e, 1, 0), await cell(e, 1, 1)], ['', '', '', ''], 'Delete empties the four')
    assert.deepEqual([await cell(e, 0, 2), await cell(e, 2, 0)], ['C1', 'A3'])
  }],
  ['h8-row-from-the-margin', async (e) => {
    await table(e)
    const a2 = await inCell(e, 1, 0)
    const left = { x: a2.x - 40, y: a2.y }
    await e.wait(600)
    await e.page.mouse.move(left.x, left.y)
    await e.page.mouse.click(left.x, left.y)
    await e.until(async () => (await cells(e))?.whole === 'rows')
    const block = await cells(e)
    assert.deepEqual([block.rows, block.whole], [[1, 1], 'rows'], `the second row: ${JSON.stringify(block)}`)
    await e.page.keyboard.press('Backspace')
    await e.until(async () => (await cell(e, 1, 2)) === '')
    assert.deepEqual([await cell(e, 1, 0), await cell(e, 1, 2), await cell(e, 0, 0)], ['', '', 'A1'], 'Backspace empties the row')
    assert.equal((await e.details()).blocks?.[1]?.rows, 3, 'the row stays (WPS W-N11)')
  }],
  ['d13-handle-backspace-then-delete', async (e) => {
    await table(e)
    const at = await corner(e)
    const handle = await e.toScreen(at.x - 6, at.y - 6)
    await e.wait(600)
    await e.page.mouse.move(handle.x + 40, handle.y + 30)
    await e.page.mouse.move(handle.x, handle.y)
    await e.until(async () => (await e.cursor()) === 'move')
    assert.equal(await e.cursor(), 'move', 'the handle shows by the corner')
    await e.page.mouse.click(handle.x, handle.y)
    await e.until(async () => (await cells(e))?.whole === 'table')
    await e.page.keyboard.press('Backspace')
    await e.until(async () => (await cell(e, 2, 2)) === '')
    assert.equal((await e.details()).blocks?.[1]?.type, 'table', 'Backspace keeps the table, empty')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await cell(e, 2, 2)) === 'C3')
    await e.page.mouse.click(handle.x, handle.y)
    await e.until(async () => (await cells(e))?.whole === 'table')
    await e.page.keyboard.press('Delete')
    await e.until(async () => (await e.details()).blocks?.[1]?.type !== 'table')
    assert.deepEqual((await e.details()).blocks.map((b) => b.type), ['paragraph', 'paragraph'], 'Delete removes it')
  }],
  ['d10-backspace-after-table', async (e) => {
    await table(e)
    const after = await e.textPoint(2, 0)
    await e.wait(600)
    await e.page.mouse.click(after.x + 1, after.y)
    await e.until(async () => (await e.selection()).focus?.path?.[0] === 2)
    for (let press = 0; press < 2; press += 1) {
      await e.page.keyboard.press('Backspace')
      await e.wait(300)
      const { focus } = await e.selection()
      assert.deepEqual([focus.path, focus.off], [[2], 0], 'the caret stays at the start of "After"')
    }
    assert.equal(await cell(e, 2, 2), 'C3', 'the last cell keeps its text')
    assert.equal((await e.paragraph(2))?.paragraph?.text, 'After')
  }],
  ['cmd-a-table-then-all', async (e) => {
    await table(e)
    const b2 = await inCell(e, 1, 1)
    await e.wait(600)
    await e.page.mouse.click(b2.x, b2.y)
    await e.until(async () => (await e.selection()).focus?.path?.length === 4)
    await e.page.keyboard.press('ControlOrMeta+a')
    await e.until(async () => (await cells(e))?.whole === 'table')
    assert.equal((await e.details()).all, false, 'the first Cmd+A: the table')
    await e.page.keyboard.press('ControlOrMeta+a')
    await e.until(async () => (await e.details()).all === true)
    assert.equal((await e.details()).all, true, 'the second: everything')
  }],
  ['table-tabs-show-without-switching', async (e) => {
    await table(e)
    await e.kit.getByTestId('ribbon-tab-home').click()
    const b2 = await inCell(e, 1, 1)
    await e.setup('caret.docStart')
    await e.wait(600)
    await e.page.mouse.click(b2.x, b2.y)
    await e.until(async () => (await e.kit.getByTestId('ribbon-tab-tableDesign').count()) === 1)
    assert.equal(await e.kit.getByTestId('ribbon-tab-tableLayout').count(), 1, 'the table tabs show')
    await e.wait(300)
    assert.equal(await selectedTab(e), 'home', 'but the ribbon stays on Home (WPS W-W3)')
    // Insert › Table: the new table's Table Design tab comes up.
    const after = await e.textPoint(2, 5)
    await e.page.mouse.click(after.x, after.y)
    await e.until(async () => (await e.kit.getByTestId('ribbon-tab-tableDesign').count()) === 0)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.kit.getByTestId('doc-table').click()
    await e.kit.getByTestId('kui-table-2x3').click()
    await e.until(async () => (await selectedTab(e)) === 'tableDesign')
    assert.equal(await selectedTab(e), 'tableDesign', 'a table made from Insert brings its tab up')
  }],
]

await segments('table-select', list, (which) => open({ which }))
