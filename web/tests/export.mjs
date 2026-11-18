// File › Export in a real browser, through the development host: node tests/export.mjs [chromium|webkit]
// The engine draws the file and the host writes it as a new file (`host/write` mode `export`): the
// editor says it exported only when the host answers, names the file the host wrote, and shows the
// host's error when it refuses. The agent's export command answers the same bytes.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await (await browser.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 150) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
// The first bytes of a workspace file, as text, or null while it is not there.
const head = (name) => page.evaluate((file) => {
  const found = window.devHost.files.get(file)
  return found ? String.fromCharCode(...new Uint8Array(found.bytes).slice(0, 8)) : null
}, name)
const names = () => page.evaluate(() => [...window.devHost.files.keys()])
// Whether a workspace file's bytes hold `text` (read as Latin-1).
const holds = (name, text) => page.evaluate(([file, wanted]) => {
  const found = window.devHost.files.get(file)
  return found ? new TextDecoder('latin1').decode(found.bytes).includes(wanted) : false
}, [name, text])

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  const toasts = (level) => kit.locator(`.kui-toast[data-level=${level}]`).allTextContents().then((all) => all.join(' | '))
  await page.waitForTimeout(1000)
  const pages = Number((await kit.getByTestId('doc-status').textContent()).match(/of (\d+)/)?.[1] ?? 1)

  await kit.getByTestId('ribbon-tab-file').click()
  step('the File tab offers Export PDF and Export Page as PNG', (await kit.getByTestId('doc-export-pdf').count()) === 1 && (await kit.getByTestId('doc-export-png').count()) === 1)
  step('no path-writing export command on the File tab', (await kit.getByTestId('doc-cmd-file.exportPdf').count()) === 0 && (await kit.getByTestId('doc-cmd-file.exportPng').count()) === 0)

  await kit.getByTestId('doc-export-pdf').click()
  await until(async () => (await head('Sample.pdf')) != null || (await toasts('error')) !== '')
  step('Export PDF writes Sample.pdf through the host', (await head('Sample.pdf'))?.startsWith('%PDF-'), `${await head('Sample.pdf')} ${await toasts('error')} files ${await names()}`)
  step('the PDF carries the document\'s text in embedded fonts', (await holds('Sample.pdf', '/FontFile')) && (await holds('Sample.pdf', '/Type /Page') || await holds('Sample.pdf', '/Type/Page')))
  await until(async () => (await toasts('success')).includes('Sample.pdf'))
  step('says it exported, naming the file the host wrote', (await toasts('success')).includes('Sample.pdf'), await toasts('success'))

  const png = pages > 1 ? 'Sample - 1.png' : 'Sample.png'
  await kit.getByTestId('doc-export-png').click()
  await until(async () => (await head(png)) != null || (await toasts('error')) !== '')
  step('Export Page as PNG writes the current page through the host', (await head(png))?.startsWith('\x89PNG'), `${png}: ${await head(png)} files ${await names()}`)

  // A second PDF: the host never replaces a file, and the editor names the one it chose.
  await kit.getByTestId('doc-export-pdf').click()
  await until(async () => (await head('Sample 2.pdf')) != null)
  await until(async () => (await toasts('success')).includes('Sample 2.pdf'))
  step('a second export is the new file the host names', (await head('Sample 2.pdf'))?.startsWith('%PDF-') && (await toasts('success')).includes('Sample 2.pdf'), await toasts('success'))

  // The host refuses the write: the editor shows its error and claims nothing.
  await page.evaluate(() => {
    const files = window.devHost.files
    files.set = () => {
      throw new Error('the workspace disk is full')
    }
  })
  const before = (await names()).length
  await kit.getByTestId('doc-export-pdf').click()
  await until(async () => (await toasts('error')).includes('the workspace disk is full'))
  step('the host\'s refusal shows as the export\'s error', (await toasts('error')).includes('the workspace disk is full'), await toasts('error'))
  step('and no file is claimed or written', (await names()).length === before && !(await toasts('success')).includes('Sample 3.pdf'))
  await page.evaluate(() => {
    delete window.devHost.files.set
  })

  // The agent's export (kit.json `agent.export`): the open editor runs the engine's command, which
  // answers the file for the host to write.
  const agent = await page.evaluate(async () => {
    const result = await window.devHost.connection().ai.run([{ command: 'file.export', params: { format: 'pdf' } }], false)
    return result.completed[0].result
  })
  step('the agent\'s export command answers the PDF bytes', agent?.extension === 'pdf' && atob(agent.data.slice(0, 8)).startsWith('%PDF-'), JSON.stringify(agent).slice(0, 120))
  const changes = await page.evaluate(() => window.devHost.changes.filter((state) => state.dirty).length)
  step('exporting does not change the document', changes === 0, `${changes} dirty changes`)
  step('no page errors', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} failed (${which})` : `all passed (${which})`)
process.exit(failures ? 1 : 0)
