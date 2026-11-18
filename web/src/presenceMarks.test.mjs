// Presence in the word processor: what this session shares (its caret and selection as replica
// text positions) and where the other people's show on the pages.
import test from 'node:test'
import assert from 'node:assert/strict'
import { peerMarks, placesOf, sharedPresence, textPosition } from './presenceMarks.ts'

const at = (object, offset) => ({ object, field: 'text', offset })

test('shares the caret, the selection, and its other end', () => {
  assert.equal(sharedPresence(null), null)
  assert.deepEqual(sharedPresence({ caret: null, anchor: null, selection: [], page: 0 }), { selection: [], viewport: { page: 0 } })
  assert.deepEqual(sharedPresence({ caret: at('p2', 4), anchor: at('p1', 6), selection: ['p1', 'p2', 7], page: 1 }), {
    selection: ['p1', 'p2'],
    caret: at('p2', 4),
    viewport: { page: 1, anchor: at('p1', 6) },
  })
})

test('a malformed text position is no position', () => {
  assert.deepEqual(textPosition(at('p', 0)), at('p', 0))
  for (const bad of [null, 'p', { object: 'p', field: 'text' }, at('p', -1), at('p', 1.5), { object: 1, field: 'text', offset: 0 }]) assert.equal(textPosition(bad), null)
})

const alice = { principal: 'a', name: 'Alice', color: '#e11d48', selection: [], caret: at('p2', 4), viewport: { page: 0, anchor: at('p1', 6) } }
const bob = { principal: 'b', name: 'Bob', color: '#2563eb', selection: [], viewport: { page: 1 } }

test('asks the engine to place each peer, in order', () => {
  assert.deepEqual(placesOf([alice, bob]), [{ caret: at('p2', 4), anchor: at('p1', 6) }, { caret: null }])
})

test('places carets and selections on the canvas, on their pages', () => {
  const pages = [
    { x0: 40, y0: 40, x1: 652, y1: 832 },
    { x0: 40, y0: 856, x1: 652, y1: 1648 },
  ]
  const placed = [
    { caret: { page: 1, x: 100, y: 72, height: 14 }, rects: [{ page: 0, x: 200, y: 700, width: 300, height: 14 }, { page: 1, x: 72, y: 72, width: 28, height: 14 }] },
    { caret: null, rects: [] },
  ]
  const marks = peerMarks([alice, bob], placed, pages)
  assert.equal(marks.length, 1, 'Bob shows nothing here')
  assert.deepEqual(marks[0].caret, { x: 140, y: 928, width: 2, height: 14, at: at('p2', 4) })
  assert.deepEqual(marks[0].rects, [
    { x: 240, y: 740, width: 300, height: 14 },
    { x: 112, y: 928, width: 28, height: 14 },
  ])
  // A page this view does not have (laid out differently): left out.
  assert.deepEqual(peerMarks([alice], [{ caret: { page: 5, x: 0, y: 0, height: 10 }, rects: [] }], pages), [])
  // An answer for fewer peers than asked: the rest show nothing.
  assert.equal(peerMarks([alice, bob], [], pages).length, 0)
})
