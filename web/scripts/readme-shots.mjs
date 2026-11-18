// Takes the README's pictures of the Doc editor (assets/readme/*.png at the repository root) on
// kition-sdk's shared runner (tools/readme-shots/README.md there): the development host in Classic
// mode, light theme, English, 1440 × 900 at device scale 2, on two kition-showcase samples. Not a
// test; re-run it by hand when the editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine and fonts in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk and kition-showcase checkouts next to this repository (or KITION_SDK and
// KITION_SHOWCASE naming them). Run it with no HTTP proxy set. Every step is a person's: a click on
// the page or the ribbon, or the Find box. The pictures are PNG at device scale 2; shrink them
// afterwards with `pngquant --quality 70-90` (the README's are at most 400 KB each).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { openFile, takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

/** The showcase samples: a magazine feature in two columns, and a two-page quarterly report. */
const MAGAZINE = 'magazine-feature'
const REPORT = 'quarterly-business-report'
const SAMPLE = { hero: MAGAZINE, columns: MAGAZINE, styles: REPORT, table: REPORT, header: REPORT }

/** A point in the kit's frame (CSS pixels from its top left corner), in the page's coordinates. */
async function at(page, x, y) {
  const frame = await page.locator('#kit').boundingBox()
  return { x: frame.x + x, y: frame.y + y }
}

/** Selects `text` as a person does with Find (Enter finds it; Escape closes the box, keeping it selected). */
async function find(page, kit, text) {
  await kit.getByTestId('doc-find').click()
  await kit.getByTestId('doc-find-text').pressSequentially(text, { delay: 20 })
  await kit.getByTestId('doc-find-text').press('Enter')
  await page.waitForTimeout(600)
  await kit.getByTestId('doc-find-text').press('Escape')
  await page.waitForTimeout(600)
}

await takeShots({
  web,
  sample: async (page, shot) => {
    const name = SAMPLE[shot.name]
    await openFile(page, { name: `${name}.docx`, bytes: fs.readFileSync(path.join(showcase, 'doc', name, `${name}.docx`)) })
  },
  ready: '[data-testid="doc-canvas-region"]',
  query: { modes: 'classic' },
  shots: [
    // The magazine feature as it opens, at 100%: the headline, the standfirst, the drop cap, two
    // justified columns and the pull quote.
    {
      name: 'hero',
      steps: async (page, { kit }) => {
        await kit.getByTestId('ribbon-tab-view').click()
        await kit.getByTestId('doc-zoom-100').click()
        await kit.getByTestId('ribbon-tab-home').click()
      },
    },
    // Styles: a heading selected, and the gallery showing the style it has (Heading 1).
    { name: 'styles', steps: async (page, { kit }) => find(page, kit, 'Monthly revenue against last year') },
    // A table: a row's first cell selected, and the Table Layout tab (rows, columns, merge, size, alignment).
    {
      name: 'table',
      steps: async (page, { kit }) => {
        await find(page, kit, 'South and West')
        await kit.getByTestId('ribbon-tab-tableLayout').click()
      },
    },
    // Headers and footers: Insert › Header › Edit Header puts the caret in the header; Home and
    // Shift+End select its line; the Header & Footer tab.
    {
      name: 'header',
      steps: async (page, { kit }) => {
        await kit.getByTestId('ribbon-tab-insert').click()
        await kit.getByRole('button', { name: 'Header', exact: true }).click()
        await kit.getByRole('menuitem', { name: 'Edit Header' }).click()
        await kit.getByTestId('ribbon-tab-headerFooter').waitFor()
        await page.waitForTimeout(400)
        await page.keyboard.press('Home')
        await page.keyboard.press('Shift+End')
        await page.waitForTimeout(400)
        await kit.getByTestId('ribbon-tab-headerFooter').click()
      },
    },
    // Columns: the caret in the story, the Layout tab, and its Columns menu open.
    {
      name: 'columns',
      steps: async (page, { kit }) => {
        const story = await at(page, 540, 648)
        await page.mouse.click(story.x, story.y)
        await kit.getByTestId('ribbon-tab-layout').click()
        await kit.getByRole('button', { name: 'Columns', exact: true }).click()
        await kit.getByRole('menuitem', { name: 'Two' }).waitFor()
      },
    },
  ],
})
