// Drawing objects in a real browser, through the development host: node tests/drawing.mjs [chromium|webkit]
// Inserts a shape from the shared Insert controls, moves, resizes and turns it with the pointer,
// formats it from the shared Shape Format controls, undoes, saves to .docx, reopens, edits it
// again, and checks that a commenter cannot change it.
import assert from 'node:assert/strict'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, { command, params })
const until = async (check, message) => {
  for (let n = 0; n < 100; n += 1) { if (await check().catch(() => false)) return; await page.waitForTimeout(100) }
  assert.fail(message)
}
const shapes = async () => (await run('drawing.describe')).objects
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
}
const view = async () => {
  const surface = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const { pages } = await page.evaluate(async () => window.devHost.connection().ai.inspect().then((i) => i.details ?? i))
  const first = pages[0].area
  // The editor opens as src/pageView.ts places it: the first page's width plus margins, at most
  // 96 CSS pixels per inch (4/3 px per point), centred, its top 20 px below the canvas top.
  const zoom = Math.min(96 / 72, Math.max(0.01, (surface.width - 64) / (first.width + 48)))
  const vx = first.x + first.width / 2 - surface.width / (2 * zoom)
  const vy = first.y - 20 / zoom
  return (pageIndex, x, y) => {
    const a = pages[pageIndex].area
    return { x: surface.x + (a.x + x - vx) * zoom, y: surface.y + (a.y + y - vy) * zoom }
  }
}
const drag = async (to, from, by) => {
  const a = to(0, from[0], from[1])
  const b = to(0, from[0] + by[0], from[1] + by[1])
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 3 })
  await page.mouse.move(b.x, b.y, { steps: 3 })
  await page.mouse.up()
}

try {
  await open()
  const to = await view()
  // Choose an oval in the shared Insert controls, then draw it on the page with a drag.
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('doc-drawing-shapes').click()
  await kit.getByTestId('doc-drawing-shape-ellipse').click()
  await drag(to, [200, 90], [160, 90])
  await until(async () => (await shapes()).length === 1, 'the oval is drawn')
  let [shape] = await shapes()
  assert.equal(shape.geometry.preset, 'ellipse')
  assert.ok(Math.abs(shape.frame.x - 200) < 3 && Math.abs(shape.frame.w - 160) < 3, `the oval is where it was drawn: ${JSON.stringify(shape.frame)}`)
  console.log('PASS draw a shape chosen in the shared controls')

  // Drag it, then its bottom-right handle; each is one step.
  shape = (await shapes())[0]
  let f = shape.frame
  await drag(to, [f.x + f.w / 2, f.y + f.h / 2], [60, 40])
  await until(async () => Math.abs((await shapes())[0].frame.x - (f.x + 60)) < 3 && Math.abs((await shapes())[0].frame.y - (f.y + 40)) < 3, 'dragging moves the shape')
  f = (await shapes())[0].frame
  await drag(to, [f.x + f.w, f.y + f.h], [40, 20])
  await until(async () => Math.abs((await shapes())[0].frame.w - (f.w + 40)) < 3, 'the handle resizes the shape')
  console.log('PASS move and resize with the pointer')

  // A rounded rectangle's yellow adjust handle rounds its corners more, in one step.
  await run('drawing.format', { ids: [(await shapes())[0].id], geometry: { preset: 'roundRect' } })
  await until(async () => (await shapes())[0].geometry.preset === 'roundRect', 'a rounded rectangle')
  f = (await shapes())[0].frame
  const side = Math.min(f.w, f.h)
  const adjust = async () => (await shapes())[0].geometry.adjust?.adj ?? 16667
  await drag(to, [f.x + side * 0.16667, f.y], [20, 0])
  await until(async () => Math.abs((await adjust()) - (16667 + (20 / side) * 100000)) < 1500, 'the adjust handle rounds the corners')
  assert.deepEqual((await shapes())[0].frame, f, 'the frame stays')
  await kit.getByTestId('doc-undo').click()
  await until(async () => Math.abs((await adjust()) - 16667) < 2, 'undo takes the adjustment back')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await adjust()) > 20000, 'redo puts it back')
  console.log('PASS reshape with the adjust handle, with undo')

  // Arrange: a second floating shape (inserted on top, selected), sent to the back from Shape
  // Format, in one step. Describing without ids lists every shape bottom to top once none is
  // selected.
  const first = (await shapes())[0].id
  // (With the first one selected, an insert would take its place.)
  await run('caret.docStart')
  const { id: second } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 60, y: 60, w: 80, h: 50 } })
  const stack = async () => {
    // A click in the text away from the shapes leaves none selected.
    const away = to(0, 90, 280)
    await page.mouse.click(away.x, away.y)
    return (await run('drawing.describe')).objects.map((o) => o.id)
  }
  await until(async () => (await stack()).join() === [first, second].join(), 'the new shape is on top')
  const pick = to(0, 100, 85)
  await page.mouse.click(pick.x, pick.y)
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await kit.getByTestId('doc-drawing-send').click()
  await kit.getByTestId('doc-drawing-send-back').click()
  await until(async () => (await stack()).join() === [second, first].join(), 'Send to Back puts it under the first')
  await kit.getByTestId('doc-undo').click()
  await until(async () => (await stack()).join() === [first, second].join(), 'undo puts it back on top')
  await run('drawing.delete', { ids: [second] })
  await until(async () => (await stack()).join() === first, 'one shape again')
  console.log('PASS arrange from Shape Format, with undo')

  // Group: Shift adds the first shape to the selection of a second; Group makes one object of
  // them, Ungroup splits it again, each one step.
  const all = async () => {
    const away = to(0, 90, 280)
    await page.mouse.click(away.x, away.y)
    return (await run('drawing.describe')).objects
  }
  const { id: other } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 60, y: 60, w: 80, h: 50 } })
  await until(async () => (await all()).length === 2, 'a second shape')
  const centreOf = async (id) => {
    const f = (await run('drawing.describe', { ids: [id] })).objects[0].frame
    return to(0, f.x + f.w / 2, f.y + f.h / 2)
  }
  const p1 = await centreOf(other)
  await page.mouse.click(p1.x, p1.y)
  const p2 = await centreOf(first)
  await page.keyboard.down('Shift')
  await page.mouse.click(p2.x, p2.y)
  await page.keyboard.up('Shift')
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the selection')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('doc-drawing-group').isEnabled({ timeout: 500 }), 'Group is offered for two shapes')
  await kit.getByTestId('doc-drawing-group').click()
  await until(async () => {
    const objects = await all()
    return objects.length === 1 && objects[0].kind === 'group' && [...objects[0].members].sort().join() === [first, other].sort().join()
  }, 'Group makes one object of the two')
  const groupId = (await all())[0].id
  const pg = await centreOf(other)
  await page.mouse.click(pg.x, pg.y)
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the group')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('doc-drawing-ungroup').isEnabled({ timeout: 500 }), 'Ungroup is offered for the group')
  await kit.getByTestId('doc-drawing-ungroup').click()
  await until(async () => (await all()).length === 2, 'Ungroup splits it')
  await kit.getByTestId('doc-undo').click()
  await until(async () => (await all()).map((o) => o.id).join() === groupId, 'undo groups them again')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await all()).length === 2, 'redo splits them')
  await run('drawing.delete', { ids: [other] })
  await until(async () => (await all()).map((o) => o.id).join() === first, 'one shape again')
  console.log('PASS group and ungroup from Shape Format, with undo')

  // Connect: two shapes selected, Connect joins them with a connector glued to both.
  await run('caret.docStart')
  const { id: target } = await run('drawing.insert', { geometry: { preset: 'rect' }, frame: { x: 60, y: 60, w: 80, h: 50 } })
  const k1 = await centreOf(target)
  await page.mouse.click(k1.x, k1.y)
  const k2 = await centreOf(first)
  await page.keyboard.down('Shift')
  await page.mouse.click(k2.x, k2.y)
  await page.keyboard.up('Shift')
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'Shape Format shows for the two')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await until(async () => kit.getByTestId('doc-drawing-connect').isEnabled({ timeout: 500 }), 'Connect is offered for two shapes')
  await kit.getByTestId('doc-drawing-connect').click()
  await until(async () => (await all()).some((o) => o.connection?.start?.id === target && o.connection?.end?.id === first), 'a connector glued to both')
  const connector = (await all()).find((o) => o.connection).id
  await run('drawing.delete', { ids: [connector, target] })
  await until(async () => (await all()).map((o) => o.id).join() === first, 'one shape again')
  console.log('PASS connect two shapes from Shape Format')

  // Select the first again, as the steps below expect.
  const back = (await run('drawing.describe', { ids: [first] })).objects[0].frame
  const centre = to(0, back.x + back.w / 2, back.y + back.h / 2)
  await page.mouse.click(centre.x, centre.y)

  // The shared Shape Format controls.
  await until(async () => kit.getByTestId('ribbon-tab-shapeFormat').isVisible(), 'the Shape Format tab shows for the selected shape')
  await kit.getByTestId('ribbon-tab-shapeFormat').click()
  await kit.getByTestId('doc-drawing-fill').click()
  await kit.getByTestId('doc-drawing-fill-colors').locator('button').nth(3).click()
  await until(async () => (await shapes())[0].fill.color?.rgb != null, 'a fill colour from Shape Format')
  await kit.getByTestId('doc-drawing-fill').click()
  await kit.getByTestId('doc-drawing-gradient-down').click()
  await until(async () => (await shapes())[0].fill.gradient?.stops.length === 2 && (await shapes())[0].fill.gradient.angle === 90, 'a gradient of the fill colour')
  await kit.getByTestId('doc-drawing-effects').click()
  await kit.getByTestId('doc-drawing-shadow-bottomRight').click()
  await until(async () => (await shapes())[0].effects?.shadow?.distance === 3, 'a shadow from Shape Effects')
  await kit.getByTestId('doc-drawing-transform').click()
  await kit.getByTestId('doc-drawing-warp-textArchUp').click()
  await until(async () => (await shapes())[0].text?.warp?.preset === 'textArchUp', 'an art text transform from Shape Format')
  await kit.getByTestId('doc-drawing-outline').click()
  await kit.getByTestId('doc-drawing-weight-3').click()
  await until(async () => (await shapes())[0].stroke.width === 3, 'an outline weight from Shape Format')
  await kit.getByTestId('doc-drawing-change-shape').click()
  await kit.getByRole('menuitem', { name: 'Star', exact: true }).click()
  await until(async () => (await shapes())[0].geometry.preset === 'star5', 'Change Shape')
  await kit.getByTestId('doc-drawing-rotate').click()
  await until(async () => (await shapes())[0].frame.rot === 90, 'Rotate Right 90°')
  // The person's Undo (an agent's undo takes back the agent's own steps).
  await kit.getByTestId('doc-undo').click()
  await page.waitForTimeout(800)
  await until(async () => ((await shapes())[0].frame.rot ?? 0) === 0, 'undo takes the turn back')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await shapes())[0].frame.rot === 90, 'redo puts it back')
  console.log('PASS fill, outline, change shape and rotate from Shape Format, with undo')

  // Text into the shape (its own text box story), by command as an agent sets it.
  const id = (await shapes())[0].id
  await run('drawing.text', { id, text: 'Plan 计划' })
  await until(async () => (await shapes())[0].text?.paragraphs[0].runs[0].text === 'Plan 计划', 'the shape takes text')

  // Save as .docx, reopen, and edit again.
  const before = (await shapes())[0]
  // Saved through the host, as the Save button does; the saved bytes reopen as a new document.
  const version = await page.evaluate(() => window.devHost.files.get('Sample.docx').version)
  await kit.getByTestId('doc-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.docx').version)) > version, 'saved through the host')
  const data = await page.evaluate(() => {
    const bytes = new Uint8Array(window.devHost.files.get('Sample.docx').bytes)
    let text = ''
    for (const b of bytes) text += String.fromCharCode(b)
    return btoa(text)
  })
  await page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
    await window.devHost.open('Shapes.docx', bytes.buffer)
  }, data)
  await until(async () => (await shapes()).length === 1 && (await shapes())[0].geometry.preset === 'star5', 'the shape reopens')
  const reopened = (await shapes())[0]
  assert.equal(reopened.id, before.id, 'the same id')
  assert.ok(Math.abs(reopened.frame.x - before.frame.x) < 1 && reopened.frame.rot === 90, 'the frame survives')
  assert.equal(reopened.text.paragraphs[0].runs[0].text, 'Plan 计划')
  await run('drawing.transform', { ids: [reopened.id], dx: -30 })
  await until(async () => Math.abs((await shapes())[0].frame.x - (reopened.frame.x - 30)) < 1, 'the reopened shape moves again')
  console.log('PASS save as .docx, reopen and edit again')

  // A commenter cannot change it.
  await open('&role=commenter')
  await page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
    await window.devHost.open('Shapes.docx', bytes.buffer)
  }, data)
  await until(async () => (await shapes()).length === 1, 'the commenter sees the shape')
  await assert.rejects(() => run('drawing.transform', { ids: [reopened.id], dx: 10 }), /comment|read-only|not allowed/i)
  assert.deepEqual(errors, [])
  console.log('PASS a commenter cannot edit the shape')
} catch (error) {
  // Outside the repository, which commits no screenshots.
  const shot = join(tmpdir(), `kition-doc-drawing-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
