// Visible scrollbars follow the document viewport without changing the file.
import './platform.mjs'
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ root: new URL('..', import.meta.url).pathname, configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 700 } })
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60_000 })
  const kit = page.frameLocator('#kit')
  const vertical = kit.getByRole('scrollbar', { name: 'Vertical document scroll' })
  await vertical.waitFor({ timeout: 5000 })
  const value = async () => Number(await vertical.getAttribute('aria-valuenow'))
  const maximum = async () => Number(await vertical.getAttribute('aria-valuemax'))
  const until = async (predicate) => {
    for (let i = 0; i < 40; i += 1) {
      if (await predicate()) return
      await page.waitForTimeout(100)
    }
    assert.fail('scrollbar did not reach the requested position')
  }
  await until(async () => (await maximum()) > 0)
  assert.equal(await value(), 0, 'opens at the top')
  await vertical.press('End')
  await until(async () => Math.abs((await value()) - (await maximum())) < 1)
  await vertical.press('Home')
  await until(async () => (await value()) < 1)
  await kit.getByTestId('kit-canvas-surface').hover()
  await page.mouse.wheel(0, 200)
  await until(async () => (await value()) > 50)
  const thumb = vertical.locator('.kui-scrollbar-thumb')
  const box = await thumb.boundingBox()
  const track = await vertical.boundingBox()
  assert(box && track, 'scroll track and thumb have visible geometry')
  assert(track.width >= 10 && box.height < track.height, 'a grabbable thumb reflects the visible fraction')
  const sink = kit.locator('.kui-text-sink')
  await sink.evaluate((element) => element.focus())
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width / 2, track.y + track.height + box.height, { steps: 8 })
  await page.mouse.up()
  assert(await sink.evaluate((element) => element.ownerDocument.activeElement === element), 'dragging preserves typing focus')
  await until(async () => Math.abs((await value()) - (await maximum())) < 1)
  await vertical.press('Home')
  await until(async () => (await value()) < 1)
  await kit.getByTestId('ribbon-tab-view').click()
  await kit.getByTestId('doc-zoom-100').click()
  await page.setViewportSize({ width: 600, height: 700 })
  const horizontal = kit.getByRole('scrollbar', { name: 'Horizontal document scroll' })
  await horizontal.waitFor()
  await horizontal.press('End')
  await until(async () => Number(await horizontal.getAttribute('aria-valuenow')) > 0)
  assert.equal(await kit.getByTestId('doc-unsaved').count(), 0, 'scrollbar actions never edit the document')
  await page.screenshot({ path: new URL(`../../target/scrollbar-${which}.png`, import.meta.url).pathname })
  console.log(`PASS ${which}: visible scrollbars follow wheel, keys, dragging and zoom`)
} finally {
  await browser.close()
  await server.close()
}
