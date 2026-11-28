import { useRef, type KeyboardEvent, type ReactNode } from 'react'
import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, ModalTitle, useKitEditor } from '@kition/sdk/editor'
import { focusText } from '../focus'
import { closeDialog } from './store'

/**
 * The frame every dialog of the editor shares: its title, its fields, OK and Cancel. The first
 * field has the keyboard when it opens; Enter is OK (not in a multi-line field, a button or an
 * input method's composition), Escape is Cancel; closing gives the keyboard back to the text,
 * whose selection is as it was.
 */
export function DialogShell({ title, testId, onOk, okDisabled, children, wide }: { title: string; testId: string; onOk: () => Promise<boolean> | boolean; okDisabled?: boolean; children: ReactNode; wide?: boolean }) {
  const { t } = useKitEditor()
  const body = useRef<HTMLDivElement>(null)
  const busy = useRef(false)
  const ok = async () => {
    if (busy.current || okDisabled) return
    busy.current = true
    try {
      if (await onOk()) closeDialog()
    } finally {
      busy.current = false
    }
  }
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Enter' || event.nativeEvent.isComposing || event.keyCode === 229) return
    if (event.target instanceof HTMLButtonElement || event.target instanceof HTMLTextAreaElement) return
    event.preventDefault()
    void ok()
  }
  return (
    <Modal open onOpenChange={(value) => (value ? undefined : closeDialog())}>
      <ModalContent
        size={wide ? 'md' : 'sm'}
        aria-describedby={undefined}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          // The fields come with the document's values, a moment after the dialog opens.
          let tries = 0
          const first = () => {
            const field = body.current?.querySelector<HTMLElement>('[data-first]') ?? body.current?.querySelector<HTMLElement>('input:not([disabled]), select:not([disabled]), textarea')
            if (field) field.focus()
            else if (tries++ < 30) window.requestAnimationFrame(first)
          }
          window.requestAnimationFrame(first)
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault()
          window.requestAnimationFrame(() => focusText())
        }}
        onKeyDown={keyDown}
        data-testid={testId}
      >
        <ModalHeader>
          <ModalTitle>{title}</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <div className="doc-dialog-form" ref={body}>
            {children}
          </div>
        </ModalBody>
        <ModalFooter className="doc-dialog-actions">
          <Button variant="primary" disabled={okDisabled} onClick={() => void ok()} data-testid={`${testId}-ok`}>
            {t('dialogOk')}
          </Button>
          <Button variant="secondary" onClick={closeDialog} data-testid={`${testId}-cancel`}>
            {t('dialogCancel')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}

/** A labelled row of a dialog. */
export function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="doc-dialog-row">
      <span>{label}</span>
      {children}
    </label>
  )
}

/** A number typed in a field: null when empty or not a number. */
export const numberOf = (text: string) => {
  const value = Number(text.trim().replace(',', '.'))
  return text.trim() !== '' && Number.isFinite(value) ? value : null
}

/** A number for a field, without trailing zeros. */
export const shown = (value: number | null | undefined, digits = 2) => (value == null || !Number.isFinite(value) ? '' : String(Number(value.toFixed(digits))))
