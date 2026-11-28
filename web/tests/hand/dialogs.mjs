// The editor's dialogs and the buttons that showed nothing (plan "Doc usable by hand", stage 6A;
// walkthrough G8, G11, K3, K5, G13, H5, L2, V1, G6; WPS W-L6): Cmd+D opens Font, a size typed
// there is the selection's after OK, one undo step back; Paragraph (Alt+Cmd+M, Layout › Indents)
// takes a first-line indent of 2 characters, 24 pt for 12 pt text; Cmd+Alt+A on a selected word
// opens the new comment's box with the keyboard in it, the word stays, Cmd+Enter adds the comment;
// Cmd+K makes the selected word a link; Track Changes is pressed while on; Show/Hide ¶ draws the
// marks; Table Design's Shading is a menu; Read Mode says so and has a way out; the alignment
// buttons show the paragraph's; the dialogs' labels are Chinese in Chinese.
// node tests/hand/dialogs.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const at = (path, off = 0) => ({ story: 'body', path, off })

/** Setup (the agent's): two plain 12 pt paragraphs, "alpha beta gamma" and "second paragraph here". */
async function plain(e) {
  await e.agent([
    { command: 'edit.selectAll', params: {} },
    { command: 'text.delete', params: {} },
    { command: 'text.insert', params: { text: 'alpha beta gamma' } },
    { command: 'text.newParagraph', params: {} },
    { command: 'text.insert', params: { text: 'second paragraph here' } },
    { command: 'edit.selectAll', params: {} },
    { command: 'format.set', params: { props: { size: 12 } } },
    { command: 'paragraph.set', params: { props: { indentFirst: 0, indentLeft: 0, align: 'left' } } },
  ])
  await e.until(async () => (await e.paragraphs()).join('|') === 'alpha beta gamma|second paragraph here')
  // The person's own session has the agent's edit too (it arrives there a moment later).
  const mine = async (index) => (await e.read('file.paragraph', { path: [index] }))?.paragraph?.text
  await e.until(async () => (await mine(0)) === 'alpha beta gamma' && (await mine(1)) === 'second paragraph here')
  await e.wait(200)
}
/** Setup: "beta" selected, as the person's own selection. */
const selectBeta = async (e) => {
  await e.setup('select.range', { anchor: at([0], 6), focus: at([0], 10) })
  await e.until(async () => (await e.selection())?.text === 'beta')
  await e.wait(150)
  assert.equal((await e.selection())?.text, 'beta', 'setup: "beta" is selected')
}
const dialog = (e, id) => e.kit.getByTestId(id)
const shown = async (e, id) => assert(await e.until(() => dialog(e, id).isVisible(), 40), `${id} opens`)
const gone = async (e, id) => assert(await e.until(async () => (await dialog(e, id).count()) === 0, 40), `${id} closes`)
/** The element with the keyboard: its test id. */
const focused = (e) => e.frame().evaluate(() => document.activeElement?.getAttribute('data-testid') ?? document.activeElement?.tagName ?? null)
const threads = (e) => e.read('comment.threads')

const list = [
  ['g8-font-dialog', async (e) => {
    await plain(e)
    await selectBeta(e)
    await e.page.keyboard.press('ControlOrMeta+d')
    await shown(e, 'doc-font-dialog')
    assert(await e.until(async () => ((await focused(e)) ?? '').startsWith('doc-font-dialog-')), `a field of the dialog has the keyboard (${await focused(e)})`)
    const size = e.kit.getByTestId('doc-font-dialog-size')
    assert.equal(await size.inputValue(), '12', 'the dialog starts from the selection')
    await size.fill('13')
    await e.kit.getByTestId('doc-font-dialog-ok').click()
    await gone(e, 'doc-font-dialog')
    assert(await e.until(async () => (await e.read('format.state'))?.size === 13), 'the selection is 13 pt')
    assert.equal((await e.selection())?.text, 'beta', 'the selection stays')
    await e.page.keyboard.press('ControlOrMeta+z')
    assert(await e.until(async () => (await e.read('format.state'))?.size === 12), 'one undo step takes it back')
  }],
  ['g8-font-dialog-chinese-size', async (e) => {
    await plain(e)
    await selectBeta(e)
    await e.page.keyboard.press('ControlOrMeta+d')
    await shown(e, 'doc-font-dialog')
    await e.until(async () => (await e.kit.getByTestId('doc-font-dialog-size').inputValue()) === '12')
    await e.kit.getByTestId('doc-font-dialog-size').fill('小三')
    await e.kit.getByTestId('doc-font-dialog-ea').fill('黑体')
    await e.kit.getByTestId('doc-font-dialog-ok').click()
    await gone(e, 'doc-font-dialog')
    assert(await e.until(async () => (await e.read('format.state'))?.size === 15), `小三 is 15 pt (${JSON.stringify(await e.read('format.state'))}, ${await dialog(e, 'doc-font-dialog').count()})`)
    const runs = (await e.paragraph(0))?.paragraph?.runs ?? []
    assert(JSON.stringify(runs).includes('黑体'), `the Chinese font is the selection's (${JSON.stringify(runs).slice(0, 300)})`)
    // Escape drops a dialog with nothing changed.
    await e.page.keyboard.press('ControlOrMeta+d')
    await shown(e, 'doc-font-dialog')
    await e.page.keyboard.press('Escape')
    await gone(e, 'doc-font-dialog')
  }],
  ['l2-paragraph-dialog', async (e) => {
    await plain(e)
    await e.setup('caret.set', { pos: at([1], 3) })
    await e.wait(200)
    await e.page.keyboard.press('ControlOrMeta+Alt+KeyM')
    await shown(e, 'doc-paragraph-dialog')
    await e.until(async () => (await e.kit.getByTestId('doc-paragraph-dialog-special').count()) > 0)
    await e.kit.getByTestId('doc-paragraph-dialog-special').selectOption('firstLine')
    await e.kit.getByTestId('doc-paragraph-dialog-by-unit').selectOption('char')
    await e.kit.getByTestId('doc-paragraph-dialog-by').fill('2')
    await e.kit.getByTestId('doc-paragraph-dialog-ok').click()
    await gone(e, 'doc-paragraph-dialog')
    assert(await e.until(async () => (await e.paragraph(1))?.resolved?.indentFirst === 24), `2 characters of 12 pt text are 24 pt (${(await e.paragraph(1))?.resolved?.indentFirst})`)
    assert.equal((await e.paragraph(0))?.resolved?.indentFirst ?? 0, 0, 'the other paragraph is as it was')
    await e.page.keyboard.press('ControlOrMeta+z')
    assert(await e.until(async () => ((await e.paragraph(1))?.resolved?.indentFirst ?? 0) === 0), 'one undo step takes it back')
  }],
  ['l2-layout-indents', async (e) => {
    await plain(e)
    await e.setup('caret.set', { pos: at([0], 2) })
    await e.kit.getByTestId('ribbon-tab-layout').click()
    await e.wait(250)
    const button = e.kit.locator('.kui-line-inline').getByTestId('doc-cmd-paragraph.indents')
    await button.click()
    await shown(e, 'doc-paragraph-dialog')
    await e.until(async () => (await e.kit.getByTestId('doc-paragraph-dialog-left').count()) > 0)
    await e.kit.getByTestId('doc-paragraph-dialog-left-unit').selectOption('pt')
    await e.kit.getByTestId('doc-paragraph-dialog-left').fill('36')
    await e.kit.getByTestId('doc-paragraph-dialog-before').fill('6')
    await e.kit.getByTestId('doc-paragraph-dialog-ok').click()
    await gone(e, 'doc-paragraph-dialog')
    assert(await e.until(async () => (await e.paragraph(0))?.resolved?.indentLeft === 36), 'Layout › Indents sets the left indent')
    assert.equal((await e.paragraph(0))?.resolved?.spaceBefore, 6, 'and the space before, in the same OK')
  }],
  ['k3-new-comment', async (e) => {
    await plain(e)
    const before = (await threads(e))?.length ?? 0
    await selectBeta(e)
    await e.page.keyboard.press('ControlOrMeta+Alt+KeyA')
    const box = e.kit.getByTestId('doc-comments-panel-new-input')
    assert(await e.until(() => box.isVisible(), 40), 'the new comment box opens')
    assert(await e.until(async () => (await focused(e)) === 'doc-comments-panel-new-input'), `the box has the keyboard (${await focused(e)})`)
    await e.page.keyboard.type('check this')
    assert.equal((await e.paragraphs())[0], 'alpha beta gamma', 'typing goes into the comment, not over the selected word')
    await e.page.keyboard.press('ControlOrMeta+Enter')
    assert(await e.until(async () => ((await threads(e))?.length ?? 0) === before + 1), 'Cmd+Enter adds the comment')
    const made = (await threads(e)).at(-1)
    assert.equal(made?.comments?.[0]?.body ?? made?.comments?.[0]?.text, 'check this', `the comment says what was typed (${JSON.stringify(made).slice(0, 200)})`)
    assert.equal((await e.paragraphs())[0], 'alpha beta gamma', 'the word is still there')
    // Escape on a new box drops it: no empty comment.
    await e.page.keyboard.press('ControlOrMeta+Alt+KeyA')
    assert(await e.until(() => box.isVisible(), 40))
    await e.page.keyboard.press('Escape')
    assert(await e.until(async () => (await box.count()) === 0), 'Escape closes the box')
    assert.equal((await threads(e))?.length, before + 1, 'no empty comment')
    assert(await e.until(async () => (await e.active()) === 'textarea.kui-text-sink'), `the keyboard is back in the text (${await e.active()})`)
  }],
  ['k3-insert-comment-button', async (e) => {
    await plain(e)
    await selectBeta(e)
    await e.kit.getByTestId('ribbon-tab-insert').click()
    await e.wait(250)
    await e.kit.locator('.kui-line-inline').getByTestId('doc-cmd-comment.new').click()
    assert(await e.until(() => e.kit.getByTestId('doc-comments-panel-new-input').isVisible(), 40), 'Insert › Comment opens the comment box, not a parameters table')
    assert.equal(await e.kit.getByTestId('command-dialog').count(), 0, 'no parameters dialog')
    await e.page.keyboard.press('Escape')
  }],
  ['g11-link-dialog', async (e) => {
    await plain(e)
    await selectBeta(e)
    await e.page.keyboard.press('ControlOrMeta+k')
    await shown(e, 'doc-link-dialog')
    assert(await e.until(async () => (await focused(e)) === 'doc-link-dialog-url'), 'the address has the keyboard')
    assert.equal(await e.kit.getByTestId('doc-link-dialog-text').inputValue(), 'beta', 'the selected text is the link text')
    await e.page.keyboard.type('example.com')
    await e.page.keyboard.press('Enter')
    await gone(e, 'doc-link-dialog')
    assert(await e.until(async () => JSON.stringify((await e.paragraph(0))?.paragraph?.runs ?? []).includes('https://example.com')), 'the selected word is a link')
    assert.equal((await e.paragraphs())[0], 'alpha beta gamma', 'its text is the same')
  }],
  ['k5-track-changes-state', async (e) => {
    await e.kit.getByTestId('ribbon-tab-review').click()
    await e.wait(250)
    const button = e.kit.locator('.kui-line-inline').getByTestId('doc-cmd-change.track')
    assert.equal(await button.getAttribute('aria-pressed'), 'false', 'off at first')
    await button.click()
    assert(await e.until(async () => (await button.getAttribute('aria-pressed')) === 'true'), 'pressed while tracking')
    await button.click()
    assert(await e.until(async () => (await button.getAttribute('aria-pressed')) === 'false'), 'not pressed after')
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['g13-show-marks', async (e) => {
    await plain(e)
    await e.setup('caret.docStart')
    await e.wait(300)
    const box = await e.surface()
    const clip = { x: box.x, y: box.y, width: box.width, height: Math.min(box.height, 400) }
    const before = await e.page.screenshot({ clip })
    await e.page.keyboard.press('ControlOrMeta+8')
    assert(await e.until(async () => (await e.read('view.state'))?.marks === true), 'Show/Hide ¶ is on')
    assert(await e.until(async () => !(await e.page.screenshot({ clip })).equals(before)), 'the marks are drawn on the page')
    await e.page.keyboard.press('ControlOrMeta+8')
    assert(await e.until(async () => (await e.read('view.state'))?.marks === false), 'and off again')
  }],
  ['h5-table-design-menus', async (e) => {
    await plain(e)
    await e.agent([{ command: 'select.range', params: { anchor: at([1], 0), focus: at([1], 0) } }, { command: 'table.insert', params: { rows: 2, cols: 2 } }])
    await e.until(async () => (await e.ask('file.paragraph', { path: [1, 0, 0, 0] }))?.paragraph != null)
    await e.setup('caret.set', { pos: at([1, 0, 0, 0]) })
    await e.until(async () => (await e.kit.getByTestId('ribbon-tab-tableDesign').count()) > 0)
    await e.kit.getByTestId('ribbon-tab-tableDesign').click()
    await e.wait(250)
    await e.kit.locator('.kui-line-inline').getByTestId('doc-table-shading').click()
    const menu = e.kit.locator('.kui-popover').first()
    assert(await e.until(() => menu.isVisible(), 30), 'Shading is a menu')
    await menu.locator('button').nth(2).click()
    assert(await e.until(async () => (await e.read('table.properties'))?.cell?.shading != null), 'a colour chosen shades the cell')
    await e.kit.locator('.kui-line-inline').getByTestId('doc-table-borders').click()
    assert(await e.until(async () => (await e.kit.locator('.kui-popover [role=menuitem], .kui-popover button').count()) >= 8, 30), 'Borders is a menu of the kinds')
    await e.page.keyboard.press('Escape')
  }],
  ['v1-read-mode', async (e) => {
    await e.kit.getByTestId('ribbon-tab-view').click()
    await e.wait(250)
    await e.kit.locator('.kui-line-inline').getByTestId('doc-cmd-view.readMode').click()
    assert(await e.until(() => e.kit.getByTestId('doc-read-mode').isVisible()), 'Read Mode says so')
    await e.kit.getByTestId('doc-read-mode-exit').click()
    assert(await e.until(async () => (await e.kit.getByTestId('doc-read-mode').count()) === 0), 'and has the way out')
    assert.equal((await e.read('view.state'))?.readMode, false)
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['g6-alignment-pressed', async (e) => {
    await plain(e)
    await e.setup('caret.set', { pos: at([0], 2) })
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.wait(250)
    const row = e.kit.locator('.kui-line-inline')
    for (const id of ['format.alignLeft', 'format.alignCenter', 'format.alignRight', 'format.justify']) assert.equal(await row.getByTestId(`doc-cmd-${id}`).count(), 1, `${id} is in the row`)
    assert(await e.until(async () => (await row.getByTestId('doc-cmd-format.alignLeft').getAttribute('aria-pressed')) === 'true'), 'Align Left is pressed')
    await row.getByTestId('doc-cmd-format.alignCenter').click()
    assert(await e.until(async () => (await row.getByTestId('doc-cmd-format.alignCenter').getAttribute('aria-pressed')) === 'true'), 'Center is pressed after')
    assert.equal(await row.getByTestId('doc-cmd-format.alignLeft').getAttribute('aria-pressed'), 'false', 'and Align Left is not')
  }],
  ['table-properties', async (e) => {
    await plain(e)
    await e.agent([{ command: 'select.range', params: { anchor: at([1], 0), focus: at([1], 0) } }, { command: 'table.insert', params: { rows: 2, cols: 2 } }])
    await e.until(async () => (await e.ask('file.paragraph', { path: [1, 0, 0, 0] }))?.paragraph != null)
    const point = await e.pathPoint([1, 0, 0, 0])
    await e.page.mouse.click(point.x, point.y, { button: 'right' })
    const row = e.kit.getByTestId('doc-menu-table-properties')
    assert(await e.until(() => row.isVisible(), 30), 'the cell menu has Table Properties…')
    await row.click()
    await shown(e, 'doc-table-properties')
    await e.until(async () => (await e.kit.getByTestId('doc-table-properties-align').count()) > 0)
    const undo = (await e.details()).canUndo
    await e.kit.getByTestId('doc-table-properties-align').selectOption('center')
    await e.kit.getByTestId('doc-table-properties-ok').click()
    await gone(e, 'doc-table-properties')
    assert(await e.until(async () => (await e.read('table.properties'))?.align === 'center'), 'the table is centred')
    assert.equal(undo, true)
  }],
  ['word-count', async (e) => {
    await plain(e)
    await e.setup('caret.docStart')
    await e.kit.getByTestId('ribbon-tab-review').click()
    await e.wait(250)
    await e.kit.locator('.kui-line-inline').getByTestId('doc-cmd-file.wordCount').click()
    await shown(e, 'doc-word-count')
    assert(await e.until(async () => (await e.kit.getByTestId('doc-word-count-words').textContent()) === '6'), `the words are counted (${await e.kit.getByTestId('doc-word-count-words').textContent()})`)
    await e.kit.getByTestId('doc-word-count-ok').click()
    await gone(e, 'doc-word-count')
    await e.kit.getByTestId('ribbon-tab-home').click()
  }],
  ['zh-dialog-labels', async () => {
    const zh = await open({ which: process.argv[2] ?? 'chromium', query: 'locale=zh-CN' })
    try {
      await plain(zh)
      await selectBeta(zh)
      for (const [key, id] of [['ControlOrMeta+d', 'doc-font-dialog'], ['ControlOrMeta+Alt+KeyM', 'doc-paragraph-dialog'], ['ControlOrMeta+k', 'doc-link-dialog']]) {
        await zh.page.keyboard.press(key)
        await shown(zh, id)
        await zh.until(async () => (await dialog(zh, id).locator('.doc-dialog-row').count()) > 0)
        const labels = await dialog(zh, id).locator('.doc-dialog-row > span:first-child, .doc-dialog-heading, .doc-dialog-choice, h2, button').allTextContents()
        const english = labels.map((label) => label.trim()).filter((label) => label && /[A-Za-z]{2,}/.test(label) && !/[一-鿿]/.test(label))
        assert.deepEqual(english, [], `${id}: no English field names in Chinese (${labels.join(' / ')})`)
        await zh.page.keyboard.press('Escape')
        await gone(zh, id)
      }
    } finally {
      await zh.close()
    }
  }],
]

await segments('dialogs', list, (which) => open({ which }))
