// File › Export: the engine's bytes go to the host, which writes the new file; success only when
// the host answers, and the host's error otherwise.
import test from 'node:test'
import assert from 'node:assert/strict'
import { decodeBase64, exportParams, exportPath, exportThroughHost, parseAnswer } from './exportFile.ts'

const PDF = Buffer.from('%PDF-1.7 test').toString('base64')

test('asks the engine for the whole document as PDF, or one page as PNG', () => {
  assert.deepEqual(exportParams('pdf', 3), { format: 'pdf' })
  assert.deepEqual(exportParams('png', 3), { format: 'png', page: 3 })
  assert.deepEqual(exportParams('png', 0), { format: 'png', page: 1 })
})

test('names the new file after the document, in its folder', () => {
  assert.equal(exportPath('Reports/Q3.docx', 'Q3.docx', 'pdf'), 'Reports/Q3.pdf')
  assert.equal(exportPath('Q3.docx', 'Q3.docx', 'png', 2), 'Q3 - 2.png')
  assert.equal(exportPath('a/b/Plan.docx', '', 'pdf'), 'a/b/Plan.pdf')
  assert.equal(exportPath('x.docx', 'Why: now?.docx', 'pdf'), 'Why_ now_.pdf')
  assert.equal(exportPath('', '', 'pdf'), 'Document.pdf')
})

test('decodes the base64 the engine answers', () => {
  assert.deepEqual([...decodeBase64(PDF).slice(0, 5)], [...Buffer.from('%PDF-')])
})

test('refuses an answer without the file asked for', () => {
  assert.throws(() => parseAnswer('null', 'pdf'), /no PDF file/)
  assert.throws(() => parseAnswer(JSON.stringify({ extension: 'png', data: PDF }), 'pdf'), /no PDF file/)
  assert.equal(parseAnswer(JSON.stringify({ format: 'pdf', extension: 'pdf', mime: 'application/pdf', bytes: 13, data: PDF }), 'pdf').data, PDF)
})

const engine = (calls) => async (id, params) => {
  calls.push([id, params])
  return JSON.stringify({ format: params.format, extension: params.format, mime: 'x', bytes: 13, data: PDF })
}

test('hands the bytes to the host and answers the path the host wrote', async () => {
  const calls = []
  const writes = []
  const host = { init: { document: { path: 'Docs/Report.docx', name: 'Report.docx' } }, exportFile: async (path, bytes) => (writes.push([path, bytes]), `${path.slice(0, -4)} 2.pdf`) }
  assert.equal(await exportThroughHost(engine(calls), host, 'pdf', 4, 9), 'Docs/Report 2.pdf')
  assert.deepEqual(calls, [['file.export', { format: 'pdf' }]])
  assert.equal(writes[0][0], 'Docs/Report.pdf')
  assert.equal(new TextDecoder().decode(writes[0][1]), '%PDF-1.7 test')
  await exportThroughHost(engine(calls), host, 'png', 4, 9)
  assert.deepEqual(calls[1], ['file.export', { format: 'png', page: 4 }])
  assert.equal(writes[1][0], 'Docs/Report - 4.png')
  await exportThroughHost(engine(calls), host, 'png', 1, 1)
  assert.equal(writes[2][0], 'Docs/Report.png', 'a one-page document needs no page number')
})

test('the host\'s refusal and the engine\'s error reach the caller', async () => {
  const host = { init: { document: { path: 'R.docx', name: 'R.docx' } }, exportFile: async () => { throw new Error('the disk is full') } }
  await assert.rejects(exportThroughHost(engine([]), host, 'pdf', 1, 1), /the disk is full/)
  const failing = async () => { throw new Error('no page 9') }
  let wrote = false
  await assert.rejects(exportThroughHost(failing, { ...host, exportFile: async () => { wrote = true; return '' } }, 'png', 9, 1), /no page 9/)
  assert.equal(wrote, false, 'nothing reaches the host when the engine fails')
})
