// Charts in a real browser, through the development host: node tests/chart.mjs [chromium|webkit]
// Inserts a chart from the shared Insert › Chart menu, changes its type and elements from the
// shared Chart Design controls, edits its data in the grid, undoes, saves to .docx, reopens, edits
// it again, and checks that a commenter cannot change it.
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
const charts = async () => (await run('drawing.describe')).objects.filter((o) => o.kind === 'chart')
const chart = async () => (await charts())[0]?.chart
const open = async (query = '') => {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
}
const bytesOf = (name) => page.evaluate((name) => {
  const bytes = new Uint8Array(window.devHost.files.get(name).bytes)
  let text = ''
  for (const b of bytes) text += String.fromCharCode(b)
  return btoa(text)
}, name)
const reopen = (name, data) => page.evaluate(async ({ name, data }) => {
  const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
  await window.devHost.open(name, bytes.buffer)
}, { name, data })

try {
  await open()
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('doc-chart-insert').click()
  await kit.getByTestId('doc-chart-insert-column').click()
  await until(async () => (await chart())?.kind === 'column', 'a column chart is inserted')
  console.log('PASS insert a chart from the shared menu')

  await until(() => kit.getByTestId('ribbon-tab-chartDesign').isVisible(), 'Chart Design shows for the selected chart')
  await kit.getByTestId('ribbon-tab-chartDesign').click()
  await kit.getByTestId('doc-chart-type').click()
  await kit.getByTestId('doc-chart-type-line').click()
  await until(async () => (await chart()).kind === 'line', 'Change Chart Type')
  await kit.getByTestId('doc-chart-legend').click()
  await kit.getByTestId('doc-chart-legend-top').click()
  await until(async () => (await chart()).legend === 'top', 'the legend moves')
  await kit.getByTestId('doc-chart-gridlines').click()
  await until(async () => (await chart()).gridlines === false, 'gridlines off')
  console.log('PASS type and elements from Chart Design')
  // Chart Elements: a trendline and error bars on every series.
  await kit.getByTestId('doc-chart-trendline').click()
  await kit.getByTestId('doc-chart-trendline-linear').click()
  await until(async () => (await chart()).series.every((s) => s.trendline?.kind === 'linear'), 'a linear trendline')
  await kit.getByTestId('doc-chart-error-bars').click()
  await kit.getByTestId('doc-chart-error-bars-stdErr').click()
  await until(async () => (await chart()).series.every((s) => s.errorBars?.kind === 'stdErr'), 'standard error bars')
  console.log('PASS a trendline and error bars from Chart Elements')
  // Format Series: the second series becomes a column on the secondary axis.
  await kit.getByTestId('doc-chart-format-series').click()
  await kit.getByTestId('doc-chart-series-which').selectOption('1')
  await kit.getByTestId('doc-chart-series-kind').selectOption('column')
  await kit.getByTestId('doc-chart-series-secondary').click()
  await kit.getByTestId('doc-chart-series-ok').click()
  await until(async () => (await chart()).series[1].secondary === true && (await chart()).series[1].kind === 'column', 'a column on the secondary axis')
  console.log('PASS a combination on two axes from Format Series')

  // Format: the value axis's scale, where labels sit, and a series' look.
  await kit.getByTestId('doc-chart-format-axis').click()
  await kit.getByTestId('doc-chart-axis-max').fill('20')
  await kit.getByTestId('doc-chart-axis-major').fill('5')
  await kit.getByTestId('doc-chart-axis-ok').click()
  await until(async () => (await chart()).yAxis?.max === 20 && (await chart()).yAxis?.major === 5, 'Format Axis sets the scale')
  await kit.getByTestId('doc-chart-label-position').click()
  await kit.getByTestId('doc-chart-label-position-t').click()
  await until(async () => (await chart()).dataLabels && (await chart()).dataLabelPosition === 't', 'labels above the points')
  await kit.getByTestId('doc-chart-format-series').click()
  await kit.getByTestId('doc-chart-series-width').fill('3')
  await kit.getByTestId('doc-chart-series-marker').selectOption('square')
  await kit.getByTestId('doc-chart-series-smooth').check()
  await kit.getByTestId('doc-chart-series-ok').click()
  await until(async () => {
    const s = (await chart()).series[0]
    return s.lineWidth === 3 && s.marker === 'square' && s.smooth === true
  }, 'Format Series sets the line')
  await kit.getByTestId('doc-undo').click()
  await until(async () => !(await chart()).series[0].smooth, 'undo the series look')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await chart()).series[0].smooth === true, 'redo the series look')
  console.log('PASS axis, label position and series look from Format, with undo')

  await kit.getByTestId('doc-chart-edit-data').click()
  await until(() => kit.getByTestId('doc-chart-data').isVisible(), 'the data grid opens')
  await kit.getByTestId('doc-chart-data-category-1').fill('Spring')
  await kit.getByTestId('doc-chart-data-cell-1-2').fill('9')
  await kit.getByTestId('doc-chart-data-add-series').click()
  await kit.getByTestId('doc-chart-data-cell-2-0').fill('1')
  await kit.getByTestId('doc-chart-data-ok').click()
  await until(async () => {
    const c = await chart()
    return c.categories[1] === 'Spring' && c.series[1].values[2] === 9 && c.series.length === 3 && c.series[2].values[0] === 1
  }, 'the data change')
  // The person's Undo takes the whole data edit back in one step.
  await kit.getByTestId('doc-undo').click()
  await until(async () => (await chart()).series.length === 2, 'undo takes the data back')
  await kit.getByTestId('doc-redo').click()
  await until(async () => (await chart()).series.length === 3, 'redo puts it back')
  console.log('PASS edit data in the grid, with undo')

  // The agent's path is the same command.
  const id = (await charts())[0].id
  await run('drawing.chart', { id, title: 'Revenue 收入' })
  await until(async () => (await chart()).title === 'Revenue 收入', 'an agent sets the title')
  const before = await chart()

  // Saved through the host, as the Save button does; the saved bytes reopen as a new document.
  const version = await page.evaluate(() => window.devHost.files.get('Sample.docx').version)
  await kit.getByTestId('doc-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.docx').version)) > version, 'saved through the host')
  const data = await bytesOf('Sample.docx')
  await reopen('Chart.docx', data)
  await until(async () => (await charts()).length === 1, 'the chart reopens')
  assert.equal((await charts())[0].id, id, 'the same id')
  assert.deepEqual(await chart(), before, 'the chart reads back as it was saved')
  await run('drawing.chart', { id, kind: 'bar', dataLabels: true })
  await until(async () => (await chart()).kind === 'bar' && (await chart()).dataLabels === true, 'the reopened chart changes again')
  console.log('PASS save as .docx, reopen and edit again')

  // A commenter cannot change it.
  await open('&role=commenter')
  await reopen('Chart.docx', data)
  await until(async () => (await charts()).length === 1, 'the commenter sees the chart')
  await assert.rejects(() => run('drawing.chart', { id, title: 'No' }), /comment|read-only|not allowed/i)
  assert.deepEqual(errors, [])
  console.log('PASS a commenter cannot edit the chart')
} catch (error) {
  // Outside the repository, which commits no screenshots.
  const shot = join(tmpdir(), `kition-doc-chart-${which}-failure.png`)
  await page.screenshot({ path: shot })
  console.log(`screenshot: ${shot}`)
  throw error
} finally {
  await browser.close()
  await server.close()
}
