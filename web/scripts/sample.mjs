// Makes public/sample/Sample.docx with the engine itself: a title, paragraphs, a list, and a table.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_doc_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_doc_kit_bg.wasm`) })
const doc = engine.DocDocument.create('Sample.docx', 612, 792)
const run = (command, params = {}) => doc.execute(command, JSON.stringify(params))
const paragraph = (text) => {
  run('text.insert', { text })
  run('text.newParagraph')
}
run('paragraph.style', { style: 'Title' })
paragraph('Field Notes: Northern Lights')
run('paragraph.style', { style: 'Normal' })
paragraph('Auroras appear when charged particles from the Sun meet the upper atmosphere. The color depends on the gas and the height.')
run('paragraph.style', { style: 'Heading 1' })
paragraph('What to bring')
run('paragraph.style', { style: 'Normal' })
run('paragraph.bullets', { kind: '•' })
paragraph('A tripod and a wide lens')
paragraph('Spare batteries, kept warm')
paragraph('A red flashlight')
run('paragraph.bullets', { off: true })
run('paragraph.style', { style: 'Heading 1' })
paragraph('Colors by height')
run('paragraph.style', { style: 'Normal' })
paragraph('Green comes from oxygen near 100 km; red from oxygen higher up; blue and violet from nitrogen.')
run('caret.docStart')
await fs.writeFile(`${web}public/sample/Sample.docx`, doc.save())
console.log('wrote public/sample/Sample.docx')
