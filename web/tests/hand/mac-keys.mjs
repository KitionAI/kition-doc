// A Mac's keys (plan "Doc usable by hand", stage 0B, section 2 rows 8, 9, 17; walkthrough C3,
// C4, C5; WPS W-D3–W-D5, W-D8, W-L4), on a Mac's keyboard on any machine (the platform the editor
// reads is a Mac's): Cmd+Left and Cmd+Right go to the ends of the line, Shift selecting; Option
// moves by words; Cmd+Up stays by paragraphs; Cmd+Page Down goes to the start of the next page;
// Cmd+] and Cmd+[ go through the size list.
// node tests/hand/mac-keys.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const LONG = 'The quick brown fox jumps over the lazy dog while the river runs past the old mill and the bells ring out across the valley below. '

/** Setup: three long paragraphs (each several lines) from the agent. */
async function longParagraphs(e) {
  const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
  for (let index = 0; index < 3; index += 1) {
    if (index) commands.push({ command: 'text.newParagraph', params: {} })
    commands.push({ command: 'text.insert', params: { text: LONG.repeat(3).trim() } })
  }
  await e.agent(commands)
  await e.until(async () => (await e.paragraphs()).length === 3)
}
const caretAt = async (e, index, off) => {
  await e.setup('caret.set', { pos: { story: 'body', path: [index], off } })
  await e.wait(150)
}
const focus = async (e) => (await e.selection()).focus

const list = [
  ['c3-cmd-arrows-mac', async (e) => {
    await longParagraphs(e)
    // Where Home and End take the caret on the paragraph's second line.
    await caretAt(e, 1, 200)
    await e.page.keyboard.press('End')
    await e.until(async () => (await focus(e)).off !== 200)
    const end = (await focus(e)).off
    await caretAt(e, 1, 200)
    await e.page.keyboard.press('Home')
    await e.until(async () => (await focus(e)).off !== 200)
    const home = (await focus(e)).off
    assert(home < 200 && end > 200 && home > 0, `the second line is ${home}–${end}`)

    await caretAt(e, 1, 200)
    await e.page.keyboard.press('Meta+ArrowRight')
    await e.until(async () => (await focus(e)).off !== 200)
    assert.equal((await focus(e)).off, end, 'Cmd+Right goes to the end of the line')
    await e.page.keyboard.press('Meta+ArrowLeft')
    await e.until(async () => (await focus(e)).off !== end)
    assert.equal((await focus(e)).off, home, 'Cmd+Left to its start')
    await caretAt(e, 1, 200)
    await e.page.keyboard.press('Meta+Shift+ArrowRight')
    await e.until(async () => ((await e.selection()).text?.length ?? 0) > 0)
    const text = (await e.paragraph(1)).paragraph.text
    assert.equal((await e.selection()).text, text.slice(200, end), 'Shift+Cmd+Right selects to the end of the line')
    await e.page.keyboard.press('ArrowLeft')

    // Option moves by words; Cmd+Up stays by paragraphs (WPS W-D4).
    await caretAt(e, 1, 4)
    await e.page.keyboard.press('Alt+ArrowRight')
    await e.until(async () => (await focus(e)).off !== 4)
    const word = text.indexOf(' ', 5)
    assert([word, word + 1].includes((await focus(e)).off), `Option+Right goes to the end of the word (${(await focus(e)).off})`)
    await e.page.keyboard.press('Meta+ArrowUp')
    await e.until(async () => (await focus(e)).off === 0)
    assert.deepEqual([(await focus(e)).path[0], (await focus(e)).off], [1, 0], 'Cmd+Up goes to the start of the paragraph')
  }],
  ['cmd-page-down', async (e) => {
    await e.pagesOf(3)
    await caretAt(e, 1, 3)
    await e.page.keyboard.press('Meta+PageDown')
    await e.until(async () => (await e.details()).page === 1)
    // The text from the caret on (a page break is a character: page 2 starts after it).
    const from = async () => {
      const at = await focus(e)
      const text = (await e.paragraph(at.path[0])).paragraph.text
      return Buffer.from(text, 'utf8').subarray(at.off).toString('utf8')
    }
    assert((await from()).startsWith('Page 2'), `Cmd+Page Down goes to the start of the next page's first line (${JSON.stringify(await from())})`)
    await e.page.keyboard.press('Meta+PageUp')
    await e.until(async () => (await e.details()).page === 0)
    assert((await from()).startsWith('Page 1'), `Cmd+Page Up to the start of the page before (${JSON.stringify(await from())})`)
  }],
  ['w-l4-size-list', async (e) => {
    await longParagraphs(e)
    await caretAt(e, 0, 4)
    await e.page.keyboard.press('Alt+Shift+ArrowRight')
    await e.until(async () => ((await e.selection()).text?.length ?? 0) > 0)
    const size = async () => (await e.read('format.state'))?.size
    const before = await size()
    const SIZES = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72]
    const next = SIZES.find((value) => value > before)
    await e.page.keyboard.press('Meta+BracketRight')
    await e.until(async () => (await size()) !== before)
    assert.equal(await size(), next, `Cmd+] goes to the next size in the list (${before} → ${next})`)
    await e.page.keyboard.press('Meta+BracketLeft')
    await e.until(async () => (await size()) === before)
    assert.equal(await size(), before, 'Cmd+[ back')
    await e.page.keyboard.press('Meta+Shift+Period')
    await e.until(async () => (await size()) !== before)
    assert.equal(await size(), next, 'Cmd+Shift+> the same')
    await e.page.keyboard.press('Meta+Shift+Comma')
    await e.until(async () => (await size()) === before)
    assert.equal(await size(), before, 'Cmd+Shift+< back')
  }],
]

await segments('mac-keys', list, (which) => open({ which, mac: true }))
