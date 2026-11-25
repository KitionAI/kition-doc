// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1000)
  const bodyText = () => page.evaluate(async () => {
    const result = await window.devHost.connection().ai.run([{ command: 'file.inspect', params: {} }], false)
    return result.completed[0].result.blocks.map((block) => block.text).join('\n')
  })
  step('Chinese tab names from the upstream catalog', (await kit.getByTestId('ribbon-tab-home').textContent()) === '开始')
  step('status shows pages and words', /第 1 页/.test(await kit.getByTestId('doc-status').textContent()))
  const zoom = Number((await kit.getByTestId('kit-zoom').textContent())?.replace('%', ''))
  step('opens at page width', zoom > 100, `${zoom}%`)
  step('no header tab in the body', (await kit.getByTestId('ribbon-tab-headerFooter').count()) === 0)

  // Click into the first paragraph and type Chinese through an input method.
  const region = await kit.getByTestId('doc-canvas-region').boundingBox()
  await page.mouse.click(region.x + 340, region.y + 300)
  await page.waitForTimeout(300)
  await page.keyboard.press('End')
  if (which === 'chromium') {
    const cdp = await page.context().newCDPSession(page)
    await cdp.send('Input.imeSetComposition', { text: 'ji', selectionStart: 2, selectionEnd: 2 })
    await cdp.send('Input.insertText', { text: '极光' })
  } else {
    await page.keyboard.insertText('极光')
  }
  await until(async () => (await bodyText()).includes('极光'))
  step('types Chinese through the input method', (await bodyText()).includes('极光'))

  // Select the word and bold it; a style from the gallery.
  await page.keyboard.down('Shift')
  await page.keyboard.press('ArrowLeft')
  await page.keyboard.press('ArrowLeft')
  await page.keyboard.up('Shift')
  await page.waitForTimeout(300)
  await kit.getByTestId('doc-cmd-format.bold').click()
  await until(async () => (await kit.getByTestId('doc-cmd-format.bold').getAttribute('aria-pressed')) === 'true')
  step('bolds the selection', (await kit.getByTestId('doc-cmd-format.bold').getAttribute('aria-pressed')) === 'true')
  await kit.getByTestId('doc-style-Heading2').click()
  await until(async () => (await kit.getByTestId('doc-style-Heading2').getAttribute('aria-pressed')) === 'true')
  step('applies a style from the gallery', (await kit.getByTestId('doc-style-Heading2').getAttribute('aria-pressed')) === 'true')

  // A 3 × 2 table from the grid; the table tabs appear.
  await kit.getByTestId('ribbon-tab-insert').click()
  await kit.getByTestId('doc-table').click()
  await kit.getByTestId('kui-table-2x3').click()
  await until(async () => (await kit.getByTestId('ribbon-tab-tableLayout').count()) === 1)
  step('inserts a table and shows the table tabs', (await kit.getByTestId('ribbon-tab-tableLayout').count()) === 1)
  await kit.getByTestId('ribbon-tab-tableLayout').click()
  step('the table layout tab lists the engine commands', (await kit.getByTestId('doc-cmd-table.insertRowBelow').count()) === 1)

  // Find and replace.
  await kit.getByTestId('ribbon-tab-home').click()
  await kit.getByTestId('doc-replace').click()
  await kit.getByTestId('doc-find-text').pressSequentially('oxygen', { delay: 20 })
  await until(async () => (await kit.getByTestId('doc-find-count').textContent())?.endsWith('/2'))
  step('finds matches', (await kit.getByTestId('doc-find-count').textContent())?.endsWith('/2'), await kit.getByTestId('doc-find-count').textContent())
  await kit.getByTestId('doc-replace-text').fill('O₂')
  await kit.getByTestId('doc-replace-all').click()
  await until(async () => !(await bodyText()).includes('oxygen'))
  step('replaces all', (await bodyText()).includes('O₂') && !(await bodyText()).includes('oxygen'))

  // The agent edits; save goes through the host; the person undoes from the ribbon.
  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'caret.docEnd', params: {} }, { command: 'text.insert', params: { text: ' Written by the agent.' } }], false))
  await page.waitForTimeout(500)
  step('the agent edits the live document', run.completed[1].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  await kit.getByTestId('doc-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.docx').version)) > 1)
  step('saves through the host', (await page.evaluate(() => window.devHost.files.get('Sample.docx').version)) > 1)
  // The person's Undo takes back their own last step (Replace All) and keeps the agent's
  // (KitModel 1.1 M12a; KitAgent A7).
  await kit.getByTestId('doc-undo').click()
  await until(async () => (await bodyText()).includes('oxygen'))
  step('undo from the ribbon', (await bodyText()).includes('oxygen') && (await bodyText()).includes('Written by the agent.'))

  // A commenter: the document is read-only, yet the comment commands run (kit.json
  // collaboration.comments.commands) and every other edit is refused.
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN&role=commenter`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await page.waitForTimeout(500)
  const agent = (command, params = {}) => page.evaluate(async ({ command, params }) => {
    try {
      return { result: (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result }
    } catch (error) {
      return { error: String(error?.message ?? error) }
    }
  }, { command, params })
  const before = await bodyText()
  step('a commenter cannot type', /read-only/.test((await agent('text.insert', { text: 'Typed by a commenter' })).error ?? ''))
  step('a commenter cannot undo', /read-only/.test((await agent('edit.undo')).error ?? ''))
  const added = await agent('comment.add', { text: 'From a commenter' })
  const threads = async () => (await agent('comment.threads')).result ?? []
  step('a commenter comments', !added.error && (await threads()).some((thread) => thread.comments[0].body === 'From a commenter'), added.error)
  await kit.getByTestId('doc-comments-toggle').click()
  await kit.getByTestId('doc-comments-new').click()
  await kit.getByTestId('doc-comments-panel-new-input').fill('Typed in the panel')
  await kit.getByTestId('doc-comments-panel-new-send').click()
  await until(async () => (await threads()).some((thread) => thread.comments[0].body === 'Typed in the panel'))
  step('a commenter comments from the panel, as themselves', (await threads()).some((thread) => thread.comments[0].body === 'Typed in the panel' && thread.comments[0].author === 'dev-commenter'))
  step('a commenter leaves the text as it was', (await bodyText()) === before)
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
