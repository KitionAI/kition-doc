// The editor's own keys (plan "Doc usable by hand", stage 0A, section 2 rows 13, 30; walkthrough
// Q2, Q4, G13, G11, I2; WPS W-R1–W-R3, W-I1, W-D10): the find bar's Enter and Shift+Enter, Cmd+G,
// F3 and Cmd+Shift+G (opening the bar when it is closed), Match Case, Ctrl+H for Replace, Cmd+8 for
// Show/Hide ¶, Cmd+K and F5 opening their parameters, Ctrl+A selecting all, Cmd+Y redoing,
// Cmd+Shift+C and Cmd+Shift+V copying and pasting the formatting.
// node tests/hand/keys.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const caretAt = async (e, index, off = 0) => {
  await e.setup('caret.set', { pos: { story: 'body', path: [index], off } })
  await e.wait(200)
}
/** Where the selection starts: paragraph and byte offset. */
const startOf = async (e) => {
  const { anchor, focus, text } = await e.selection()
  const first = [anchor, focus].sort((a, b) => a.path[0] - b.path[0] || a.off - b.off)[0]
  return { at: `${first.path.join('.')}/${first.off}`, text }
}
const findCount = (e) => e.kit.getByTestId('doc-find-count').textContent()
const closeFind = async (e) => {
  await e.kit.getByTestId('doc-find-text').focus()
  await e.page.keyboard.press('Escape')
  await e.until(async () => (await e.kit.getByTestId('doc-find-bar').count()) === 0, 30)
}

const list = [
  ['q4-find-keys', async (e) => {
    await caretAt(e, 0)
    await e.page.keyboard.press('ControlOrMeta+f')
    await e.until(() => e.kit.getByTestId('doc-find-text').isVisible(), 30)
    await e.page.keyboard.type('the', { delay: 20 })
    await e.until(async () => /^1\/\d+$/.test((await findCount(e)) ?? ''))
    const total = Number((await findCount(e)).split('/')[1])
    assert(total >= 3, `the sample has several matches (${total})`)
    const first = await startOf(e)
    await e.page.keyboard.press('Enter')
    await e.until(async () => (await startOf(e)).at !== first.at)
    const second = await startOf(e)
    assert.notEqual(second.at, first.at, 'Enter finds the next match')
    await e.page.keyboard.press('Shift+Enter')
    await e.until(async () => (await startOf(e)).at === first.at)
    assert.equal((await startOf(e)).at, first.at, 'Shift+Enter the previous one')
    await closeFind(e)

    // The bar closed: Cmd+G opens it on the same text and goes to the next match.
    await e.page.keyboard.press('ControlOrMeta+g')
    await e.until(() => e.kit.getByTestId('doc-find-bar').isVisible(), 30)
    await e.until(async () => (await startOf(e)).at === second.at)
    assert.equal(await e.kit.getByTestId('doc-find-text').inputValue(), 'the', 'Cmd+G opens the bar on the text searched for')
    assert.equal((await startOf(e)).at, second.at, 'and goes to the next match')
    await e.page.keyboard.press('ControlOrMeta+Shift+g')
    await e.until(async () => (await startOf(e)).at === first.at)
    assert.equal((await startOf(e)).at, first.at, 'Cmd+Shift+G goes back')
    await e.page.keyboard.press('F3')
    await e.until(async () => (await startOf(e)).at === second.at)
    assert.equal((await startOf(e)).at, second.at, 'F3 goes on')
    await closeFind(e)
  }],
  ['q2-match-case', async (e) => {
    await caretAt(e, 0)
    await e.page.keyboard.press('ControlOrMeta+f')
    const field = e.kit.getByTestId('doc-find-text')
    await e.until(() => field.isVisible(), 30)
    await field.fill('THE')
    await e.until(async () => /\/\d+$/.test((await findCount(e)) ?? ''))
    const any = Number((await findCount(e)).split('/')[1])
    await e.kit.getByTestId('doc-find-case').click()
    await e.until(async () => Number((await findCount(e))?.split('/')[1]) !== any)
    assert.equal(await e.kit.getByTestId('doc-find-case').getAttribute('aria-pressed'), 'true', 'Match Case is on')
    assert(Number((await findCount(e)).split('/')[1]) < any, `Match Case finds fewer (${any} → ${await findCount(e)})`)
    await e.kit.getByTestId('doc-find-case').click()
    await e.until(async () => Number((await findCount(e))?.split('/')[1]) === any)
    await field.fill('he')
    await e.until(async () => /\/\d+$/.test((await findCount(e)) ?? ''))
    const parts = Number((await findCount(e)).split('/')[1])
    await e.kit.getByTestId('doc-find-word').click()
    await e.until(async () => Number((await findCount(e))?.split('/')[1]) !== parts)
    assert(Number((await findCount(e)).split('/')[1]) < parts, `Whole Words finds fewer (${parts} → ${await findCount(e)})`)
    await e.kit.getByTestId('doc-find-word').click()
    await closeFind(e)
  }],
  ['ctrl-h-replace', async (e) => {
    await caretAt(e, 0)
    await e.page.keyboard.press('Control+h')
    await e.until(() => e.kit.getByTestId('doc-replace-text').isVisible(), 30)
    assert(await e.kit.getByTestId('doc-replace-text').isVisible(), 'Ctrl+H opens the bar on Replace')
    await closeFind(e)
  }],
  ['g13-show-marks', async (e) => {
    await caretAt(e, 0)
    const marks = async () => (await e.read('view.state'))?.marks === true
    const before = await marks()
    await e.page.keyboard.press('ControlOrMeta+8')
    await e.until(async () => (await marks()) !== before)
    assert.equal(await marks(), !before, 'Cmd+8 turns Show/Hide ¶ over')
    await e.page.keyboard.press('ControlOrMeta+8')
    await e.until(async () => (await marks()) === before)
    assert.equal(await marks(), before, 'and back')
  }],
  ['dialog-keys', async (e) => {
    const dialog = e.kit.locator('[role=dialog]')
    for (const [key, name] of [['ControlOrMeta+k', 'Cmd+K (Link)'], ['F5', 'F5 (Go To)']]) {
      await caretAt(e, 0)
      await e.page.keyboard.press(key)
      await e.until(() => dialog.first().isVisible(), 30)
      assert(await dialog.first().isVisible(), `${name} opens its dialog`)
      await e.page.keyboard.press('Escape')
      await e.until(async () => (await dialog.count()) === 0, 30)
      await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
      assert.equal(await e.active(), 'textarea.kui-text-sink', `the text has the keyboard after ${name}'s dialog`)
    }
  }],
  ['ctrl-a-select-all', async (e) => {
    await caretAt(e, 0)
    await e.page.keyboard.press('Control+a')
    await e.until(async () => ((await e.selection()).text?.length ?? 0) > 100)
    const all = (await e.paragraphs()).filter(Boolean).length
    assert(((await e.selection()).text?.length ?? 0) > 100 && all > 2, 'Ctrl+A selects the whole document')
    await e.page.keyboard.press('ArrowLeft')
  }],
  ['i2-redo-keys', async (e) => {
    await caretAt(e, 2)
    const before = (await e.paragraph(2)).paragraph.text
    await e.page.keyboard.type('Q', { delay: 20 })
    await e.until(async () => (await e.paragraph(2)).paragraph.text === `Q${before}`)
    for (const redo of ['ControlOrMeta+y', 'ControlOrMeta+Shift+z']) {
      await e.page.keyboard.press('ControlOrMeta+z')
      await e.until(async () => (await e.paragraph(2)).paragraph.text === before)
      assert.equal((await e.paragraph(2)).paragraph.text, before, 'Cmd+Z undoes')
      await e.page.keyboard.press(redo)
      await e.until(async () => (await e.paragraph(2)).paragraph.text === `Q${before}`)
      assert.equal((await e.paragraph(2)).paragraph.text, `Q${before}`, `${redo} redoes`)
    }
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraph(2)).paragraph.text === before)
  }],
  ['format-copy-keys', async (e) => {
    // Cmd+Shift+C copies the formatting, Cmd+Shift+V pastes it (not the system clipboard's text).
    const select = async (index) => {
      await caretAt(e, index)
      await e.page.keyboard.down('Shift')
      for (let step = 0; step < 4; step += 1) await e.page.keyboard.press('ArrowRight')
      await e.page.keyboard.up('Shift')
      await e.until(async () => (await e.selection()).text?.length === 4)
    }
    const text = (await e.paragraphs()).join('\n')
    await select(2)
    await e.page.keyboard.press('ControlOrMeta+b')
    await e.until(async () => (await e.read('format.state'))?.bold === true)
    await e.page.keyboard.press('ControlOrMeta+Shift+c')
    await e.wait(200)
    await select(3)
    assert.notEqual((await e.read('format.state'))?.bold, true, 'paragraph 4 starts plain')
    await e.page.keyboard.press('ControlOrMeta+Shift+v')
    await e.until(async () => (await e.read('format.state'))?.bold === true)
    assert.equal((await e.read('format.state'))?.bold, true, 'Cmd+Shift+V pastes the bold')
    assert.equal((await e.paragraphs()).join('\n'), text, 'and no text')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.page.keyboard.press('ControlOrMeta+z')
  }],
]

await segments('keys', list, (which) => open({ which }))
