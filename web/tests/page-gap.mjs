// Paper stays near the ribbon in short and tall windows, without editing the document.
import './platform.mjs'
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 2048, height: 600 } })
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60_000 })
  const kit = page.frameLocator('#kit')
  const surface = kit.getByTestId('kit-canvas-surface')
  await surface.waitFor()
  for (const height of [600, 1800, 900]) {
    await page.setViewportSize({ width: 2048, height })
    let gap = Infinity
    for (let attempt = 0; attempt < 40; attempt += 1) {
      gap = await surface.evaluate((canvas) => {
        const pixels = canvas.getContext('2d').getImageData(Math.floor(canvas.width / 2), 0, 1, canvas.height).data
        for (let y = 0; y < canvas.height; y += 1) {
          if (pixels[y * 4] === 255 && pixels[y * 4 + 1] === 255 && pixels[y * 4 + 2] === 255) return y / window.devicePixelRatio
        }
        return canvas.clientHeight
      })
      if (gap >= 16 && gap <= 24) break
      await page.waitForTimeout(100)
    }
    assert(gap >= 16 && gap <= 24, `${height}px window: paper should start about 20px below the ribbon, got ${gap}px`)
    assert.equal(await kit.getByTestId('kit-zoom').textContent(), '133%', 'window height does not change reading scale')
    await page.screenshot({ path: new URL(`../../target/page-gap-${which}-${height}.png`, import.meta.url).pathname })
  }
  assert.equal(await kit.getByTestId('doc-unsaved').count(), 0, 'placing paper never edits the document')
  console.log(`PASS ${which}: compact paper spacing survives window resizing`)
} finally {
  await browser.close()
  await server.close()
}
