/**
 * The system clipboard (plan "Doc usable by hand", stage 0A, section 2 row 14; walkthrough J1, J2,
 * J4; WPS W-J1, W-J4–W-J6). Copy and Cut hand the system clipboard the engine's answer (the copied
 * text) while the key press is still the person's: WebKit refuses a write once the engine has
 * answered. Paste reads the clipboard the browser's `paste` event carries (no permission asked),
 * else the clipboard API (the ribbon's buttons). What is pasted goes to the engine as the command's
 * parameters, so the engine decides: text that is its own last copy pastes its formatted fragment,
 * other text pastes as text, HTML as HTML, a picture as a picture. An agent never sees this
 * clipboard (the engine's paste gate); this is the person's paste.
 */
import { base64, toast } from '@kition/sdk/editor'

export type Clip = { text: string; html: string; image: Blob | null }

/** How a paste takes the clipboard: as it is (`keep`), its text only, or through Paste Special. */
export type PasteMode = 'keep' | 'text' | 'special'

export type ClipboardEditor = {
  run: (command: string, params?: Record<string, unknown>) => Promise<string | null>
  enabled: (command: string) => boolean
  ask: (command: string, values?: Record<string, unknown>) => void
  t: (key: string) => string
}

const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp']

/** The clipboard a `paste` event carries. */
export function clipOf(data: DataTransfer): Clip {
  const file = [...data.files].find((item) => IMAGE_TYPES.includes(item.type)) ?? null
  return { text: data.getData('text/plain'), html: data.getData('text/html'), image: file }
}

/** The system clipboard through the clipboard API, which asks for permission (null when refused). */
export async function readClipboard(): Promise<Clip | null> {
  try {
    const items = await navigator.clipboard.read()
    const read = async (type: string) => {
      const item = items.find((entry) => entry.types.includes(type))
      return item ? (await item.getType(type)).text() : ''
    }
    let image: Blob | null = null
    for (const item of items) {
      const type = item.types.find((name) => IMAGE_TYPES.includes(name))
      if (type) {
        image = await item.getType(type)
        break
      }
    }
    return { text: await read('text/plain'), html: await read('text/html'), image }
  } catch {
    try {
      return { text: await navigator.clipboard.readText(), html: '', image: null }
    } catch {
      return null
    }
  }
}

/**
 * Copies (or cuts) the selection to the system clipboard; with nothing selected it keeps what it
 * held. A selection with no text (a picture) empties it, so Cmd+V pastes the engine's own copy, not
 * what another application had put there before.
 */
export async function copySelection(editor: ClipboardEditor, cut: boolean, selected: boolean) {
  if (cut && !editor.enabled('edit.cut')) return
  const copied = editor
    .run(cut ? 'edit.cut' : 'edit.copy', {})
    .then((raw) => (raw ? (JSON.parse(raw) as { text?: string; html?: string }) : {}))
    .then(({ text, html }) => {
      const plain = text ?? ''
      ownCopy = plain ? plain.replace(/\r\n/g, '\n') : null
      return { plain, html: plain ? (html ?? '') : '' }
    })
  // Written before the engine answers: the clipboard waits for the blobs. Paragraphs are CR LF
  // apart, as a word processor puts them (WPS W-J6); the HTML (stage 0B) keeps the formatting for
  // other applications. A picture alone has neither, so Cmd+V pastes the engine's own copy.
  const text = copied.then(({ plain }) => (plain || selected ? new Blob([plain.replace(/\r?\n/g, '\r\n')], { type: 'text/plain' }) : Promise.reject(new Error('nothing selected'))))
  const html = copied.then(({ html }) => new Blob([html], { type: 'text/html' }))
  try {
    // The item is made now, while the key press is the person's (WebKit refuses it later): an
    // empty HTML when there is none.
    await navigator.clipboard.write([new ClipboardItem({ 'text/plain': text, 'text/html': html })])
  } catch {
    const plain = (await copied.catch(() => ({ plain: '' }))).plain
    if (plain || selected) await navigator.clipboard.writeText(plain.replace(/\r?\n/g, '\r\n')).catch(() => undefined)
  }
}

/**
 * The text of this editor's last copy (LF paragraphs), or null: a clipboard still holding it pastes
 * the engine's own formatted copy, not its HTML (which keeps less).
 */
let ownCopy: string | null = null

/**
 * HTML with something in it: a browser makes the empty HTML of a picture's copy a bare `<meta>`,
 * which pastes nothing (the engine's own copy is the picture).
 */
const withContent = (html: string) => (/<(img|table|hr)\b/i.test(html) || html.replace(/<(style|script|head|title)\b[\s\S]*?<\/\1>/gi, '').replace(/<[^>]*>/g, '').replace(/&nbsp;/g, ' ').trim() !== '' ? html : '')

/** Pastes `clip` in `mode`. */
export async function pasteClip(editor: ClipboardEditor, clip: Clip, mode: PasteMode) {
  const { text, image } = clip
  const html = withContent(clip.html)
  if (mode === 'special') {
    editor.ask('edit.pasteSpecial', { ...(text ? { text } : {}), ...(html ? { html } : {}) })
    return
  }
  if (mode === 'text') {
    if (text) await editor.run('edit.pasteText', { text })
    else toast('info', editor.t('clipboardEmpty'))
    return
  }
  // A picture alone (a screenshot) goes in as a picture.
  if (image && !text) {
    await editor.run('image.insert', { data: base64(new Uint8Array(await image.arrayBuffer())) })
    return
  }
  if (html && !(ownCopy !== null && text.replace(/\r\n/g, '\n') === ownCopy)) {
    await editor.run('edit.pasteSpecial', { as: 'html', html, ...(text ? { text } : {}) })
    return
  }
  // Text, or nothing the browser shows us: the engine's own last copy when there is no text.
  await editor.run('edit.paste', text ? { text } : {})
}

/**
 * A paste the person asked for with a button or a key the browser does not paste on (Keep Text
 * Only, Paste Special): reads the clipboard through the clipboard API. When the browser refuses,
 * the editor's own last copy is still pasted, and a notice says how to paste from elsewhere.
 */
export async function pasteFromClipboard(editor: ClipboardEditor, mode: PasteMode) {
  const clip = await readClipboard()
  try {
    if (clip) {
      await pasteClip(editor, clip, mode)
      return
    }
    // The browser refused: the editor's own copy, if any, and a word on Cmd+V.
    if (mode === 'special') editor.ask('edit.pasteSpecial')
    else if ((await editor.run(mode === 'text' ? 'edit.pasteText' : 'edit.paste', {})) == null) return
    toast('info', editor.t('clipboardDenied'))
  } catch (error) {
    toast('error', editor.t('pasteFailed'), clip ? (error instanceof Error ? error.message : String(error)) : editor.t('clipboardDenied'))
  }
}
