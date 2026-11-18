// Comment threads a host keeps (Kit UI protocol section 7k): the anchor the kit sends with
// `comment.create`, the marks it draws for `comments/show`, and its `comments/locate` answer.
import test from 'node:test'
import assert from 'node:assert/strict'
import { anchorOf, foundList, hostMarks, locatedList, markerPlaces, selectionLine } from './hostAnchors.ts'

const answer = { ids: ['p1', 'p2'], range: { start: 4, end: 12 }, quote: 'one.\nBeta', page: 2, at: { page: 1, x: 72, y: 90, height: 14 } }

test('the selection becomes the protocol anchor, without the view’s own place', () => {
  assert.deepEqual(anchorOf(answer, 'Page 2'), { ids: ['p1', 'p2'], range: { start: 4, end: 12 }, quote: 'one.\nBeta', page: 2, label: 'Page 2' })
  assert.deepEqual(selectionLine(answer), { page: 1, x: 72, y: 90, height: 14 })
  assert.equal(anchorOf(null), null, 'a caret is no anchor')
  assert.equal(anchorOf({ ids: [] }), null)
  assert.deepEqual(anchorOf({ ids: ['p1'], range: { start: 5, end: 2 }, quote: '' }), { ids: ['p1'] }, 'a backwards range and an empty quote are left out')
  assert.equal(anchorOf({ ids: ['p'], quote: 'x'.repeat(900) })?.quote?.length, 500, 'quotes within the protocol’s limit')
  assert.equal(anchorOf({ ids: Array.from({ length: 300 }, (_, i) => `p${i}`) })?.ids.length, 256)
})

test('located anchors are read defensively, one per anchor asked', () => {
  const located = locatedList([{ found: true, exact: false, text: 'quoted', page: 2, at: { page: 1, x: 1, y: 2, height: 3 }, rects: [{ page: 1, x: 1, y: 2, width: 3, height: 4 }, { page: -1 }] }, { found: false }, 'junk'], 4)
  assert.equal(located.length, 4)
  assert.deepEqual(located[0], { found: true, exact: false, text: 'quoted', page: 2, at: { page: 1, x: 1, y: 2, height: 3 }, rects: [{ page: 1, x: 1, y: 2, width: 3, height: 4 }] })
  assert.deepEqual(foundList(located), [true, false, false, false])
  assert.deepEqual(foundList(locatedList(null, 2)), [false, false], 'no document yet: nothing found')
})

test('open threads found in the document are marked on their page; resolved and lost ones are not', () => {
  const pages = [
    { x0: 0, y0: 0, x1: 612, y1: 792 },
    { x0: 0, y0: 812, x1: 612, y1: 1604 },
  ]
  const threads = [
    { id: 't1', anchor: { ids: ['p1'] }, resolved: false, count: 2 },
    { id: 't2', anchor: { ids: ['p2'] }, resolved: true, count: 1 },
    { id: 't3', anchor: { ids: ['gone'] }, resolved: false, count: 1 },
  ]
  const at = { page: 1, x: 72, y: 100, height: 14 }
  const located = locatedList([{ found: true, at, rects: [{ page: 1, x: 72, y: 100, width: 50, height: 14 }] }, { found: true, at, rects: [] }, { found: false }], 3)
  const marks = hostMarks(threads, located, pages, 't1')
  assert.deepEqual(marks, [{ id: 't1', count: 2, active: true, rects: [{ x: 72, y: 912, width: 50, height: 14 }], line: { x: 72, y: 912, height: 14 }, right: 612 }])
})

test('markers sit in the right margin beside their line and never overlap', () => {
  const mark = (id, y) => ({ id, count: 1, active: false, rects: [], line: { x: 72, y, height: 14 }, right: 612 })
  const places = markerPlaces([mark('b', 100), mark('a', 100), mark('c', 400)], 24, 12)
  assert.deepEqual(places[0], { id: 'b', x: 576, y: 95 })
  assert.equal(places[1].y >= places[0].y + 24, true, 'the second on the same line moves down')
  assert.deepEqual(places[2], { id: 'c', x: 576, y: 395 })
})
