// The keyboard reaches the text from the first key and keeps reaching it (plan "Doc usable by
// hand", stage 0A; walkthrough K1, K2, the timed task's first stop): a person's keys and clicks only,
// the document read back through the development host. The first segment needs the editor just
// opened, nothing clicked and nothing focused by the test.
// node tests/hand/focus.mjs [chromium|webkit] [segment…]
import assert from 'node:assert/strict'
import { open, segments } from './harness.mjs'

const list = [
  ['k1-first-key', async (e) => {
    const before = (await e.paragraphs())[0]
    await e.page.keyboard.type('Abc', { delay: 30 })
    await e.until(async () => (await e.paragraphs())[0] !== before)
    const after = (await e.paragraphs())[0]
    assert.equal(after, `Abc${before}`, `typing right after the document opened goes in at the caret, the first paragraph's start (${JSON.stringify(after.slice(0, 20))})`)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'the text has the keyboard')
    await e.page.keyboard.press('ArrowRight')
    await e.page.keyboard.type('|')
    await e.until(async () => (await e.paragraphs())[0].startsWith('Abc'))
    await e.until(async () => (await e.paragraphs())[0] === `Abc${before.slice(0, 1)}|${before.slice(1)}`)
    assert.equal((await e.paragraphs())[0], `Abc${before.slice(0, 1)}|${before.slice(1)}`, 'the arrows work too')
    for (let index = 0; index < 5; index += 1) await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraphs())[0] === before)
  }],
  ['k2-focus-after-find', async (e) => {
    const before = (await e.paragraphs())[0]
    await e.page.keyboard.press('ControlOrMeta+f')
    const field = e.kit.getByTestId('doc-find-text')
    await e.until(() => field.isVisible(), 30)
    assert.equal(await e.active(), 'input.kui-field', 'Cmd+F gives the find field the keyboard')
    await e.page.keyboard.type('the', { delay: 20 })
    await e.until(async () => /\/\d+$/.test((await e.kit.getByTestId('doc-find-count').textContent()) ?? ''))
    await e.page.keyboard.press('Escape')
    await e.until(async () => (await e.kit.getByTestId('doc-find-bar').count()) === 0, 30)
    const found = await e.selection()
    assert.equal(found.text?.toLowerCase(), 'the', `the last match stays selected after Escape (${JSON.stringify(found.text)})`)
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'Escape gives the text the keyboard')
    const text = (await e.paragraphs()).join('\n')
    await e.page.keyboard.type('THE', { delay: 20 })
    await e.until(async () => (await e.paragraphs()).join('\n') !== text)
    assert((await e.paragraphs()).join('\n').includes('THE'), 'typing after Escape goes into the document, over the match')
    await e.page.keyboard.press('ControlOrMeta+z')
    await e.until(async () => (await e.paragraphs()).join('\n') === text)

    // The bar's Close button does the same.
    await e.page.keyboard.press('ControlOrMeta+f')
    await e.until(() => field.isVisible(), 30)
    await e.kit.getByTestId('doc-find-close').click()
    await e.until(async () => (await e.active()) === 'textarea.kui-text-sink', 30)
    assert.equal(await e.active(), 'textarea.kui-text-sink', 'Close gives the text the keyboard')
    assert.equal((await e.paragraphs())[0], before)
  }],
]

await segments('focus', list, (which) => open({ which }))
