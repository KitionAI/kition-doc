// Escape in Focus cancels first (KitPointer 4.4), and Focus shows no find bar: with a selection,
// Escape collapses it and stays in Focus; with nothing to cancel, Escape leaves Focus. The find bar
// is the kit's own bar, so Focus hides it and leaving Focus shows it again.
// node tests/focus-escape.mjs [chromium|webkit]
import './platform.mjs'
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ root: new URL('..', import.meta.url).pathname, configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1165, height: 900 } })
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic,focus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const app = kit.locator('.kui-app')
  const mode = () => app.getAttribute('data-mode')
  const sink = kit.locator('.kui-text-sink')
  await sink.evaluate((element) => element.focus())

  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus', 'Cmd+\\ enters Focus')
  // Select All, then Escape: it collapses the selection, so Focus stays.
  await page.keyboard.press('ControlOrMeta+a')
  await page.waitForTimeout(400)
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  assert.equal(await mode(), 'focus', 'an Escape that cancelled a selection stays in Focus')
  await page.waitForTimeout(400)
  // Nothing left to cancel: Escape leaves Focus.
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  assert.equal(await mode(), 'classic', 'an Escape with nothing to cancel leaves Focus')

  // The find bar is the kit's own bar: hidden in Focus, back after it.
  await sink.evaluate((element) => element.focus())
  await page.keyboard.press('ControlOrMeta+f')
  await kit.getByTestId('doc-find-bar').waitFor({ timeout: 5000 })
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus', 'Cmd+\\ enters Focus from the find field')
  assert.equal(await kit.getByTestId('doc-find-bar').count(), 0, 'Focus shows no find bar')
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'classic', 'Cmd+\\ leaves Focus')
  await kit.getByTestId('doc-find-bar').waitFor({ timeout: 5000 })
  // Escape in the find field closes the bar only.
  await kit.getByTestId('doc-find-text').focus()
  await page.keyboard.press('Escape')
  assert.equal(await kit.getByTestId('doc-find-bar').count(), 0, 'Escape closes the find bar')
  console.log(`PASS kit.doc focus escape (${which})`)
} finally {
  await browser.close()
  await server.close()
}
