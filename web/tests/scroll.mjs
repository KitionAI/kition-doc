// Scrolling stays within the document's pages in both the desktop and browser engines.
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
try {
  await page.goto(`${origin}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60_000 })
  const surface = page.frameLocator('#kit').getByTestId('kit-canvas-surface')
  const paper = () => surface.evaluate((canvas) => {
    const pixels = canvas.getContext('2d').getImageData(Math.floor(canvas.width / 2), 0, 1, canvas.height).data
    const rows = []
    for (let y = 0; y < canvas.height; y += 1) {
      if (pixels[y * 4] === 255 && pixels[y * 4 + 1] === 255 && pixels[y * 4 + 2] === 255) rows.push(y)
    }
    return { top: (rows[0] ?? canvas.height) / canvas.height, coverage: rows.length / canvas.height }
  })
  await surface.hover()
  for (const delta of [-100_000, 100_000, -100_000]) {
    await page.mouse.wheel(0, delta)
    await page.waitForTimeout(500)
    const image = await paper()
    assert(image.coverage > 0.5, 'the document stays visible at either scroll boundary')
    if (delta < 0) assert(image.top < 0.2, 'the first page stays near the top after scrolling up')
  }
  assert.equal(await page.frameLocator('#kit').getByTestId('doc-unsaved').count(), 0, 'scrolling never edits the document')
  console.log(`PASS ${which}: scrolling stops at the document pages`)
} finally {
  await browser.close()
  await server.close()
}
