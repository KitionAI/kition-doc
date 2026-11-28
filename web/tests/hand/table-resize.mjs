// Dragging a table's column border by hand (plan "Doc usable by hand", stage 4, section 2 row 25;
// walkthrough H6; WPS W-N12): over the border the pointer shows left and right arrows; while it
// is dragged a dashed line runs down the page and the table waits; let go, the left column is
// wider by the drag and its right neighbour narrower by as much, the table as wide as before, one
// Undo taking it back; the same at 150%; a double click fits the column to its text.
// node tests/hand/table-resize.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const NAMES = ['A', 'B', 'C']
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
  await e.until(async () => (await e.ask('file.paragraph', { path: [1, 2, 2, 0] }))?.paragraph?.text === 'C3')
}
/** Where the text of cell (0, c) starts (document points; the column's left edge plus its margin). */
async function starts(e) {
  const out = []
  for (let c = 0; c < 3; c += 1) {
    await e.setup('caret.set', { pos: { story: 'body', path: [1, 0, c, 0], off: 0 } })
    await e.until(async () => (await e.selection()).focus?.path?.[2] === c)
    out.push((await e.caretRect()).x)
  }
  await e.setup('caret.docStart')
  return out
}
/** The border between A1 and B1 on the page, at A1's line. */
async function border(e) {
  await e.pathPoint([1, 0, 1, 0], 0)
  const caret = await e.caretRect()
  const point = await e.toScreen(caret.x - 5.4, caret.top + caret.height / 2)
  await e.setup('caret.docStart')
  await e.wait(400)
  return point
}
async function dragBorder(e, from, dx) {
  await e.page.mouse.move(from.x, from.y)
  await e.page.mouse.down()
  for (let step = 1; step <= 8; step += 1) await e.page.mouse.move(from.x + (dx * step) / 8, from.y)
  await e.wait(300)
  const during = { dashed: await e.dashedDrawn(), canUndo: (await e.details()).canUndo, starts: null }
  await e.page.mouse.up()
  await e.wait(400)
  return during
}
const near = (a, b, tolerance = 1.2) => Math.abs(a - b) <= tolerance

async function resizes(e) {
  await table(e)
  const before = await starts(e)
  const at = await border(e)
  const zoom = (await e.view()).zoom
  await e.page.mouse.move(at.x + 30, at.y)
  await e.page.mouse.move(at.x + 2, at.y)
  await e.until(async () => (await e.cursor()) === 'col-resize')
  assert.equal(await e.cursor(), 'col-resize', 'left and right arrows on the border')
  const undoBefore = (await e.details()).canUndo
  const during = await dragBorder(e, at, 40)
  assert(during.dashed, 'a dashed line while it is dragged')
  assert(near(during.dashed.x, at.x + 40, 2), `under the pointer (${during.dashed.x} vs ${at.x + 40})`)
  assert(during.dashed.height > 600 * zoom, 'down the page')
  assert.equal(during.canUndo, undoBefore, 'the table waits for the release')
  await e.until(async () => !near((await starts(e))[1], before[1]))
  const after = await starts(e)
  assert(near(after[1] - before[1], 40 / zoom), `the left column is wider by the drag (${before} -> ${after})`)
  assert(near(after[2], before[2]), 'the right neighbour gave it up: the third column starts where it did')
  await e.page.keyboard.press('ControlOrMeta+z')
  await e.until(async () => near((await starts(e))[1], before[1]))
  assert(near((await starts(e))[1], before[1]), 'one Undo takes it back')
}

const list = [
  ['h6-drag-column-border', resizes],
  ['h6-drag-at-150', async (e) => {
    await e.kit.getByTestId('kit-zoom').click()
    const field = e.kit.getByTestId('doc-zoom-menu-field')
    await field.fill('150')
    await field.press('Enter')
    await e.until(async () => (await e.zoom()) === 150)
    await resizes(e)
  }],
  ['double-click-fits', async (e) => {
    await table(e)
    const before = await starts(e)
    const at = await border(e)
    await e.page.mouse.move(at.x, at.y)
    await e.page.mouse.dblclick(at.x, at.y)
    await e.until(async () => (await starts(e))[1] < before[1] - 20)
    const after = await starts(e)
    assert(after[1] < before[1] - 20, `the first column fits A1..A3 (${before} -> ${after})`)
  }],
]

await segments('table-resize', list, (which) => open({ which }))
