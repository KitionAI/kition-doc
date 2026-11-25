// A document changed outside Kition keeps its history (Kit UI protocol UI-R1, section 7):
// node tests/replica-merge.mjs [chromium|webkit]
// Through the development host with a stored replica and checkpoints: the person's edit is saved,
// another program changes the .docx (here a second editor without a replica), and the document
// reopens with the stored replica (the host always hands it over), on which the engine records
// what the newer file holds as one `import` change. The outside change shows, the paragraphs keep
// their object ids, and the replica and its checkpoints stay. Opening the file alone (no replica)
// gives new ids, so the ids kept are the replica's, not the file's.
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
const errors = []
const PATH = 'Sample.docx'

async function until(page, check, what, tries = 150) {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  throw new Error(`timed out: ${what}`)
}
async function openPage(query) {
  const page = await context.newPage()
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(1000)
  return page
}
const run = (page, steps) => page.evaluate(async (steps) => (await window.devHost.connection().ai.run(steps, false)).completed.map((step) => step.result), steps)
const bodyText = async (page) => (await run(page, [{ command: 'file.inspect', params: {} }]))[0].blocks.map((block) => block.text).join('\n')
/** The object ids of the body's paragraphs, as the replica names them (`presence.caret` of the whole body). */
const paragraphIds = async (page) => (await run(page, [{ command: 'edit.selectAll', params: {} }, { command: 'presence.caret', params: {} }]))[1].selection
const version = (page) => page.evaluate((path) => window.devHost.files.get(path).version, PATH)
async function save(page) {
  const before = await version(page)
  await page.frameLocator('#kit').getByTestId('doc-save').click()
  await until(page, async () => (await version(page)) > before, 'saved through the host')
}
const append = (page, text) => run(page, [{ command: 'caret.docEnd', params: {} }, { command: 'text.insert', params: { text } }])

try {
  // The person's editor: the host keeps the replica and its checkpoints.
  const page = await openPage('&replica=1&checkpoints=1')
  await append(page, ' Kept here.')
  await until(page, async () => (await bodyText(page)).includes('Kept here.'), 'the edit')
  await save(page)
  const ids = await paragraphIds(page)
  assert.ok(Array.isArray(ids) && ids.length > 1, `the paragraphs' ids: ${JSON.stringify(ids)}`)
  assert.ok(await page.evaluate((path) => window.devHost.history.replica(path) != null, PATH), 'the save stored the replica')
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save' }))
  assert.ok(checkpoint, 'a checkpoint of the saved version')
  console.log('PASS edit, save, replica and checkpoint kept')

  // Another program changes the file: an editor without a replica opens the saved bytes and saves.
  const saved = await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], PATH)
  const outside = await openPage('')
  await outside.evaluate((bytes) => window.devHost.open('Sample.docx', new Uint8Array(bytes).buffer), saved)
  await until(outside, async () => (await bodyText(outside)).includes('Kept here.'), 'the saved file in the other program')
  await append(outside, ' Changed outside.')
  await until(outside, async () => (await bodyText(outside)).includes('Changed outside.'), 'the outside edit')
  await save(outside)
  const changed = await outside.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], PATH)
  await outside.close()

  // The file changed outside reopens with the stored replica: the engine records the difference.
  await page.evaluate((bytes) => window.devHost.reopenChanged(new Uint8Array(bytes).buffer), changed)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await until(page, async () => (await bodyText(page)).includes('Changed outside.'), 'the outside change shows')
  const text = await bodyText(page)
  assert.ok(text.includes('Kept here. Changed outside.'), `the person's edit and the outside change: ${text.slice(-200)}`)
  assert.deepEqual(await paragraphIds(page), ids, 'the paragraphs keep their object ids (the stored replica, with the import change)')
  assert.equal(await page.frameLocator('#kit').getByTestId('doc-unsaved').count(), 0, 'taking the newer file is no unsaved edit')
  assert.ok(await page.evaluate((path) => window.devHost.history.replica(path) != null, PATH), 'the replica is still kept')
  // The next save writes the replica on, and the earlier checkpoint stays.
  await append(page, ' After.')
  await until(page, async () => (await bodyText(page)).includes('Changed outside. After.'), 'an edit after the import')
  await save(page)
  const kept = await page.evaluate((path) => window.devHost.history.checkpoints(path), PATH)
  assert.ok(kept.some((entry) => entry.id === checkpoint.id), `the earlier checkpoint stays: ${JSON.stringify(kept)}`)
  console.log('PASS a file changed outside opens on the stored replica')

  // The control: the same file opened alone (its replica dropped) has other ids.
  const current = await page.evaluate((path) => [...new Uint8Array(window.devHost.files.get(path).bytes)], PATH)
  await page.evaluate((bytes) => window.devHost.open('Sample.docx', new Uint8Array(bytes).buffer), current)
  await until(page, async () => (await bodyText(page)).includes('Changed outside. After.'), 'the file opened alone')
  assert.notDeepEqual(await paragraphIds(page), ids, 'a file opened alone gets new ids')
  console.log('PASS a file opened alone starts a new replica')

  assert.deepEqual(errors, [], 'no page errors')
  console.log(`PASS file changed outside (${which})`)
} finally {
  await browser.close()
  await server.close()
}
