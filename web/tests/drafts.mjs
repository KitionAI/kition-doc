// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m),
// through the development host with `?replica=1&checkpoints=1`: node tests/drafts.mjs [chromium|webkit]
// a. The person types into a paragraph; the agent's draft adds text to the last paragraph; its
//    comparison lists the change and the document does not show it; the person types again
//    meanwhile; the person accepts the draft: both of the person's edits and the draft's are in
//    the document.
// b. One click of the editor's Undo takes back the draft's edit only; Redo brings it back.
// d. A checkpoint with a label at a save, the person types after it, the checkpoint restored
//    through the host: the document is the checkpoint's; one Undo brings the later edit back.
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const PATH = 'Sample.docx'
const run = (command, params = {}, options = {}) =>
  page.evaluate(async ({ command, params, options }) => (await window.devHost.connection().ai.run([{ command, params }], false, options)).completed.at(-1).result, { command, params, options })
const until = async (check, message) => {
  for (let n = 0; n < 200; n += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(`${message}: ${(await body().catch(() => [])).join(' | ').slice(-400)}`)
}
/** The body's paragraphs as the editor's inspect answers them, in the document or (`draft`) in the agent's draft. */
const body = async (draft = false) => (await run('file.inspect', {}, draft ? { draft: true } : {})).blocks.map((block) => block.text ?? '')
const holds = async (text, draft = false) => (await body(draft)).some((line) => line.includes(text))
/** The person clicks into the page's first lines and types `text` at the end of that paragraph. */
const typeByHand = async (text) => {
  const region = await kit.getByTestId('doc-canvas-region').boundingBox()
  await page.mouse.click(region.x + 340, region.y + 300)
  await page.keyboard.press('End')
  await page.keyboard.type(text)
  await until(() => holds(text), `the person's "${text}" shows`)
  return (await body()).findIndex((line) => line.includes(text))
}
const press = async (id) => {
  await until(() => kit.getByTestId(id).isEnabled(), `${id} is enabled`)
  await kit.getByTestId(id).click()
}
const version = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, PATH)
let failures = 0

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic&locale=en-US&replica=1&checkpoints=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await until(async () => (await body()).length > 1, 'the sample opens with its paragraphs')

  // a. The person's first edit, then the agent's draft on another paragraph.
  const PERSON = ' Typed first.'
  const LATER = ' Typed meanwhile.'
  const DRAFT = ' From the draft.'
  const typed = await typeByHand(PERSON)
  const paragraphs = (await body()).length
  assert.notEqual(typed, paragraphs - 1, 'the person typed into another paragraph than the last')
  await page.evaluate(async (text) => window.devHost.connection().ai.run([{ command: 'caret.docEnd', params: {} }, { command: 'text.insert', params: { text } }], false, { draft: true }), DRAFT)
  assert.equal((await body(true)).findIndex((line) => line.includes(DRAFT)), paragraphs - 1, 'the draft wrote into the last paragraph')
  assert.ok(await holds(DRAFT, true), 'the draft holds the agent\'s text')
  assert.ok(!(await holds(DRAFT)), 'the document does not, yet')
  // The comparison lists the draft's change and changes nothing in the document.
  const compared = await page.evaluate(() => window.devHost.connection().draft.compare({ maxSize: 256 }))
  const listed = compared.summary.changed.length + compared.summary.created.length + (compared.summary.texts?.length ?? 0)
  assert.ok(listed > 0, `the preview lists the draft's change: ${JSON.stringify(compared.summary).slice(0, 400)}`)
  assert.ok(!(await holds(DRAFT)), 'comparing changes nothing in the document')
  // The person types again meanwhile, then accepts the draft.
  await typeByHand(LATER)
  const merged = await page.evaluate(() => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
  assert.equal(merged.merged, true, 'the draft merged')
  await until(() => holds(DRAFT), 'the merged text shows')
  assert.ok(await holds(PERSON), 'the person\'s first edit stays')
  assert.ok(await holds(LATER), 'the edit made meanwhile stays')
  assert.equal((await body()).filter((line) => line.includes(DRAFT)).length, 1, 'the draft\'s text is in once')
  console.log(`PASS ${which}: a draft compared, then accepted over the person's edits`)

  // b. One Undo takes back the draft's edit only; Redo brings it back.
  await press('doc-undo')
  await until(async () => !(await holds(DRAFT)), 'Undo takes the draft back')
  assert.ok((await holds(PERSON)) && (await holds(LATER)), 'the person\'s edits stay after Undo')
  await press('doc-redo')
  await until(() => holds(DRAFT), 'Redo brings the draft back')
  assert.ok((await holds(PERSON)) && (await holds(LATER)), 'the person\'s edits stay after Redo')
  console.log(`PASS ${which}: one Undo takes the draft back, Redo brings it again`)

  // d. A labelled checkpoint at a save, an edit after it, the checkpoint restored, Undo.
  const saved = await version()
  await kit.getByTestId('doc-save').click()
  await until(async () => (await version()) > saved, 'saved through the host')
  await until(() => page.evaluate((path) => window.devHost.history.replica(path) != null, PATH), 'the replica is stored with the save')
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save', label: 'Before the review' }))
  assert.equal(checkpoint?.label, 'Before the review', `a labelled checkpoint: ${JSON.stringify(checkpoint)}`)
  const atCheckpoint = await body()
  const AFTER = ' After the checkpoint.'
  await typeByHand(AFTER)
  await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
  await until(async () => !(await holds(AFTER)), 'the restore takes the later edit away')
  assert.deepEqual(await body(), atCheckpoint, 'the document is the checkpoint\'s')
  await press('doc-undo')
  await until(() => holds(AFTER), 'Undo brings the later edit back')
  assert.ok((await holds(DRAFT)) && (await holds(PERSON)), 'the earlier edits stay')
  console.log(`PASS ${which}: a restored checkpoint is one step of the person's Undo`)
  assert.deepEqual(errors, [], 'no page errors')
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
