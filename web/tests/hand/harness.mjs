// What the hand tests share (web/tests/hand/*.mjs, plan "Doc usable by hand"): a Vite server and
// one browser on dev.html, a person's input (page.mouse, page.keyboard), and reads that are no
// input of a person: the document through the development host's agent connection, the person's
// own selection through the editor's read-only commands, the canvas's view from the editor's React
// tree, and what the canvas draws over the pages (the caret) from its 2D calls.
// A test file runs its segments in order; `node tests/hand/<file>.mjs <chromium|webkit> [segment…]`
// runs only the named ones (a regression case names its segment).
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

/**
 * Records the caret the editor draws over the pages on the canvas surface, frame by frame (a frame
 * starts where the rendered pages are copied in): a vertical stroke of one segment, in the
 * canvas's CSS pixels. Read with `caretDrawn()`.
 */
const CARET_INIT = () => {
  const drawn = (window.__caret = { cur: null, dashed: null, under: [], fills: [], path: [], frames: 0 })
  const P = window.CanvasRenderingContext2D?.prototype
  if (!P) return
  const surface = (canvas) => canvas?.classList?.contains('kui-canvas-surface')
  const at = (ctx, x, y) => {
    const m = ctx.getTransform()
    const d = window.devicePixelRatio || 1
    return [(m.a * x + m.c * y + m.e) / d, (m.b * x + m.d * y + m.f) / d]
  }
  const wrap = (name, fn) => {
    const original = P[name]
    P[name] = function (...args) {
      try {
        if (surface(this.canvas)) fn(this, args)
      } catch {}
      return original.apply(this, args)
    }
  }
  wrap('drawImage', () => {
    drawn.cur = null
    drawn.dashed = null
    drawn.under = []
    drawn.fills = []
    drawn.path = []
    drawn.frames += 1
  })
  wrap('beginPath', () => {
    drawn.path = []
  })
  // A filled shape of four corners or more: a selection's highlight (stage 4: cells shaded whole).
  wrap('fill', (ctx, [path]) => {
    const points = path && typeof path === 'object' ? [] : drawn.path
    if (points.length < 4) return
    const xs = points.map((p) => p[0])
    const ys = points.map((p) => p[1])
    drawn.fills.push({ x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys), style: String(ctx.fillStyle) })
  })
  wrap('fillRect', (ctx, [x, y, w, h]) => {
    const [x0, y0] = at(ctx, x, y)
    const [x1, y1] = at(ctx, x + w, y + h)
    drawn.fills.push({ x0: Math.min(x0, x1), y0: Math.min(y0, y1), x1: Math.max(x0, x1), y1: Math.max(y0, y1), style: String(ctx.fillStyle), rect: true })
  })
  wrap('moveTo', (ctx, [x, y]) => drawn.path.push(at(ctx, x, y)))
  wrap('lineTo', (ctx, [x, y]) => drawn.path.push(at(ctx, x, y)))
  wrap('stroke', (ctx, [path]) => {
    const points = path ? [] : drawn.path
    if (points.length === 2 && Math.abs(points[0][0] - points[1][0]) < 0.6 && Math.abs(points[0][1] - points[1][1]) > 4 && ctx.getLineDash().length === 0 && !drawn.cur) {
      drawn.cur = { x: points[0][0], y: Math.min(points[0][1], points[1][1]), height: Math.abs(points[1][1] - points[0][1]) }
    }
    // A dashed upright bar: where dragged text would land (stage 1).
    if (points.length === 2 && Math.abs(points[0][0] - points[1][0]) < 0.6 && Math.abs(points[0][1] - points[1][1]) > 4 && ctx.getLineDash().length > 0 && !drawn.dashed) {
      drawn.dashed = { x: points[0][0], y: Math.min(points[0][1], points[1][1]), height: Math.abs(points[1][1] - points[0][1]) }
    }
    // A level line in the engine's dark grey: under what an input method composes (stage 3).
    if (points.length === 2 && Math.abs(points[0][1] - points[1][1]) < 0.6 && Math.abs(points[0][0] - points[1][0]) > 1 && ctx.getLineDash().length === 0 && String(ctx.strokeStyle).toLowerCase() === '#404040') {
      drawn.under.push({ x0: Math.min(points[0][0], points[1][0]), x1: Math.max(points[0][0], points[1][0]), y: points[0][1] })
    }
    drawn.path = []
  })
}

/** Opens dev.html (the sample document) in `which` browser at `width` × `height`; Single line unless `query` says otherwise. */
export async function open({ which = process.argv[2] ?? 'chromium', query = 'locale=en-US', width = 1600, height = 1000, mac = false } = {}) {
  const server = await createServer({ root: new URL('../..', import.meta.url).pathname, configFile: new URL('../../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const browser = await (which === 'webkit' ? webkit : chromium).launch()
  const context = await browser.newContext({ viewport: { width, height } })
  if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await context.addInitScript(CARET_INIT)
  // A Mac's keyboard on any machine (`mac`): the editor and its frame read the platform.
  if (mac) await context.addInitScript(() => Object.defineProperty(Navigator.prototype, 'platform', { get: () => 'MacIntel', configurable: true }))
  const page = await context.newPage()
  page.setDefaultTimeout(10_000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html${query ? `?${query}` : ''}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const frame = () => page.frames().find((item) => item !== page.mainFrame())
  const wait = (ms) => page.waitForTimeout(ms)
  const until = async (check, tries = 60, gap = 100) => {
    for (let index = 0; index < tries; index += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(gap)
    }
    return false
  }
  await until(() => kit.getByTestId('kit-canvas-surface').isVisible())
  await until(() => kit.getByTestId('kit-zoom').isVisible())

  /** Runs commands as the agent: setup and reads, never a person's action. */
  const agent = (commands) => page.evaluate(async (list) => (await window.devHost.connection().ai.run(list, false)).completed?.map((item) => item.result) ?? [], commands)
  const ask = async (command, params = {}) => (await agent([{ command, params }]))[0]
  /** The person's editor state, as the editor reads it (`inspect().details`). */
  const details = async () => {
    const info = await page.evaluate(async () => window.devHost.connection().ai.inspect())
    return info.details ?? info.inspected ?? info
  }
  /**
   * A read-only command answered for the person's own session (their caret and selection), through
   * the editor's `query` found on the React tree; `run` is for setup only (placing the caret).
   */
  const viaEditor = (method, command, params) =>
    frame().evaluate(
      async ([method, command, params]) => {
        const element = document.querySelector('.kui-app')
        const key = element && Object.keys(element).find((name) => name.startsWith('__reactFiber'))
        for (let fiber = element?.[key], depth = 0; fiber && depth < 200; depth += 1, fiber = fiber.return) {
          const editor = fiber.memoizedProps?.editor
          if (editor && typeof editor.query === 'function' && typeof editor.run === 'function') {
            const answer = await editor[method](command, params)
            if (typeof answer !== 'string') return answer
            try {
              return JSON.parse(answer)
            } catch {
              return answer
            }
          }
        }
        throw new Error('no editor on the React tree')
      },
      [method, command, params],
    )
  const read = (command, params = {}) => viaEditor('query', command, params)
  const setup = (command, params = {}) => viaEditor('run', command, params)
  /** The person's selection: its text and its ends. */
  const selection = () => read('file.selection')
  /** The body's paragraphs' text, in order (tables as one entry). */
  const paragraphs = async () => ((await ask('file.inspect', { text: true })).blocks ?? []).map((block) => block.text ?? '')
  /** A paragraph of the body (`file.paragraph`): its text, runs and resolved properties. */
  const paragraph = (index) => ask('file.paragraph', { path: [index] })
  const zoom = async () => Number((await kit.getByTestId('kit-zoom').textContent()).replace('%', ''))
  const surface = () => kit.getByTestId('kit-canvas-surface').boundingBox()
  /** The canvas's view (document point at its top-left, zoom), read from the editor's React tree. */
  const view = () =>
    frame().evaluate(() => {
      const element = document.querySelector('[data-testid=kit-canvas-surface]')
      if (!element) return null
      const key = Object.keys(element).find((name) => name.startsWith('__reactFiber'))
      for (let fiber = element[key], depth = 0; fiber && depth < 120; depth += 1, fiber = fiber.return) {
        const memory = fiber.memoizedProps?.viewMemory?.current
        if (memory) return { ...memory }
      }
      return null
    })
  /** The caret as the canvas draws it now, on the page (null when none is drawn). */
  const caretDrawn = async () => {
    const drawn = await frame().evaluate(() => window.__caret?.cur ?? null)
    if (!drawn) return null
    const box = await surface()
    return { x: box.x + drawn.x, y: box.y + drawn.y, height: drawn.height }
  }
  /** The dashed bar the canvas draws now (where dragged text lands), on the page; null when none. */
  const dashedDrawn = async () => {
    const drawn = await frame().evaluate(() => window.__caret?.dashed ?? null)
    if (!drawn) return null
    const box = await surface()
    return { x: box.x + drawn.x, y: box.y + drawn.y, height: drawn.height }
  }
  /** The filled shapes drawn now over the pages (highlights), on the page (`{x0, y0, x1, y1, style}`). */
  const fillsDrawn = async () => {
    const fills = await frame().evaluate(() => window.__caret?.fills ?? [])
    const box = await surface()
    return fills.map((f) => ({ ...f, x0: box.x + f.x0, x1: box.x + f.x1, y0: box.y + f.y0, y1: box.y + f.y1 }))
  }
  /** The lines drawn now under what an input method composes, on the page (`{x0, x1, y}`). */
  const underDrawn = async () => {
    const lines = await frame().evaluate(() => window.__caret?.under ?? [])
    const box = await surface()
    return lines.map((line) => ({ x0: box.x + line.x0, x1: box.x + line.x1, y: box.y + line.y }))
  }
  /** A canvas point (document points) on the page, as the view shows it now. */
  const toScreen = async (x, y) => {
    const shown = await view()
    const box = await surface()
    return { x: box.x + (x - shown.x) * shown.zoom, y: box.y + (y - shown.y) * shown.zoom }
  }
  /** The CSS cursor the canvas shows now. */
  const cursor = () => frame().evaluate(() => document.querySelector('[data-testid=kit-canvas-surface]')?.style.cursor ?? '')
  /**
   * The point on the page of character `ch` of body paragraph `index`: setup puts the person's caret
   * there through the editor's own `caret.set`, and the caret the canvas then draws is read back,
   * so it holds at any zoom and scroll. Null while that place is out of view.
   */
  const textPoint = async (index, ch = 0) => {
    const text = (await paragraph(index))?.paragraph?.text ?? ''
    const off = Buffer.from([...text].slice(0, ch).join(''), 'utf8').length
    return pathPoint([index], off)
  }
  /** As `textPoint`, for the paragraph at `path` (a table cell's: `[block, row, cell, 0]`), byte `off`. */
  const pathPoint = async (path, off = 0) => {
    await setup('caret.set', { pos: { story: 'body', path, off } })
    // The caret drawn there, not the one drawn before the canvas showed the new place: it is where
    // the engine reports the caret, on the screen.
    let drawn = null
    await until(async () => {
      drawn = await caretDrawn()
      const rect = await caretRect()
      if (!drawn || !rect) return false
      const at = await toScreen(rect.x, rect.top)
      return Math.abs(at.x - drawn.x) < 1.5 && Math.abs(at.y - drawn.y) < 1.5
    }, 30)
    if (!drawn) return null
    const box = await surface()
    const point = { x: drawn.x, y: drawn.y + drawn.height / 2 }
    return point.y > box.y && point.y < box.y + box.height ? point : null
  }
  /**
   * Setup, not a person's step: a document of `pages` pages from the agent, each a heading and a
   * few paragraphs, a page break between them, in place of the sample's text.
   */
  const pagesOf = async (pages) => {
    const commands = [{ command: 'edit.selectAll', params: {} }, { command: 'text.delete', params: {} }]
    for (let index = 0; index < pages; index += 1) {
      if (index) commands.push({ command: 'page.insertBreak', params: {} })
      commands.push({ command: 'text.insert', params: { text: `Page ${index + 1}` } })
      for (let line = 0; line < 3; line += 1) {
        commands.push({ command: 'text.newParagraph', params: {} })
        commands.push({ command: 'text.insert', params: { text: `Line ${line + 1} of page ${index + 1}.` } })
      }
    }
    await agent(commands)
    await until(async () => (await details()).pages?.length >= pages)
  }
  /** The element with the keyboard in the editor's frame, in short. */
  const active = () =>
    frame().evaluate(() => {
      const element = document.activeElement
      if (!element || element === document.body) return 'body'
      return `${element.tagName.toLowerCase()}${element.className && typeof element.className === 'string' ? `.${element.className.split(' ')[0]}` : ''}`
    })
  /** The caret's place on the canvas as the engine reports it (`inspect().caret`, points). */
  const caretRect = async () => (await details()).caret ?? null
  /** Whether the engine's caret (with a selection, its moving end) is inside the canvas's view. */
  const caretInView = async () => {
    const caret = await caretRect()
    const shown = await view()
    const box = await surface()
    if (!caret || !shown || !box) return false
    const high = box.height / shown.zoom
    return caret.top >= shown.y - 0.5 && caret.top + caret.height <= shown.y + high + 0.5
  }
  /** The system clipboard's text (chromium, which grants the permission). */
  const clipboardText = () => page.evaluate(() => navigator.clipboard.readText())
  const close = async () => {
    await browser.close().catch(() => undefined)
    await server.close().catch(() => undefined)
  }
  return { which, page, kit, frame, wait, until, agent, ask, details, read, setup, selection, paragraphs, paragraph, zoom, surface, view, caretDrawn, dashedDrawn, underDrawn, toScreen, cursor, caretRect, caretInView, textPoint, pathPoint, fillsDrawn, pagesOf, active, clipboardText, errors, close }
}

/**
 * Runs a test file's segments, each `[name, fn]`, in order: only those named on the command line
 * when any are. Prints PASS or FAIL per segment and exits non-zero on a failure.
 */
export async function segments(title, list, start) {
  const which = process.argv[2] ?? 'chromium'
  const wanted = process.argv.slice(3)
  const chosen = wanted.length ? list.filter(([name]) => wanted.includes(name)) : list
  if (wanted.length && chosen.length !== wanted.length) {
    console.log(`FAIL unknown segment among ${wanted.join(', ')}`)
    process.exit(1)
  }
  const editor = await start(which)
  let failed = 0
  try {
    for (const [name, fn] of chosen) {
      try {
        await fn(editor)
        console.log(`PASS ${title} ${name} (${which})`)
      } catch (error) {
        failed += 1
        console.log(`FAIL ${title} ${name} (${which}): ${error instanceof Error ? error.message.split('\n').slice(0, 4).join(' | ') : String(error)}`)
        await editor.page.keyboard.press('Escape').catch(() => undefined)
        await editor.page.mouse.up().catch(() => undefined)
      }
    }
    if (editor.errors.length) {
      failed += 1
      console.log(`FAIL page errors: ${editor.errors.slice(0, 5).join(' | ')}`)
    }
  } finally {
    await editor.close()
  }
  process.exit(failed ? 1 : 0)
}
