// Comments a host keeps (Kit UI protocol 2.6 section 7k), in a real browser against a mocked host
// (tests/host/comments.html): node tests/host-comments.mjs [chromium|webkit]
// Select a passage and click Comment: the kit sends `comment.create` with the selection's anchor
// instead of writing a Word comment; the host's thread comes back as a marker beside its line that
// follows the text through edits and page breaks; clicking it sends `comments/activated`. The
// file's own Word comments stay the kit's. A viewer gets markers but no Comment; a commenter both.
import './platform.mjs'
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
const run = (command, params = {}) => page.evaluate(async ({ command, params }) => (await window.mock.connection.ai.run([{ command, params }], false)).completed[0]?.result, { command, params })
// An agent's steps in one run: each run starts from the person's selection (the agent's own
// editor state lasts one run, KHP C3b), so steps that build on each other's selection go together.
const runSteps = (steps) => page.evaluate(async (steps) => (await window.mock.connection.ai.run(steps, false)).completed.map((step) => step.result), steps)
const mock = () => page.evaluate(() => ({ created: window.mock.created, activated: window.mock.activated, threads: window.mock.threads, capabilities: window.mock.connection.capabilities }))

async function open(role) {
  await page.goto(`${origin}/tests/host/comments.html${role ? `?role=${role}` : ''}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent === 'ready', null, { timeout: 60000 })
  await page.waitForTimeout(1000)
  return page.frameLocator('#kit')
}

try {
  let kit = await open(null)
  // The person selects `text` with Find (Cmd/Ctrl+F; the engine's edit.find selects the first
  // match after the caret): the Comment button and the pane follow the person's selection, which
  // an agent's run leaves as it was.
  const selectAsPerson = async (text) => {
    if ((await kit.getByTestId('doc-find-text').count()) === 0) {
      await kit.getByTestId('kit-text-sink').evaluate((element) => element.focus())
      await page.keyboard.press('ControlOrMeta+f')
    }
    await kit.getByTestId('doc-find-text').fill(text)
    await until(async () => /^[1-9]/.test((await kit.getByTestId('doc-find-count').textContent()) ?? ''))
  }
  const marker = (id) => kit.locator(`[data-testid=doc-host-comment-marker][data-thread=${id}]`)
  // Where a marker is on screen, or null when it is not drawn.
  const box = async (id) => ((await marker(id).count()) === 1 ? marker(id).boundingBox({ timeout: 1000 }) : null)
  const add = () => kit.getByTestId('doc-host-comment-add')
  step('the kit lists comment-anchors', (await mock()).capabilities.includes('comment-anchors'), (await mock()).capabilities.join(' '))
  step('no Comment button without a selection', (await add().count()) === 0)

  // Select a passage, then Comment.
  await selectAsPerson('charged particles')
  await until(async () => (await add().count()) === 1)
  step('a selection offers Comment beside it', (await add().count()) === 1)
  await add().click()
  await until(async () => (await mock()).created.length === 1)
  const created = (await mock()).created[0]
  step('Comment sends comment.create with the selection\'s anchor', created?.anchor?.quote === 'charged particles' && created.anchor.ids.length >= 1 && created.anchor.range && created.anchor.range.end > created.anchor.range.start, JSON.stringify(created))
  step('the anchor names its page, from 1', created?.anchor?.page === 1 && created.anchor.label === 'Page 1', JSON.stringify(created?.anchor))
  const native = await run('comment.threads')
  step('no Word comment is written into the file', Array.isArray(native) && native.length === 0, JSON.stringify(native))

  // The person's caret goes back to the start, where it was before Find: the agent's edits below
  // move the passage, and the editor keeps the person's caret in view.
  await kit.getByTestId('kit-text-sink').evaluate((element) => element.focus())
  await page.keyboard.press('ControlOrMeta+Home')

  // The host shows its thread: a marker beside the line, the passage tinted.
  await until(async () => (await marker('t1').count()) === 1)
  step('the host\'s thread shows as a marker', (await marker('t1').count()) === 1)
  const first = await box('t1')
  await marker('t1').click()
  await until(async () => (await mock()).activated.length === 1)
  step('clicking the marker sends comments/activated with the thread', JSON.stringify((await mock()).activated) === '["t1"]', JSON.stringify((await mock()).activated))
  step('the host finds the anchor in the kit (comments/locate)', JSON.stringify(await page.evaluate(() => window.mock.connection.locateComments([window.mock.created[0].anchor, { ids: ['nowhere'], quote: 'not in this document' }]))) === '[true,false]')

  // Edits move the marker with its text.
  await runSteps([{ command: 'caret.docStart', params: {} }, { command: 'text.newParagraph', params: {} }, { command: 'text.newParagraph', params: {} }, { command: 'text.insert', params: { text: 'Added at the top.' } }])
  await until(async () => ((await box('t1'))?.y ?? 0) > (first?.y ?? 0) + 10)
  const moved = await box('t1')
  step('paragraphs added above move the marker down with its text', moved && first && moved.y > first.y + 10, `${first?.y} -> ${moved?.y}`)
  await runSteps([{ command: 'select.text', params: { text: 'charged' } }, { command: 'select.collapse', params: { end: true } }, { command: 'text.insert', params: { text: ', fast' } }])
  const located = (await run('comment.locate', { anchors: [created.anchor] }))?.[0]
  step('an edit inside the passage keeps the anchor around it', located?.found === true && located.text === 'charged, fast particles', JSON.stringify(located))

  // A page break above: the anchor moves to page 2, and so does its marker.
  await runSteps([{ command: 'caret.docStart', params: {} }, { command: 'insert.pageBreak', params: {} }])
  const paged = (await run('comment.locate', { anchors: [created.anchor] }))?.[0]
  step('a page break above moves the anchor to page 2', paged?.found === true && paged.page === 2, JSON.stringify(paged))
  await until(async () => {
    const at = await box('t1')
    return at == null || at.y > (moved?.y ?? 0) + 200
  })
  const left = await box('t1')
  step('the marker left page 1 with its text', left == null || left.y > (moved?.y ?? 0) + 200, JSON.stringify(left))
  const canvas = kit.locator('.doc-page-viewport canvas').first()
  const area = await canvas.boundingBox()
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2)
  await until(async () => {
    const at = await box('t1')
    if (at && at.y > area.y && at.y < area.y + area.height - 30) return true
    await page.mouse.wheel(0, 300)
    return false
  }, 60)
  step('scrolled to page 2, the marker is beside its line there', (await marker('t1').count()) === 1)

  // The file's own Word comments stay the kit's.
  await runSteps([{ command: 'select.text', params: { text: 'oxygen' } }, { command: 'review.newComment', params: { text: 'A Word comment' } }])
  await kit.getByTestId('doc-comments-toggle').click()
  await until(async () => (await kit.getByTestId('doc-comments-panel').textContent())?.includes('A Word comment'))
  step('the Comments pane still lists the file\'s Word comment', (await kit.getByTestId('doc-comments-panel').textContent())?.includes('A Word comment'))
  await selectAsPerson('red flashlight')
  await until(async () => (await add().count()) === 1)
  await kit.getByTestId('doc-comments-new').click()
  await until(async () => (await mock()).created.length === 2)
  step('the pane\'s New comment starts the host\'s thread', (await mock()).created[1]?.anchor?.quote === 'red flashlight', JSON.stringify((await mock()).created[1]))
  step('and writes no second Word comment', (await run('comment.threads'))?.length === 1)

  // Review › New Comment (Word's own command) is the host's thread too.
  await kit.getByTestId('ribbon-tab-review').click()
  await selectAsPerson('Spare batteries')
  await until(async () => (await add().count()) === 1)
  await until(async () => !(await kit.getByTestId('doc-cmd-review.newComment').isDisabled()))
  await kit.getByTestId('doc-cmd-review.newComment').click()
  await until(async () => (await mock()).created.length === 3)
  step('Review › New Comment sends comment.create', (await mock()).created[2]?.anchor?.quote === 'Spare batteries', JSON.stringify((await mock()).created[2]))
  step('still one Word comment', (await run('comment.threads'))?.length === 1)

  // A resolved thread has no marker.
  await page.evaluate(() => window.mock.show(window.mock.threads.map((thread) => (thread.id === 't1' ? { ...thread, resolved: true } : thread))))
  await until(async () => (await marker('t1').count()) === 0)
  step('a resolved thread has no marker', (await marker('t1').count()) === 0)
  step('no page errors', errors.length === 0, errors.join(' | '))

  // A viewer: markers to open threads, no Comment (the host offers no comment.create, UI-V4).
  kit = await open('viewer')
  step('a viewer is offered no comment.create', !(await mock()).capabilities.includes('service:comment.create') && (await mock()).capabilities.includes('comment-anchors'))
  const [, anchor] = await runSteps([{ command: 'select.text', params: { text: 'charged particles' } }, { command: 'comment.anchor', params: {} }])
  await page.evaluate((at) => window.mock.show([{ id: 'v1', anchor: { ids: at.ids, range: at.range, quote: at.quote }, resolved: false, count: 3 }]), anchor)
  await until(async () => (await kit.locator('[data-testid=doc-host-comment-marker][data-thread=v1]').count()) === 1)
  step('a viewer sees the thread\'s marker', (await kit.locator('[data-testid=doc-host-comment-marker][data-thread=v1]').count()) === 1)
  step('and no Comment button on a selection', (await kit.getByTestId('doc-host-comment-add').count()) === 0)
  await kit.locator('[data-testid=doc-host-comment-marker][data-thread=v1]').click()
  await until(async () => (await mock()).activated.includes('v1'))
  step('a viewer\'s click opens the thread in the host', (await mock()).activated.includes('v1'))

  // A commenter: read-only content, comments through the host.
  kit = await open('commenter')
  await selectAsPerson('charged particles')
  await until(async () => (await kit.getByTestId('doc-host-comment-add').count()) === 1)
  await kit.getByTestId('doc-host-comment-add').click()
  await until(async () => (await mock()).created.length === 1)
  step('a commenter comments through the host', (await mock()).created[0]?.anchor?.quote === 'charged particles', JSON.stringify((await mock()).created))
  step('no page errors (viewer, commenter)', errors.length === 0, errors.join(' | '))
} catch (error) {
  failures += 1
  console.log(`FAIL ${error.stack ?? error}`)
} finally {
  await browser.close()
  await server.close()
}
console.log(failures ? `${failures} failed (${which})` : `all passed (${which})`)
process.exit(failures ? 1 : 0)
