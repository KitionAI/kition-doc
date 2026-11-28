import { useEffect, useState } from 'react'

/**
 * The editor's own dialogs (plan "Doc usable by hand", stage 6A): which one is open. A dialog is
 * the person's way of filling a command's parameters; OK runs that one command (one undo step),
 * Cancel and Escape run nothing. Only one is open at a time.
 */
export type DocDialogKind = 'font' | 'paragraph' | 'link' | 'wordCount' | 'tableProperties' | 'pictureSize'

let open: DocDialogKind | null = null
let opened = 0
const listeners = new Set<(open: { kind: DocDialogKind | null; count: number }) => void>()

/** Opens `kind` (another open dialog closes). */
export function openDialog(kind: DocDialogKind) {
  open = kind
  opened += 1
  for (const listener of listeners) listener({ kind, count: opened })
}

export function closeDialog() {
  open = null
  for (const listener of listeners) listener({ kind: null, count: opened })
}

/** The open dialog and how many times one was opened (a dialog starts again from the document's values at each opening). */
export function useDocDialog() {
  const [state, setState] = useState<{ kind: DocDialogKind | null; count: number }>({ kind: open, count: opened })
  useEffect(() => {
    listeners.add(setState)
    return () => {
      listeners.delete(setState)
    }
  }, [])
  return state
}
