/**
 * The right-click menu on the pages (plan "Doc usable by hand", stage 2A, section 2 row 31;
 * walkthrough R1; WPS W-S1-W-S4). The engine settles the selection first and answers what the menu
 * is for (`contextMenu` in the binding, `context_menu.rs`): text, with or without a selection, a
 * misspelled word, a table's cell, a picture, a shape or a chart. The rows are WPS's, in its order,
 * and each runs a command the ribbon runs too; Font…, Paragraph… and Table Properties… open the
 * editor's dialogs (stage 6A). WPS's Borders and Shading… dialog is the Borders submenu here.
 *
 * It opens at the pointer (or, from Shift+F10 and the menu key, at the caret's foot or the selected
 * object's middle), and closes on Escape, a press outside, a choice, the wheel or the window going
 * away; the keyboard goes back to the text, unless a choice opened a dialog.
 */
import { useEffect } from 'react'
import { CommandItem, ContextMenu, MenuItem, MenuSeparator, SubMenu, useKitEditor, type ContextMenuPoint } from '@kition/sdk/editor'
import type { ClipboardAction } from './ribbon/HomeTab'
import { focusText, keyboardLost } from './focus'

/** A spelling or grammar issue under the press, with the menu's first suggestions (engine `spelling.suggestions`). */
export type MenuIssue = { word: string; kind: 'spelling' | 'grammar'; message: string | null; suggestions: string[]; start: unknown; end: unknown }

/** The engine's answer to a right-click. */
export type MenuAnswer = {
  target: 'text' | 'picture' | 'shape' | 'chart'
  /** Text is selected (not an object). */
  selected: boolean
  /** The caret is in a table's cell. */
  table: boolean
  spelling: MenuIssue | null
  /** The canvas point (document points) the menu opens at. */
  at: [number, number] | null
}

export type OpenMenu = MenuAnswer & { client: ContextMenuPoint }

const BULLETS = ['•', '○', '▪', '◆', '✓', '–']
const NUMBERING: Array<[string, string]> = [
  ['1. 2. 3.', 'numbered'],
  ['1) 2) 3)', 'numberedParen'],
  ['I. II. III.', 'outline'],
  ['A. B. C.', 'upperLetter'],
  ['a) b) c)', 'lowerLetter'],
  ['i. ii. iii.', 'lowerRoman'],
]
const WRAPS = ['inline', 'square', 'tight', 'through', 'topAndBottom', 'behindText', 'inFrontOfText'] as const
const ALIGNS = ['topLeft', 'topCenter', 'topRight', 'centerLeft', 'center', 'centerRight', 'bottomLeft', 'bottomCenter', 'bottomRight'] as const

export function DocContextMenu({ menu, onClose, onClipboard, onNewComment }: {
  menu: OpenMenu | null
  onClose: () => void
  onClipboard: (action: ClipboardAction) => void
  /** Review › New Comment: the host's thread when it keeps them, else the engine's comment. */
  onNewComment: () => void
}) {
  const { run, enabled, t } = useKitEditor()
  // The window going away, or the view scrolled under it: the menu no longer points at its place.
  useEffect(() => {
    if (!menu) return undefined
    const close = () => onClose()
    window.addEventListener('blur', close)
    window.addEventListener('wheel', close, { capture: true, passive: true })
    return () => {
      window.removeEventListener('blur', close)
      window.removeEventListener('wheel', close, { capture: true })
    }
  }, [menu, onClose])
  const close = () => {
    onClose()
    // The keyboard back to the text, unless a choice opened a dialog (it takes the keyboard).
    window.setTimeout(() => {
      if (keyboardLost() && !document.querySelector('.kui-dialog-backdrop')) focusText()
    }, 0)
  }
  const go = (command: string, params: Record<string, unknown> = {}) => void run(command, params)
  const issue = menu?.spelling ?? null
  const object = menu != null && menu.target !== 'text'
  // Cut and Copy are grey with nothing selected (WPS W-S1).
  const nothing = menu != null && !menu.selected && menu.target === 'text'
  const clipboard = (
    <>
      <MenuItem label={t('menuCut')} disabled={nothing || !enabled('edit.cut')} onSelect={() => onClipboard('cut')} testId="doc-menu-cut" />
      <MenuItem label={t('menuCopy')} disabled={nothing || !enabled('edit.copy')} onSelect={() => onClipboard('copy')} testId="doc-menu-copy" />
      <MenuItem label={t('menuPaste')} disabled={!enabled('edit.paste')} onSelect={() => onClipboard('paste')} testId="doc-menu-paste" />
    </>
  )
  return (
    <ContextMenu at={menu?.client ?? null} onClose={close} label={t('contextMenu')} testId="doc-context-menu">
      {menu && issue ? (
        <>
          {issue.suggestions.length ? (
            issue.suggestions.map((word, index) => (
              <MenuItem
                key={word}
                label={word}
                onSelect={() => {
                  // The word's own ends, then the suggestion over it: one undo step.
                  go('select.range', { anchor: issue.start, focus: issue.end })
                  go('spelling.applySuggestion', { text: word })
                }}
                testId={`doc-menu-suggestion-${index}`}
              />
            ))
          ) : (
            <MenuItem label={t('menuNoSuggestions')} disabled onSelect={() => undefined} testId="doc-menu-no-suggestions" />
          )}
          {issue.kind === 'spelling' ? (
            <>
              <MenuItem label={t('menuIgnoreAll')} onSelect={() => go('spelling.ignoreAll', { word: issue.word })} testId="doc-menu-ignore-all" />
              <MenuItem label={t('menuAddToDictionary')} onSelect={() => go('spelling.addToDictionary', { word: issue.word })} testId="doc-menu-add-word" />
            </>
          ) : null}
          <MenuSeparator />
        </>
      ) : null}
      {menu ? clipboard : null}
      {menu && !object ? (
        <>
          <MenuItem label={t('menuKeepTextOnly')} disabled={!enabled('edit.pasteText')} onSelect={() => onClipboard('pasteText')} testId="doc-menu-paste-text" />
          <MenuItem label={t('menuPasteSpecial')} disabled={!enabled('edit.pasteSpecial')} onSelect={() => onClipboard('pasteSpecial')} testId="doc-menu-paste-special" />
          <MenuSeparator />
          <MenuItem label={t('menuFont')} disabled={!enabled('format.set')} onSelect={() => go('format.fontDialog')} testId="doc-menu-font" />
          <MenuItem label={t('menuParagraph')} disabled={!enabled('paragraph.set')} onSelect={() => go('paragraph.dialog')} testId="doc-menu-paragraph" />
          <SubMenu label={t('menuBullets')} path="Context/Bullets" testId="doc-menu-bullets">
            {BULLETS.map((kind) => (
              <CommandItem key={kind} command="paragraph.bullets" params={{ kind }} label={kind} />
            ))}
            <MenuSeparator />
            <CommandItem command="paragraph.bullets" params={{ off: true }} label={t('menuNone')} />
          </SubMenu>
          <SubMenu label={t('menuNumbering')} path="Context/Numbering" testId="doc-menu-numbering">
            {NUMBERING.map(([label, kind]) => (
              <CommandItem key={kind} command="paragraph.numbering" params={{ kind }} label={label} />
            ))}
            <MenuSeparator />
            <CommandItem command="paragraph.restartNumbering" label={t('menuRestartNumbering')} />
            <CommandItem command="paragraph.numbering" params={{ off: true }} label={t('menuNone')} />
          </SubMenu>
          <MenuItem label={t('menuFormatPainter')} disabled={!enabled('edit.formatPainter')} onSelect={() => go('edit.formatPainter')} testId="doc-menu-format-painter" />
        </>
      ) : null}
      {menu?.table && !object ? (
        <>
          <MenuSeparator />
          <SubMenu label={t('menuInsert')} path="Context/Insert" testId="doc-menu-table-insert">
            <CommandItem command="table.insertRowAbove" label={t('menuRowAbove')} />
            <CommandItem command="table.insertRowBelow" label={t('menuRowBelow')} />
            <CommandItem command="table.insertColumnLeft" label={t('menuColumnLeft')} />
            <CommandItem command="table.insertColumnRight" label={t('menuColumnRight')} />
          </SubMenu>
          <SubMenu label={t('menuDelete')} path="Context/Delete" testId="doc-menu-table-delete">
            <CommandItem command="table.deleteCells" label={t('menuDeleteCells')} />
            <CommandItem command="table.deleteRow" label={t('menuDeleteRows')} />
            <CommandItem command="table.deleteColumn" label={t('menuDeleteColumns')} />
            <CommandItem command="table.deleteTable" label={t('menuDeleteTable')} />
          </SubMenu>
          <CommandItem command="table.split" ask label={t('menuSplitCells')} />
          {enabled('table.merge') ? <CommandItem command="table.merge" label={t('menuMergeCells')} /> : null}
          <CommandItem command="table.selectTable" label={t('menuSelectTable')} />
          <SubMenu label={t('menuAutoFit')} path="Context/AutoFit" testId="doc-menu-table-autofit">
            <CommandItem command="table.autofit" params={{ mode: 'contents' }} label={t('menuAutoFitContents')} />
            <CommandItem command="table.autofit" params={{ mode: 'window' }} label={t('menuAutoFitWindow')} />
            <CommandItem command="table.autofit" params={{ mode: 'fixed' }} label={t('menuFixedWidth')} />
            <MenuSeparator />
            <CommandItem command="table.distributeRows" label={t('menuDistributeRows')} />
            <CommandItem command="table.distributeColumns" label={t('menuDistributeColumns')} />
          </SubMenu>
          <SubMenu label={t('menuBorders')} path="Context/Borders" testId="doc-menu-table-borders">
            {(['all', 'outside', 'inside', 'none'] as const).map((kind) => (
              <CommandItem key={kind} command="table.borders" params={{ kind }} label={t(`menuBorders.${kind}`)} />
            ))}
          </SubMenu>
          <SubMenu label={t('menuCellAlignment')} path="Context/CellAlignment" testId="doc-menu-table-align">
            {ALIGNS.map((value) => (
              <CommandItem key={value} command="table.cellAlign" params={{ value }} label={t(`menuAlign.${value}`)} />
            ))}
          </SubMenu>
          <SubMenu label={t('menuTextDirection')} path="Context/TextDirection" testId="doc-menu-table-direction">
            {(['horizontal', 'down', 'up'] as const).map((value) => (
              <CommandItem key={value} command="table.textDirection" params={{ value }} label={t(`menuDirection.${value}`)} />
            ))}
          </SubMenu>
          <MenuItem label={t('menuTableProperties')} disabled={!enabled('table.properties')} onSelect={() => go('table.properties')} testId="doc-menu-table-properties" />
        </>
      ) : null}
      {menu && object ? (
        <>
          <MenuSeparator />
          <MenuItem label={t('menuDeleteObject')} disabled={!enabled('text.delete')} onSelect={() => go('text.delete')} testId="doc-menu-delete-object" />
          {menu.target === 'picture' ? <CommandItem command="image.altText" ask label={t('menuAltText')} /> : null}
          {menu.target !== 'chart' ? <MenuItem label={t('menuSize')} disabled={!enabled('image.size')} onSelect={() => go('image.size')} testId="doc-menu-size" /> : null}
          <CommandItem command="caption.insert" ask label={t('menuCaption')} />
          <MenuSeparator />
          {menu.target === 'shape' ? <CommandItem command="arrange.group" label={t('menuGroup')} /> : null}
          <CommandItem command="arrange.bringForward" label={t('menuBringForward')} />
          <CommandItem command="arrange.sendBackward" label={t('menuSendBackward')} />
          <SubMenu label={t('menuWrapText')} path="Context/WrapText" testId="doc-menu-wrap">
            {WRAPS.map((wrap) => (
              <CommandItem key={wrap} command="arrange.wrap" params={{ wrap }} label={t(`menuWrap.${wrap}`)} />
            ))}
          </SubMenu>
        </>
      ) : null}
      {menu ? (
        <>
          <MenuSeparator />
          <CommandItem command="link.insert" ask label={t('menuHyperlink')} />
          <MenuItem label={t('menuNewComment')} disabled={!enabled('comment.new')} onSelect={onNewComment} testId="doc-menu-new-comment" />
        </>
      ) : null}
    </ContextMenu>
  )
}
