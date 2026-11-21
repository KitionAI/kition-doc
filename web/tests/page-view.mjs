// The initial view keeps text readable on wide screens and shows margin guides as overlays.
import './platform.mjs'
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
try {
  for (const width of [2048, 800]) {
    const page = await browser.newPage({ viewport: { width, height: 1140 } })
    await page.goto(`${origin}/dev.html?locale=en-US`)
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60_000 })
    const kit = page.frameLocator('#kit')
    await kit.getByTestId('kit-canvas-surface').waitFor()
    const zoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))
    assert(zoom > 50 && zoom <= 134, `the initial zoom stays readable at ${width}px, got ${zoom}%`)
    if (width === 2048) assert.equal(zoom, 133, 'points open at 96 CSS pixels per inch on wide screens')
    await page.screenshot({ path: new URL(`../../target/page-view-${which}-${width}.png`, import.meta.url).pathname })
    if (width === 2048) {
      await kit.getByTestId('ribbon-tab-view').click()
      await kit.getByTestId('doc-zoom-100').click()
      assert.equal(await kit.getByTestId('kit-zoom').textContent(), '100%', 'the explicit zoom command remains available')
      await kit.getByTestId('doc-zoom-width').click()
      const pageWidthZoom = Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))
      assert(pageWidthZoom > 134, 'Page Width can still zoom in when explicitly requested')
    }
    assert.equal(await kit.getByTestId('doc-unsaved').count(), 0, 'placing the view does not edit the file')
    await page.close()
  }
  console.log(`PASS ${which}: readable initial page scale on wide and narrow screens`)
} finally {
  await browser.close()
  await server.close()
}
