// The system clipboard by hand (plan "Doc usable by hand", stage 0A, section 2 row 14; walkthrough
// J1, J2, J3, J4; WPS W-J1, W-J4–W-J6): Cmd+C and Cmd+X put the selected text on the system
// clipboard, Cmd+V pastes what another application put there (plain text, lines as paragraphs, HTML
// with its bold and its list), Cmd+Alt+Shift+V pastes the text only, and a copy inside the document
// keeps its formatting; the copy's HTML goes on the system clipboard beside its text (stage 0B).
// Chromium reads and writes the system clipboard; WebKit, which gives the test
// no clipboard permission, checks that copying and pasting inside the document still works.
// node tests/hand/clipboard.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

/** Setup: the person's caret at character `ch` of body paragraph `index`. */
const caretAt = async (e, index, ch = 0) => {
  const text = (await e.paragraph(index))?.paragraph?.text ?? ''
  const off = Buffer.from([...text].slice(0, ch).join(''), 'utf8').length
  await e.setup('caret.set', { pos: { story: 'body', path: [index], off } })
  await e.wait(200)
}
/** Selects `count` characters to the right with Shift+→ (a person's keys). */
const selectRight = async (e, count) => {
  await e.page.keyboard.down('Shift')
  for (let index = 0; index < count; index += 1) await e.page.keyboard.press('ArrowRight')
  await e.page.keyboard.up('Shift')
  await e.until(async () => [...((await e.selection()).text ?? '')].length === count)
}
/** Puts `text` (and `html`) on the system clipboard, as another application would (chromium). */
const outside = (e, text, html = null) =>
  e.page.evaluate(async ([text, html]) => {
    const items = { 'text/plain': new Blob([text], { type: 'text/plain' }) }
    if (html) items['text/html'] = new Blob([html], { type: 'text/html' })
    await navigator.clipboard.write([new ClipboardItem(items)])
  }, [text, html])
const body = async (e) => (await e.paragraphs()).join('\n')
/** Undoes until the body is `text` again. */
const undoTo = async (e, text) => {
  for (let index = 0; index < 6 && (await body(e)) !== text; index += 1) {
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.wait(250)
  }
  assert.equal(await body(e), text, 'undone')
}

const list = [
  ['j1-system-clipboard', async (e) => {
    const start = await body(e)
    await caretAt(e, 2, 0)
    await selectRight(e, 5)
    const word = (await e.selection()).text
    if (e.which === 'chromium') await outside(e, 'stale')
    await e.page.keyboard.press('ControlOrMeta+c')
    if (e.which === 'chromium') {
      await e.until(async () => (await e.clipboardText()) === word)
      assert.equal(await e.clipboardText(), word, 'Cmd+C puts the selected text on the system clipboard')
    }
    // Cut and paste back: the paragraph is as it was.
    const paragraph = (await e.paragraph(2)).paragraph.text
    await e.page.keyboard.press('ControlOrMeta+x')
    await e.until(async () => (await e.paragraph(2)).paragraph.text !== paragraph)
    assert.equal((await e.paragraph(2)).paragraph.text, paragraph.slice(word.length), 'Cmd+X takes the text out')
    if (e.which === 'chromium') assert.equal(await e.clipboardText(), word, 'and puts it on the system clipboard')
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await e.paragraph(2)).paragraph.text === paragraph)
    assert.equal((await e.paragraph(2)).paragraph.text, paragraph, 'Cmd+V puts it back')
    await undoTo(e, start)
    // Nothing selected: Cmd+C leaves the system clipboard as it was.
    if (e.which === 'chromium') {
      await outside(e, 'kept')
      await caretAt(e, 2, 3)
      await e.page.keyboard.press('ControlOrMeta+c')
      await e.wait(300)
      assert.equal(await e.clipboardText(), 'kept', 'Cmd+C with nothing selected keeps the clipboard')
    }
  }],
  ['j3-copy-keeps-bold', async (e) => {
    const start = await body(e)
    await caretAt(e, 2, 0)
    await selectRight(e, 5)
    await e.page.keyboard.press('ControlOrMeta+b')
    await e.until(async () => (await e.read('format.state'))?.bold === true)
    await e.page.keyboard.press('ControlOrMeta+c')
    await e.wait(300)
    const end = [...(await e.paragraph(3)).paragraph.text].length
    await caretAt(e, 3, end)
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => [...(await e.paragraph(3)).paragraph.text].length > end)
    await e.setup('select.range', { anchor: { story: 'body', path: [3], off: Buffer.from((await e.paragraph(3)).paragraph.text, 'utf8').length - 2 }, focus: { story: 'body', path: [3], off: Buffer.from((await e.paragraph(3)).paragraph.text, 'utf8').length - 1 } })
    await e.until(async () => (await e.read('format.state'))?.bold === true, 30)
    assert.equal((await e.read('format.state'))?.bold, true, 'a bold word copied inside the document pastes bold')
    await undoTo(e, start)
    // The bold too.
    const bold = async () => ((await e.paragraph(2)).paragraph.runs ?? []).some((run) => run.props?.bold)
    for (let index = 0; index < 4 && (await bold()); index += 1) {
      await e.page.keyboard.press('ControlOrMeta+z')
      await e.wait(250)
    }
    assert(!(await bold()), 'the bold undone')
  }],
  ['j1-copy-html', async (e) => {
    // Stage 0B: Cmd+C puts the copy's HTML on the system clipboard too, so another application
    // keeps the bold; pasted back here it is still the document's own copy.
    if (e.which !== 'chromium') return
    const start = await body(e)
    await caretAt(e, 2, 0)
    await selectRight(e, 5)
    await e.page.keyboard.press('ControlOrMeta+b')
    await e.until(async () => (await e.read('format.state'))?.bold === true)
    await e.page.keyboard.press('ControlOrMeta+c')
    await e.wait(300)
    const html = await e.page.evaluate(async () => {
      const item = (await navigator.clipboard.read()).find((entry) => entry.types.includes('text/html'))
      return item ? (await item.getType('text/html')).text() : ''
    })
    const word = [...(await e.paragraph(2)).paragraph.text].slice(0, 5).join('')
    assert(html.includes(word.trim()) && /<(b|strong)\b|font-weight:\s*(bold|[6-9]00)/i.test(html), `the clipboard's HTML has the bold word: ${html.replace(/style="[^"]*"/g, "").slice(0, 400)}`)
    assert.equal((await e.clipboardText()).replace(/\r\n/g, '\n'), word, 'and its text')
    const end = [...(await e.paragraph(3)).paragraph.text].length
    await caretAt(e, 3, end)
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await e.paragraph(3)).paragraph.text.endsWith(word))
    assert((await e.paragraph(3)).paragraph.text.endsWith(word), 'Cmd+V pastes it back as one word, no new paragraph')
    await undoTo(e, start)
    const bold = async () => ((await e.paragraph(2)).paragraph.runs ?? []).some((run) => run.props?.bold)
    for (let index = 0; index < 4 && (await bold()); index += 1) {
      await e.page.keyboard.press('ControlOrMeta+z')
      await e.wait(250)
    }
  }],
  ['j1-copy-picture', async (e) => {
    // A picture has no text: the system clipboard empties, so Cmd+V pastes the picture, not what
    // was on the clipboard before.
    const start = await body(e)
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEklEQVR4nGP4z8CAFWEXHbQSACj/P8Fu7N9hAAAAAElFTkSuQmCC'
    await caretAt(e, 3, 0)
    await e.setup('image.insert', { data: png })
    const objects = async (index) => (await e.paragraph(index)).paragraph.objects?.length ?? 0
    await e.until(async () => (await objects(3)) === 1)
    if (e.which === 'chromium') await outside(e, 'stale')
    await caretAt(e, 3, 0)
    await selectRight(e, 1)
    await e.page.keyboard.press('ControlOrMeta+c')
    await e.wait(300)
    if (e.which === 'chromium') assert.equal(await e.clipboardText(), '', 'copying the picture empties the system clipboard')
    const before = await objects(2)
    await caretAt(e, 2, 0)
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await objects(2)) === before + 1)
    assert.equal(await objects(2), before + 1, 'Cmd+V pastes the picture')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await objects(2)) === before)
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await objects(3)) === 0)
    assert.equal(await body(e), start)
  }],
  ['j2-paste-from-outside', async (e) => {
    if (e.which !== 'chromium') return
    const start = await body(e)
    const count = (await e.paragraphs()).length
    await caretAt(e, 2, 0)
    await outside(e, 'From elsewhere. ')
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await e.paragraph(2)).paragraph.text.startsWith('From elsewhere. '))
    assert((await e.paragraph(2)).paragraph.text.startsWith('From elsewhere. '), 'plain text from another application pastes')
    await undoTo(e, start)

    // Two lines: two paragraphs.
    await caretAt(e, 2, 0)
    await outside(e, 'First line\nSecond line\n')
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await e.paragraphs()).length === count + 2)
    const lines = await e.paragraphs()
    assert.equal(lines[2], 'First line', 'the first line is a paragraph')
    assert.equal(lines[3], 'Second line', 'the second line is the next')
    await undoTo(e, start)

    // HTML keeps its bold and its list.
    await caretAt(e, 2, 0)
    await outside(e, 'Strong word\nitem one\nitem two', '<p><b>Strong</b> word</p><ul><li>item one</li><li>item two</li></ul>')
    await e.page.keyboard.press('ControlOrMeta+v')
    await e.until(async () => (await e.paragraphs()).some((text) => text === 'item two'))
    const pasted = await e.paragraphs()
    const at = pasted.indexOf('Strong word')
    assert(at >= 0, `the HTML's paragraph is there (${JSON.stringify(pasted.slice(0, 6))})`)
    const runs = (await e.paragraph(at)).paragraph.runs ?? []
    assert(runs.some((run) => run.props?.bold), `"Strong" is bold (${JSON.stringify(runs)})`)
    const item = pasted.indexOf('item one')
    assert((await e.paragraph(item)).paragraph.props?.numbering, 'the list items are a list')
    await undoTo(e, start)

    // Keep Text Only: Cmd+Alt+Shift+V pastes the HTML's text without its bold.
    await caretAt(e, 2, 0)
    await outside(e, 'Plain only', '<b>Plain only</b>')
    await e.page.keyboard.press('ControlOrMeta+Alt+Shift+v')
    await e.until(async () => (await e.paragraph(2)).paragraph.text.startsWith('Plain only'))
    const plain = (await e.paragraph(2)).paragraph.runs ?? []
    assert(!plain[0]?.props?.bold, `Keep Text Only drops the bold (${JSON.stringify(plain[0])})`)
    await undoTo(e, start)

    // The ribbon's Paste reads the system clipboard too.
    await caretAt(e, 2, 0)
    await outside(e, 'From the button. ')
    await e.kit.getByTestId('ribbon-tab-home').click()
    await e.kit.getByTestId('doc-paste').click()
    await e.kit.getByTestId('doc-paste-keep').click()
    await e.until(async () => (await e.paragraph(2)).paragraph.text.startsWith('From the button. '))
    assert((await e.paragraph(2)).paragraph.text.startsWith('From the button. '), 'Home › Paste pastes the system clipboard')
    await undoTo(e, start)
  }],
]

await segments('clipboard', list, (which) => open({ which }))
