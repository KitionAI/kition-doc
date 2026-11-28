import type { KitEngineInfo, RasterRect } from '@kition/sdk/editor'
import type { CaretRect } from './followCaret'

/** What the editor reads from the engine's `inspect()`. */
export type DocInfo = {
  page: number
  /** Where the caret (the selection's moving end) is on the canvas, in points. */
  caret: CaretRect | null
  /** What an input method composes at the caret (its text and cursor, characters), shown by the engine. */
  preedit: { text: string; cursor: number } | null
  /** Where a composition starts on the canvas (the caret, or the selection's start), in points. */
  inputAt: CaretRect | null
  pages: RasterRect[]
  words: number
  /** Characters selected. */
  selected: number
  /** Where the caret is: `body`, or `headerFooter`. */
  story: string
  equation: boolean
  /** The selected shape's id (`drawing.*`). */
  shape: string | null
  /** Every selected drawing object's id (the selected one, then those added with Shift). */
  shapes: string[]
  /** The selected picture: its box on the canvas (points, around it as turned) and how text wraps it. */
  picture: PictureBox | null
  /** The selected chart's id (`drawing.chart`). */
  chart: string | null
  /** Whether Escape has something to cancel: a gesture, a shape chosen to draw, or a selection. */
  escape: boolean
  /** Whether the selection is the whole story (Select All, three clicks in the selection bar). */
  all: boolean
  /** The format painter held: for one selection, or until put down (double-clicked); `null` when not. */
  painter: 'once' | 'sticky' | null
  /** Whether Cmd+A selects the table the caret is in first (WPS W-N6), not yet everything. */
  selectsTable: boolean
}

type Area = { x: number; y: number; width: number; height: number }

/** The selected picture (stage 5A): its page, its box on the canvas in points, its wrapping (`arrange.wrap`'s values), its angle. */
export type PictureBox = { page: number; x: number; y: number; width: number; height: number; wrap: string; rotation: number }

const pictureOf = (picture: unknown): PictureBox | null => {
  if (!picture || typeof picture !== 'object') return null
  const { page, x, y, width, height, wrap, rotation } = picture as Record<string, unknown>
  return [page, x, y, width, height].every((value) => typeof value === 'number' && Number.isFinite(value)) && typeof wrap === 'string'
    ? { page: page as number, x: x as number, y: y as number, width: width as number, height: height as number, wrap, rotation: typeof rotation === 'number' && Number.isFinite(rotation) ? rotation : 0 }
    : null
}

const caretOf = (caret: unknown): CaretRect | null => {
  if (!caret || typeof caret !== 'object') return null
  const { page, x, top, height } = caret as Record<string, unknown>
  return [page, x, top, height].every((value) => typeof value === 'number' && Number.isFinite(value)) ? { page: page as number, x: x as number, top: top as number, height: height as number } : null
}

const preeditOf = (preedit: unknown): { text: string; cursor: number } | null => {
  if (!preedit || typeof preedit !== 'object') return null
  const { text, cursor } = preedit as Record<string, unknown>
  return typeof text === 'string' && typeof cursor === 'number' ? { text, cursor } : null
}

export function describe(info: KitEngineInfo): DocInfo {
  const inspected = info.inspected as { page?: number; caret?: CaretRect | null; preedit?: unknown; inputAt?: unknown; pages?: Array<{ area?: Area }>; words?: number; selected?: number; story?: unknown; equation?: boolean; shape?: string | null; shapes?: string[]; picture?: unknown; chart?: string | null; escape?: boolean; all?: boolean; painter?: unknown; selectsTable?: boolean }
  return {
    page: inspected.page ?? 0,
    caret: caretOf(inspected.caret),
    preedit: preeditOf(inspected.preedit),
    inputAt: caretOf(inspected.inputAt),
    pages: (inspected.pages ?? []).map(({ area }) => (area ? { x0: area.x, y0: area.y, x1: area.x + area.width, y1: area.y + area.height } : { x0: 0, y0: 0, x1: 0, y1: 0 })),
    words: inspected.words ?? 0,
    selected: inspected.selected ?? 0,
    story: typeof inspected.story === 'string' ? inspected.story : 'body',
    equation: inspected.equation === true,
    shape: inspected.shape ?? null,
    shapes: Array.isArray(inspected.shapes) ? inspected.shapes : [],
    picture: pictureOf(inspected.picture),
    chart: inspected.chart ?? null,
    escape: inspected.escape === true,
    all: inspected.all === true,
    painter: inspected.painter === 'once' || inspected.painter === 'sticky' ? inspected.painter : null,
    selectsTable: inspected.selectsTable === true,
  }
}

/** `format.state`: the formatting at the selection. */
export type FormatState = {
  font?: string
  size?: number | null
  bold?: boolean
  italic?: boolean
  underline?: boolean
  strike?: boolean
  subscript?: boolean
  superscript?: boolean
  border?: boolean
  styleName?: string
  color?: string
  /** The caret's paragraph's alignment (`left`, `center`, `right`, `justify`, `distribute`). */
  align?: string | null
}

/** `view.state`: the view's options and the drawing mode. */
export type ViewState = {
  mode?: string
  marks?: boolean
  ruler?: boolean
  gridlines?: boolean
  navPane?: boolean
  readMode?: boolean
  focusMode?: boolean
  draw?: { mode?: string }
}

export type StyleEntry = { id: string; name: string; kind: string; quick: boolean }

/**
 * The contextual tabs and when they show: while the caret is in a table, on a picture or shape,
 * in a header or footer, or in an equation.
 */
export const CONTEXT_TABS: Array<{ id: string; menu: string; when: (enabled: (id: string) => boolean, info: DocInfo) => boolean; extra?: string[]; label?: string; primary?: readonly string[] }> = [
  { id: 'tableDesign', menu: 'Table Design', when: (enabled) => enabled('table.style'), primary: ['table.style', 'table.shading', 'table.borders', 'table.look'] },
  { id: 'tableLayout', menu: 'Table Layout', when: (enabled) => enabled('table.insertRowBelow'), primary: ['table.insertRowAbove', 'table.insertRowBelow', 'table.insertColumnLeft', 'table.insertColumnRight', 'table.deleteRow', 'table.deleteColumn', 'table.merge', 'table.split', 'table.cellAlign'] },
  // Its own Arrange group (Wrap Text, which opens the Layout Options panel beside the picture, and
  // Rotate) is `PictureArrangeGroup`; Crop and Corrections are off it (PICTURE_OFF; stage 5A item 4).
  { id: 'pictureFormat', menu: 'Picture Format', when: (enabled) => enabled('image.size') && !enabled('shape.fill') && !enabled('arrange.ungroup'), primary: ['image.size', 'image.color', 'image.effects', 'image.style', 'image.border', 'image.altText'] },
  // A shape, or a group (Arrange › Ungroup applies), takes Shape Format.
  { id: 'shapeFormat', menu: 'Shape Format', when: (enabled) => enabled('shape.fill') || enabled('arrange.ungroup'), primary: ['shape.fill', 'shape.outline', 'shape.effects', 'shape.change'] },
  { id: 'chartDesign', menu: 'Chart Design', label: 'chartDesign', when: (_, info) => info.chart != null },
  { id: 'headerFooter', menu: 'Header & Footer', when: (_, info) => info.story === 'headerFooter', primary: ['header.close', 'page.differentFirstPage', 'page.differentOddEven', 'page.linkToPrevious', 'headerFooter.previous', 'headerFooter.next'] },
  { id: 'equation', menu: 'Equation', when: (_, info) => info.equation, primary: ['equation.insertStructure', 'equation.insertSymbol', 'equation.display', 'equation.inputFormat', 'equation.normalText', 'equation.convert', 'equation.exit'] },
]

/**
 * Picture Format's buttons left off the tab (stage 5A item 4): Crop and Corrections have no window
 * a person would do them in (no crop handles, no gallery of corrections), only the command's
 * numbers; the commands stay for agents and the command search.
 */
export const PICTURE_OFF = ['image.crop', 'image.corrections']

/** File commands the host's own Save, Open, and Close already cover. */
export const HOST_FILE_COMMANDS = ['file.new', 'file.open', 'file.save', 'file.saveAs', 'file.close', 'file.print', 'file.share', 'file.options']

/**
 * The engine's export commands, which the File tab's own Export group runs instead: the engine's
 * bytes go to the host, which writes the file (`ribbon/ExportGroup.tsx`). `file.exportPdf` and
 * `file.exportPng` write to a path, which only the headless kit can.
 */
export const EXPORT_COMMANDS = ['file.export', 'file.exportPdf', 'file.exportPng']

/** The engine commands each generated tab shows in Single line's row (Office's Simplified Ribbon), by menu. */
export const DOC_PRIMARY: Record<string, readonly string[]> = {
  File: ['file.info', 'file.properties', 'file.encrypt'],
  Design: ['theme.set', 'theme.styleSet', 'theme.colors', 'theme.fonts', 'theme.paragraphSpacing', 'page.watermark', 'page.color', 'page.borders'],
  Layout: ['paragraph.indents', 'paragraph.spacing', 'arrange.position', 'arrange.wrap', 'arrange.align'],
  References: ['toc.insert', 'toc.update', 'footnote.insert', 'endnote.insert', 'caption.insert', 'citation.insert', 'bibliography.insert', 'index.insert'],
  Mailings: ['mailMerge.start', 'mailMerge.recipients', 'mailMerge.insertField', 'mailMerge.addressBlock', 'mailMerge.greetingLine', 'mailMerge.preview', 'mailMerge.finish'],
  Review: ['spelling.check', 'file.wordCount', 'comment.new', 'comment.deleteCurrent', 'comment.previous', 'comment.next', 'change.track', 'change.accept', 'change.reject'],
}

export const FONTS = ['Aptos', 'Aptos Display', 'Calibri', 'Cambria', 'Arial', 'Helvetica', 'Times New Roman', 'Georgia', 'Garamond', 'Verdana', 'Courier New', 'Consolas']
export const SIZES = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72]
