/**
 * The editor's own keys, as a table (plan "Doc usable by hand", stage 0A, section 2 rows 14, 30,
 * 35; walkthrough A5, G13, J4, Q4, C10; WPS W-J1, W-R1, W-U4, W-L7). kit-editor's frame asks the
 * editor (`EditorFrame`'s `onKeyDown`) after the engine's text tool and before its own zoom keys and
 * the commands' shortcuts, so a key here takes the place of both. Each row's action runs through the
 * editor's `run` (the ordered queue with the tool's events) or opens a part of the editor.
 *
 * A row names its keys as the commands' shortcuts do: `Cmd` is Cmd on a Mac and Ctrl elsewhere
 * (either of them), `Ctrl` the Control key itself. No row works while an input method composes
 * (the key is the input method's). Cmd+V is not here: the browser's own paste event carries the
 * clipboard (App.tsx). Later stages add their rows here.
 */

/** What a row's action can do: the editor's runs and its parts. */
export type KeyContext = {
  run: (command: string, params?: Record<string, unknown>) => void
  /** Opens the command's parameters (the generic dialog), as the ribbon's button does. */
  ask: (command: string) => void
  /** Opens the find bar, on the replace field with `replacing`. */
  openFind: (replacing: boolean) => void
  /** Find Next (`1`) or Previous (`-1`): opens the find bar first when it is closed. */
  findStep: (direction: 1 | -1) => void
  clipboard: (action: 'copy' | 'cut' | 'pasteText' | 'pasteSpecial') => void
}

export type KeyEvent = { key: string; code: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean; keyCode?: number; isComposing?: boolean; nativeEvent?: { isComposing?: boolean } }

type KeyRow = {
  keys: string[]
  /** Only when this holds (the row's key is left to others otherwise). */
  when?: () => boolean
  act: (context: KeyContext, event: KeyEvent) => void
}

export const onMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform)

export const KEYS: KeyRow[] = [
  // Cmd+0, Cmd+= and Cmd+- are not zoom keys in a word processor (WPS W-U4, W-L7; section 2 row
  // 35): Cmd+= is Subscript and Cmd+Shift+= Superscript (Word for Mac), Cmd+0 adds or removes the
  // space before the paragraph (the engine's own key), and Cmd+- does nothing.
  { keys: ['Cmd+='], act: (c) => c.run('format.subscript') },
  { keys: ['Cmd+Shift+='], act: (c) => c.run('format.superscript') },
  { keys: ['Cmd+0'], act: (c) => c.run('paragraph.addSpaceBefore') },
  { keys: ['Cmd+-'], act: () => undefined },
  // Find, Replace, Go To (WPS W-R1; Cmd+H is the system's Hide, so Replace is Ctrl+H).
  { keys: ['Cmd+F'], act: (c) => c.openFind(false) },
  { keys: ['Ctrl+H'], act: (c) => c.openFind(true) },
  { keys: ['Cmd+G', 'F3'], act: (c) => c.findStep(1) },
  { keys: ['Cmd+Shift+G'], act: (c) => c.findStep(-1) },
  { keys: ['F5'], act: (c) => c.ask('edit.goto') },
  { keys: ['Ctrl+G'], when: onMac, act: (c) => c.ask('edit.goto') },
  // Show/Hide ¶ (both of the engine's keys).
  { keys: ['Cmd+8', 'Cmd+Shift+8'], act: (c) => c.run('view.marks') },
  // Insert › Link's parameters until the link dialog (stage 6A).
  { keys: ['Cmd+K'], act: (c) => c.ask('link.insert') },
  // Cmd+Y redoes as Cmd+Shift+Z does (WPS W-I1; the catalogue reads only the last of the
  // engine's two keys).
  { keys: ['Cmd+Y'], act: (c) => c.run('edit.redo') },
  // Ctrl+A selects all as Cmd+A does (WPS W-D10); elsewhere Ctrl is Cmd already.
  { keys: ['Ctrl+A'], when: onMac, act: (c) => c.run('edit.selectAll') },
  // The system clipboard: written while the key press is still the person's.
  { keys: ['Cmd+C'], act: (c) => c.clipboard('copy') },
  { keys: ['Cmd+X'], act: (c) => c.clipboard('cut') },
  { keys: ['Cmd+Alt+Shift+V'], act: (c) => c.clipboard('pasteText') },
  { keys: ['Ctrl+Cmd+V', 'Cmd+Alt+V'], act: (c) => c.clipboard('pasteSpecial') },
]

/** The key codes of the characters rows name (the code, not the character Shift makes of it). */
const CODES: Record<string, string> = { '=': 'Equal', '-': 'Minus' }
/** The characters a key with Shift (or without) may report for a row's character. */
const CHARACTERS: Record<string, string[]> = { '=': ['=', '+'], '-': ['-', '_'] }

/** Whether `event` is the key `shortcut` names (`Cmd+Shift+G`, `F3`, `Ctrl+Cmd+V`). */
export function keyMatches(shortcut: string, event: KeyEvent) {
  const parts = shortcut.split('+')
  const name = parts.length > 1 && parts[parts.length - 1] === '' ? '+' : (parts.pop() ?? '')
  const mods = new Set(parts.filter(Boolean))
  const command = mods.has('Cmd')
  const control = mods.has('Ctrl')
  // `Ctrl+Cmd` wants both keys (⌃⌘V); a `Cmd` row is not pressed with both.
  const both = event.metaKey && event.ctrlKey
  if (command && control ? !both : command ? !(event.metaKey || event.ctrlKey) || both : control ? !event.ctrlKey || event.metaKey : event.metaKey || event.ctrlKey) return false
  if (mods.has('Shift') !== event.shiftKey || mods.has('Alt') !== event.altKey) return false
  if (/^[A-Za-z]$/.test(name)) return event.code === `Key${name.toUpperCase()}` || event.key.toLowerCase() === name.toLowerCase()
  if (/^[0-9]$/.test(name)) return event.code === `Digit${name}` || event.key === name
  if (CODES[name]) return event.code === CODES[name] || (CHARACTERS[name] ?? [name]).includes(event.key)
  return event.key === name
}

/** Runs the first row `event` names; answers whether one took the key. */
export function pressKey(event: KeyEvent, context: KeyContext | null) {
  if (!context || event.nativeEvent?.isComposing || event.isComposing || event.keyCode === 229 || event.key === 'Process') return false
  for (const row of KEYS) {
    if (!row.keys.some((shortcut) => keyMatches(shortcut, event))) continue
    if (row.when && !row.when()) continue
    row.act(context, event)
    return true
  }
  return false
}
