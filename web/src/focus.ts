/**
 * Where the keyboard goes when nothing in the editor holds it (plan "Doc usable by hand", stage
 * 0A; walkthrough K1, K2). Typing reaches the document through kit-editor's text input
 * (`.kui-text-sink`), which takes the keyboard back after a press on the pages, but only while this
 * page has the focus. A document just opened, the find bar closed, or a dialog gone left the focus
 * on the page's `<body>`, and no key worked until the text was clicked.
 *
 * kit-editor's frame hands a key pressed on the `<body>` to the editor's `keyboardHome` (the text
 * input here) once; what it cannot do is a typed character, which it re-sends as a key event that
 * types nothing. The capture listener here moves the keyboard before such a character lands, so
 * the browser types it into the text input itself.
 */
import { useEffect } from 'react'

/** kit-editor's text input: typing goes into the document from here. */
export function textInput(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.kui-text-sink')
}

/** Whether no element of the editor holds the keyboard (the page's body has it). */
export function keyboardLost() {
  const active = document.activeElement
  return !active || active === document.body || active === document.documentElement || !active.isConnected
}

/** Gives the keyboard to the document's text; answers whether it has it now. */
export function focusText() {
  const input = textInput()
  if (!input) return false
  input.focus({ preventScroll: true })
  return document.activeElement === input
}

/** A control a person types into: the keyboard is theirs there, never taken from it. */
function typingIn(element: Element | null) {
  if (!element) return false
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) return true
  if (element instanceof HTMLElement && element.isContentEditable) return true
  return element instanceof HTMLInputElement && !['button', 'submit', 'reset', 'checkbox', 'radio', 'file', 'range', 'color', 'image'].includes(element.type)
}

/**
 * The document just opened: the person means to write in it, so the text takes the keyboard, as a
 * word processor does with the window it opened. Not while the host has someone typing in one of
 * its own fields (its search box); a host on another origin is asked through this page's focus.
 */
export function takeKeyboardOnOpen() {
  if (!keyboardLost()) return
  let hostIdle = document.hasFocus()
  if (!hostIdle) {
    try {
      const host = window.parent !== window ? window.parent.document : null
      hostIdle = host !== null && !typingIn(host.activeElement)
    } catch {
      hostIdle = false
    }
  }
  if (hostIdle) focusText()
}

/**
 * Closes the Single line group panel a choice was made in (the Styles gallery): the panel closes
 * on Escape, which is all it listens to besides a press outside it.
 */
export function closeLinePanels() {
  if (!document.querySelector('.kui-drop-panel')) return
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', cancelable: true }))
}

/** Whether the key types a character (a letter, a digit, a space), not a command. */
const typesText = (event: KeyboardEvent) => event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey

/**
 * The safety nets. A character typed while the body has the keyboard moves the keyboard to the
 * text first, so the browser types it there (other keys are kit-editor's `keyboardHome`'s). A blur
 * with nowhere to go (the find bar closed, a dialog or a menu gone) gives the text the keyboard back
 * when nothing else takes it; a press on the pages does that through kit-editor already.
 */
export function useKeyboardFallback() {
  useEffect(() => {
    let pressed = false
    const down = () => {
      pressed = true
    }
    const up = () => {
      pressed = false
    }
    const keyDown = (event: KeyboardEvent) => {
      if (!event.isTrusted || event.defaultPrevented || event.isComposing || event.keyCode === 229 || !typesText(event) || !keyboardLost()) return
      if (event.target !== document.body && event.target !== document.documentElement) return
      if (document.querySelector('.kui-dialog-backdrop')) return
      focusText()
    }
    const focusOut = (event: FocusEvent) => {
      if (event.relatedTarget !== null) return
      window.setTimeout(() => {
        if (!pressed && keyboardLost() && document.hasFocus() && !document.querySelector('.kui-dialog-backdrop')) focusText()
      }, 0)
    }
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('pointerup', up, true)
    window.addEventListener('pointercancel', up, true)
    document.addEventListener('keydown', keyDown, true)
    document.addEventListener('focusout', focusOut)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('pointerup', up, true)
      window.removeEventListener('pointercancel', up, true)
      document.removeEventListener('keydown', keyDown, true)
      document.removeEventListener('focusout', focusOut)
    }
  }, [])
}
