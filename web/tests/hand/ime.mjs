// Chinese input (plan "Doc usable by hand", stage 3; walkthrough D16; WPS W-V1–W-V7):
// node tests/hand/ime.mjs [chromium|webkit] [segment…]
// The hidden field the input method types into sits where a composition starts, as tall as the
// caret's line, so its candidate window opens under the line (both browsers). In Chromium the
// input method itself is driven through the DevTools protocol (`Input.imeSetComposition`,
// `Input.insertText`): "n", "ni", "ni hao" show in the text at the caret, underlined, the text
// after them moved on, while the document, its undo history and its saved state stay as they were;
// what is committed is one undo step of its own; Enter, Escape, the arrows, Tab and Backspace
// pressed while composing are the input method's; a selection is replaced only by the commit;
// zoomed on another page and in a table's cell the field and the line are at the caret; an agent's
// edit while composing takes the composition away and the commit goes in at the caret. The text is
// read back from the engine; the caret and the underline from what is drawn over the pages.
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const press = async (e, key, ms = 250) => {
  await e.page.keyboard.press(key)
  await e.wait(ms)
}
const undo = (e) => press(e, 'ControlOrMeta+z', 400)

/** Setup: `count` short paragraphs from the agent in place of the sample's text. */
async function paragraphsOf(e, count) {
  const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
  for (let index = 0; index < count; index += 1) {
    if (index) commands.push({ command: 'text.newParagraph', params: {} })
    commands.push({ command: 'text.insert', params: { text: `Paragraph ${index + 1} for the input method.` } })
  }
  await e.agent(commands)
  await e.until(async () => (await e.paragraphs()).length === count)
}

const textOf = async (e, index) => (await e.paragraph(index))?.paragraph?.text
/** Waits until paragraph `index`'s text is `want`; answers the text it has then. */
async function textBecomes(e, index, want) {
  await e.until(async () => (await textOf(e, index)) === want, 40)
  return textOf(e, index)
}

/** The line under the composition drawn now, on the page: from x0 to x1 at y; null when none. */
async function underline(e) {
  const lines = await e.underDrawn()
  if (!lines.length) return null
  return { x0: Math.min(...lines.map((line) => line.x0)), x1: Math.max(...lines.map((line) => line.x1)), y: lines[0].y, lines: lines.length }
}

/** The input method's hidden field on the page, and whether it has the keyboard. */
async function sink(e) {
  const box = await e.kit.locator('textarea.kui-text-sink').boundingBox()
  const focused = await e.frame().evaluate(() => document.activeElement?.classList.contains('kui-text-sink') === true)
  return box ? { x: box.x, y: box.y, height: box.height, focused } : null
}
/** Waits until the hidden field is within `px` of (`x`, `y`); answers where it is. */
async function sinkNear(e, x, y, px = 8) {
  let at = null
  await e.until(async () => {
    at = await sink(e)
    return at != null && Math.abs(at.x - x) <= px && Math.abs(at.y - y) <= px
  }, 40)
  return at
}

/** Chromium's input method, through the DevTools protocol. */
async function inputMethod(e) {
  const cdp = await e.page.context().newCDPSession(e.page)
  return {
    compose: async (text) => {
      await cdp.send('Input.imeSetComposition', { text, selectionStart: text.length, selectionEnd: text.length })
      await e.wait(200)
    },
    commit: async (text) => {
      await cdp.send('Input.insertText', { text })
      await e.wait(250)
    },
    /** A key as the browser sends it while an input method composes (key code 229). */
    key: async (key, code = key) => {
      await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code, windowsVirtualKeyCode: 229, nativeVirtualKeyCode: 229 })
      await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: 229, nativeVirtualKeyCode: 229 })
      await e.wait(150)
    },
  }
}

/** Composes `text` and waits for its line to be drawn longer than `width`; answers the line. */
async function composed(e, ime, text, width = 0) {
  await ime.compose(text)
  let line = null
  await e.until(async () => (line = await underline(e)) != null && line.x1 - line.x0 > width, 40)
  return line
}
const gone = (e) => e.until(async () => (await underline(e)) == null, 40)

/** How many pixels of two PNG screenshots differ clearly. */
const differ = (e, one, two) =>
  e.page.evaluate(
    async ([a, b]) => {
      const pixels = async (data) => {
        const image = new Image()
        image.src = `data:image/png;base64,${data}`
        await image.decode()
        const canvas = document.createElement('canvas')
        canvas.width = image.width
        canvas.height = image.height
        const context = canvas.getContext('2d')
        context.drawImage(image, 0, 0)
        return context.getImageData(0, 0, canvas.width, canvas.height).data
      }
      const [p, q] = [await pixels(a), await pixels(b)]
      let total = 0
      for (let index = 0; index < p.length; index += 4) {
        if (Math.abs(p[index] - q[index]) + Math.abs(p[index + 1] - q[index + 1]) + Math.abs(p[index + 2] - q[index + 2]) > 120) total += 1
      }
      return total
    },
    [one.toString('base64'), two.toString('base64')],
  )

/** The first body paragraph of the sample with this much text at least. */
async function longParagraph(e, length = 30) {
  const list = await e.paragraphs()
  const index = list.findIndex((text) => [...text].length >= length && !text.includes('￼'))
  assert(index >= 0, 'the sample has a long paragraph')
  return index
}

const both = [
  ['d16-sink-at-caret', async (e) => {
    const index = await longParagraph(e)
    const point = await e.textPoint(index, 5)
    assert(point, 'the caret is in view')
    const caret = await e.caretDrawn()
    let at = await sinkNear(e, caret.x, caret.y)
    assert(at && Math.abs(at.x - caret.x) <= 8 && Math.abs(at.y - caret.y) <= 8, `the input method's field sits at the caret (${JSON.stringify(at)} vs ${JSON.stringify(caret)}); it used to sit in the canvas's corner`)
    assert(Math.abs(at.height - caret.height) <= 3, `as tall as the caret's line (${at.height} vs ${caret.height})`)
    assert(at.focused, 'and has the keyboard')
    for (let step = 0; step < 6; step += 1) await press(e, 'ArrowRight', 60)
    await e.wait(300)
    const moved = await e.caretDrawn()
    assert(moved && moved.x > caret.x + 5, 'the caret moved right')
    at = await sinkNear(e, moved.x, moved.y)
    assert(at && Math.abs(at.x - moved.x) <= 8, `the field follows it (${JSON.stringify(at)} vs ${JSON.stringify(moved)})`)
    // Text an input method commits goes in at the caret, one Cmd+Z takes it out.
    const before = await textOf(e, index)
    await e.page.keyboard.insertText('你好')
    const want = [...before].slice(0, 11).join('') + '你好' + [...before].slice(11).join('')
    assert.equal(await textBecomes(e, index, want), want)
    await undo(e)
    assert.equal(await textBecomes(e, index, before), before, 'one Cmd+Z takes it out')
  }],
]

const chromiumOnly = [
  ['d16-ime-composition', async (e) => {
    const ime = await inputMethod(e)
    const index = await longParagraph(e)
    const before = await textOf(e, index)
    await e.textPoint(index, 4)
    const start = await e.caretDrawn()
    assert(start, 'the caret is drawn')
    assert.equal(await e.kit.getByTestId('doc-unsaved').count(), 0, 'the sample opens saved')
    const clip = { x: Math.round(start.x) + 1, y: Math.round(start.y), width: 160, height: Math.max(8, Math.round(start.height)) }
    const plain = await e.page.screenshot({ clip })
    // The first key of a composition is the input method's: it types nothing.
    await ime.key('n', 'KeyN')
    let width = 0
    for (const typed of ['n', 'ni', 'ni hao']) {
      const line = await composed(e, ime, typed, width)
      assert(line && line.x1 - line.x0 > width, `composing “${typed}” draws a longer line under it (${JSON.stringify(line)}; nothing showed while composing)`)
      assert(Math.abs(line.x0 - start.x) <= 2, `from the caret where the composition started (${line.x0} vs ${start.x})`)
      assert(line.y > start.y && line.y <= start.y + start.height + 4, `under the line's text (${line.y} in ${start.y}+${start.height})`)
      width = line.x1 - line.x0
      assert.equal(await textOf(e, index), before, 'the document does not change while composing')
      assert.equal(await e.kit.getByTestId('doc-unsaved').count(), 0, 'nor needs saving')
    }
    const state = await e.details()
    assert.deepEqual(state.preedit, { text: 'ni hao', cursor: 6 }, 'the engine shows the composition')
    assert.equal(state.canUndo, false, 'and no undo step came of it')
    const now = await e.caretDrawn()
    const line = await underline(e)
    assert(now && Math.abs(now.x - line.x1) <= 2, `the caret is at the input method's cursor, the composition's end (${now?.x} vs ${line.x1})`)
    const at = await sinkNear(e, start.x, start.y)
    assert(at && Math.abs(at.x - start.x) <= 8 && Math.abs(at.y - start.y) <= 8, `the field sits where the composition starts, so the candidate window lines up with it (${JSON.stringify(at)} vs ${JSON.stringify(start)})`)
    await e.page.mouse.move(5, 5)
    await e.wait(300)
    const composing = await e.page.screenshot({ clip })
    const changed = await differ(e, plain, composing)
    assert(changed > 60, `the composed letters are on the page and the text after them moved on (${changed} pixels changed)`)
    // The commit: the text, one step of its own.
    await ime.commit('你好')
    const want = [...before].slice(0, 4).join('') + '你好' + [...before].slice(4).join('')
    assert.equal(await textBecomes(e, index, want), want, 'the commit types the chosen text at the caret')
    assert(await gone(e), 'no composition shows after the commit')
    assert.equal((await e.details()).preedit, null)
    await undo(e)
    assert.equal(await textBecomes(e, index, before), before, 'one Cmd+Z takes the commit out')
  }],
  ['d16-ime-keys', async (e) => {
    const ime = await inputMethod(e)
    await paragraphsOf(e, 3)
    await e.textPoint(1, 9)
    await e.page.keyboard.type('ab', { delay: 30 })
    const typed = 'Paragraphab 2 for the input method.'
    assert.equal(await textBecomes(e, 1, typed), typed)
    const caret = (await e.details()).caret
    // Escape while composing is the input method's: it cancels the composition, the text stays.
    await composed(e, ime, 'ni hao')
    await ime.key('Escape')
    await ime.compose('')
    assert(await gone(e), 'Escape while composing takes the composition away')
    assert.equal(await textOf(e, 1), typed, 'and types nothing')
    // Enter, the arrows, Tab and Backspace while composing: no new paragraph, nothing moves or goes.
    await composed(e, ime, 'ni hao')
    for (const key of ['Enter', 'ArrowLeft', 'ArrowUp', 'Tab', 'Backspace', 'Delete']) await ime.key(key)
    await e.wait(300)
    assert.equal((await e.paragraphs()).length, 3, 'no key while composing made a paragraph')
    assert.equal(await textOf(e, 1), typed, 'nor changed the text')
    assert.deepEqual((await e.details()).caret, caret, 'nor moved the caret')
    assert((await underline(e)) != null, 'the composition still shows')
    // Enter commits the letters as they are (WPS W-V4): text, no new paragraph.
    await ime.commit('shijie')
    const lettered = 'Paragraphabshijie 2 for the input method.'
    assert.equal(await textBecomes(e, 1, lettered), lettered)
    assert.equal((await e.paragraphs()).length, 3, 'still three paragraphs')
    // Two commits in a row, then typing: each its own undo step (WPS W-V5).
    await composed(e, ime, 'ni hao')
    await ime.commit('你好')
    await e.page.keyboard.type('cd', { delay: 30 })
    const all = 'Paragraphabshijie你好cd 2 for the input method.'
    assert.equal(await textBecomes(e, 1, all), all)
    for (const want of ['Paragraphabshijie你好 2 for the input method.', lettered, typed, 'Paragraph 2 for the input method.']) {
      await undo(e)
      assert.equal(await textBecomes(e, 1, want), want, 'one Cmd+Z a step: the typing, each commit, the typing before')
    }
  }],
  ['d16-ime-selection', async (e) => {
    const ime = await inputMethod(e)
    await paragraphsOf(e, 2)
    const plain = 'Paragraph 1 for the input method.'
    await e.setup('select.range', { anchor: { story: 'body', path: [0], off: 12 }, focus: { story: 'body', path: [0], off: 15 } })
    await e.until(async () => (await e.selection())?.text === 'for', 30)
    const from = (await e.details()).inputAt
    const start = await e.toScreen(from.x, from.top)
    const line = await composed(e, ime, 'ni')
    assert(line && Math.abs(line.x0 - start.x) <= 2, `the composition shows where the selection starts (${line?.x0} vs ${start.x})`)
    assert.equal(await textOf(e, 0), plain, 'the selected text is still in the document')
    // Escape in the input method: the selection is back.
    await ime.compose('')
    assert(await gone(e))
    assert.equal((await e.selection())?.text, 'for', 'the selection is back')
    // The commit replaces it, one undo step.
    await composed(e, ime, 'ni')
    await ime.commit('你')
    const replaced = 'Paragraph 1 你 the input method.'
    assert.equal(await textBecomes(e, 0, replaced), replaced)
    await undo(e)
    assert.equal(await textBecomes(e, 0, plain), plain, 'one Cmd+Z brings the selected text back')
  }],
  ['d16-ime-zoomed-page2', async (e) => {
    const ime = await inputMethod(e)
    await e.pagesOf(3)
    await e.kit.getByTestId('kit-zoom').click()
    await e.kit.getByTestId('doc-zoom-menu-field').fill('150')
    await e.kit.getByTestId('doc-zoom-menu-apply').click()
    await e.until(async () => (await e.zoom()) === 150)
    assert.equal(await e.zoom(), 150, 'zoomed to 150%')
    // The second page's first line: placed by the engine's caret, brought into view by a key.
    const index = (await e.paragraphs()).findIndex((text) => text === 'Line 1 of page 2.')
    assert(index > 0, 'a paragraph on page 2')
    await e.setup('caret.set', { pos: { story: 'body', path: [index], off: 4 } })
    await press(e, 'ArrowRight', 400)
    await e.until(() => e.caretInView(), 30)
    assert.equal((await e.details()).page, 1, 'the caret is on page 2')
    const caret = await e.caretDrawn()
    assert(caret, 'the caret is drawn')
    const line = await composed(e, ime, 'zhong')
    assert(line && Math.abs(line.x0 - caret.x) <= 2, `zoomed on page 2 the composition is at the caret (${line?.x0} vs ${caret.x})`)
    const at = await sinkNear(e, caret.x, caret.y)
    assert(at && Math.abs(at.x - caret.x) <= 8 && Math.abs(at.y - caret.y) <= 8, `and the field too (${JSON.stringify(at)} vs ${JSON.stringify(caret)})`)
    await ime.commit('中')
    assert.equal(await textBecomes(e, index, 'Line 中1 of page 2.'), 'Line 中1 of page 2.')
  }],
  ['d16-ime-table-cell', async (e) => {
    const ime = await inputMethod(e)
    await paragraphsOf(e, 2)
    await e.agent([{ command: 'caret.docEnd', params: {} }, { command: 'table.insert', params: { rows: 2, cols: 2 } }])
    let table = -1
    await e.until(async () => (table = ((await e.ask('file.inspect', {})).blocks ?? []).findIndex((block) => block.type === 'table')) >= 0)
    assert(table >= 0, 'a table')
    await e.setup('caret.set', { pos: { story: 'body', path: [table, 0, 1, 0], off: 0 } })
    await e.wait(400)
    const rect = (await e.details()).caret
    const caret = await e.toScreen(rect.x, rect.top)
    const line = await composed(e, ime, 'ni hao')
    assert(line && Math.abs(line.x0 - caret.x) <= 2, `in a cell the composition is at the caret (${line?.x0} vs ${caret.x})`)
    const at = await sinkNear(e, caret.x, caret.y)
    assert(at, `and the field (${JSON.stringify(at)} vs ${JSON.stringify(caret)})`)
    await ime.commit('你好')
    await e.until(async () => (await e.ask('file.paragraph', { path: [table, 0, 1, 0] }))?.paragraph?.text === '你好', 40)
    assert.equal((await e.ask('file.paragraph', { path: [table, 0, 1, 0] }))?.paragraph?.text, '你好', 'the commit is in the cell')
  }],
  ['d16-ime-other-edit', async (e) => {
    const ime = await inputMethod(e)
    await paragraphsOf(e, 3)
    await e.textPoint(2, 9)
    await composed(e, ime, 'ni hao')
    // An agent edits the paragraph before while the person composes: the composition goes.
    await e.agent([
      { command: 'select.range', params: { anchor: { story: 'body', path: [0], off: 0 }, focus: { story: 'body', path: [0], off: 0 } } },
      { command: 'text.insert', params: { text: 'Agent ' } },
    ])
    await e.until(async () => (await textOf(e, 0))?.startsWith('Agent '), 40)
    assert(await gone(e), 'the composition the document moved on from goes')
    // What the input method commits then goes in at the person's caret.
    await ime.commit('你好')
    const want = 'Paragraph你好 3 for the input method.'
    assert.equal(await textBecomes(e, 2, want), want)
  }],
]

const which = process.argv[2] ?? 'chromium'
// The composition first: it checks the sample opens with nothing to save.
await segments('ime', which === 'chromium' ? [chromiumOnly[0], ...both, ...chromiumOnly.slice(1)] : both, (browser) => open({ which: browser }))
