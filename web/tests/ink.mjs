// Ink in a real browser, through the development host: node tests/ink.mjs [chromium|webkit]
// Picks a pen from the shared Draw controls, draws a stroke with the pointer, undoes and redoes
// it, erases it with the eraser's drag, saves to .docx, reopens, and checks that a commenter
// cannot draw.
import './platform.mjs'
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
const inks = async () => (await run('drawing.describe', {})).objects.filter((o) => o.kind === 'ink')
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
  await kit.getByTestId('ribbon-tab-draw').click()
  await kit.getByTestId('doc-ink-pen').click()
  await until(async () => (await run('draw.state')).tool === 'pen', 'the pen is in hand')
  await until(() => kit.getByTestId('doc-ink-pen').evaluate((b) => b.getAttribute('aria-pressed') === 'true'), 'the Pen button shows it')
  await kit.getByTestId('doc-ink-weight').click()
  await kit.getByTestId('doc-ink-weight-3').click()
  await until(async () => (await run('draw.state')).width === 3, 'the pen is 3 pt')
  await drag(to, [150, 200], [120, 30])
  await until(async () => (await inks()).length === 1, 'one stroke on release')
  const [stroke] = (await inks())[0].ink
  assert.equal(stroke.width, 3)
  assert.ok(Math.abs(stroke.points[0][0] - 150) < 3 && Math.abs(stroke.points.at(-1)[0] - 270) < 3, JSON.stringify(stroke.points))
  await kit.getByTestId('doc-undo').click()
  await until(async () => (await inks()).length === 0, 'undo takes the stroke back')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await inks()).length === 1, 'redo puts it back')
  console.log('PASS draw a stroke with the shared pen, with undo')

  await kit.getByTestId('doc-ink-highlighter').click()
  await until(async () => (await run('draw.state')).tool === 'highlighter', 'the highlighter is in hand')
  await drag(to, [150, 300], [100, 0])
  await until(async () => (await inks()).some((o) => o.ink[0].tool === 'highlighter'), 'a highlighter stroke')
  await kit.getByTestId('doc-ink-eraser').click()
  await until(async () => (await run('draw.state')).tool === 'eraser', 'the eraser is in hand')
  await drag(to, [210, 180], [0, 50])
  await until(async () => (await inks()).length === 1 && (await inks())[0].ink[0].tool === 'highlighter', 'the eraser takes the pen stroke it crosses')
  console.log('PASS the highlighter, and the eraser drag')

  await kit.getByTestId('doc-ink-select').click()
  const version = await page.evaluate(() => window.devHost.files.get('Sample.docx').version)
  await kit.getByTestId('doc-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.docx').version)) > version, 'saved through the host')
  const data = await page.evaluate(() => {
    const bytes = new Uint8Array(window.devHost.files.get('Sample.docx').bytes)
    let text = ''
    for (const b of bytes) text += String.fromCharCode(b)
    return btoa(text)
  })
  const reopen = (name) => page.evaluate(async ({ b64, name }) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
    await window.devHost.open(name, bytes.buffer)
  }, { b64: data, name })
  await reopen('Ink.docx')
  await until(async () => (await inks()).length === 1 && (await inks())[0].ink[0].tool === 'highlighter', 'the stroke reopens')
  console.log('PASS save as .docx and reopen')

  await open('&role=commenter')
  await reopen('Ink.docx')
  await until(async () => (await inks()).length === 1, 'the commenter sees the ink')
  await assert.rejects(() => run('drawing.ink', { strokes: [{ points: [[10, 10], [20, 20]] }] }), /comment|read-only|not allowed/i)
  assert.deepEqual(errors, [])
  console.log('PASS a commenter cannot draw')
} catch (error) {
  // Outside the repository, which commits no screenshots.
  const shot = join(tmpdir(), `kition-doc-ink-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
