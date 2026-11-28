import { TextAlignCenterIcon as AlignCenter, TextAlignJustifyIcon as AlignJustify, TextAlignStartIcon as AlignLeft, TextAlignEndIcon as AlignRight, ArrowDownAZIcon as ArrowDownAZ, BaselineIcon as Baseline, BoldIcon as Bold, CaseSensitiveIcon as CaseSensitive, ClipboardPasteIcon as ClipboardPaste, CopyIcon as Copy, EraserIcon as Eraser, HighlighterIcon as Highlighter, ListIndentDecreaseIcon as IndentDecrease, ListIndentIncreaseIcon as IndentIncrease, ItalicIcon as Italic, ListIcon as List, ListOrderedIcon as ListOrdered, ListTreeIcon as ListTree, PaintBucketIcon as PaintBucket, PaintbrushIcon as Paintbrush, PilcrowIcon as Pilcrow, ReplaceIcon as Replace, ScissorsIcon as Scissors, SearchIcon as Search, SquareDashedIcon as SquareDashed, StrikethroughIcon as Strikethrough, SubscriptIcon as Subscript, SuperscriptIcon as Superscript, TypeIcon as Type, UnderlineIcon as Underline, UnfoldVerticalIcon as UnfoldVertical, Grid2x2Icon as Grid2x2, MousePointer2Icon as MousePointer2, AArrowUpIcon as AArrowUp, AArrowDownIcon as AArrowDown, HeadingIcon as Heading } from '@kition/tessera/icons/react'
import {
  ColorGrid,
  CommandButton,
  CommandItem,
  MenuItem,
  MenuSeparator,
  RibbonButton,
  RibbonGroup,
  RibbonItem,
  RibbonMenu,
  RibbonRow,
  RibbonStack,
  useEngineQuery,
  useKitEditor,
} from '@kition/sdk/editor'
import { FONTS, SIZES, type FormatState, type StyleEntry, type ViewState } from '../doc'
import { closeLinePanels } from '../focus'

/** What the clipboard's buttons do: the system clipboard (`clipboard.ts`), as the keys do. */
export type ClipboardAction = 'copy' | 'cut' | 'paste' | 'pasteText' | 'pasteSpecial'

const hex = (color: string) => color.replace(/^#/, '').toUpperCase()

const HIGHLIGHTS: Array<[string, string]> = [
  ['yellow', '#FFFF00'],
  ['brightGreen', '#00FF00'],
  ['turquoise', '#00FFFF'],
  ['pink', '#FF00FF'],
  ['blue', '#0000FF'],
  ['red', '#FF0000'],
  ['darkBlue', '#000080'],
  ['teal', '#008080'],
  ['green', '#008000'],
  ['violet', '#800080'],
  ['darkRed', '#800000'],
  ['darkYellow', '#808000'],
  ['gray50', '#808080'],
  ['gray25', '#C0C0C0'],
  ['black', '#000000'],
]

export function HomeTab({ onFind, onClipboard }: { onFind: (replace: boolean) => void; onClipboard: (action: ClipboardAction) => void }) {
  const { run, enabled, phrase: p, testId } = useKitEditor()
  const format = useEngineQuery<FormatState>('format.state') ?? {}
  const view = useEngineQuery<ViewState>('view.state') ?? {}
  const styles = (useEngineQuery<StyleEntry[]>('style.list') ?? []).filter((style) => style.quick && style.kind === 'paragraph')
  const fonts = format.font && !FONTS.includes(format.font) ? [format.font, ...FONTS] : FONTS
  const size = format.size ?? null

  return (
    <>
      <RibbonGroup label={p('Clipboard')} icon={ClipboardPaste}>
        <RibbonMenu icon={ClipboardPaste} label={p('Paste')} size="large" testId={`${testId}-paste`} primary>
          <MenuItem label={p('Paste')} disabled={!enabled('edit.paste')} onSelect={() => onClipboard('paste')} testId="doc-paste-keep" />
          <MenuItem label={p('Keep Text Only')} disabled={!enabled('edit.pasteText')} onSelect={() => onClipboard('pasteText')} testId="doc-paste-text" />
          <MenuItem label={p('Paste Special')} disabled={!enabled('edit.pasteSpecial')} onSelect={() => onClipboard('pasteSpecial')} testId="doc-paste-special" />
        </RibbonMenu>
        <RibbonStack>
          <RibbonButton icon={Scissors} label={p('Cut')} disabled={!enabled('edit.cut')} onClick={() => onClipboard('cut')} data-testid="doc-cmd-edit.cut" />
          <RibbonButton icon={Copy} label={p('Copy')} disabled={!enabled('edit.copy')} onClick={() => onClipboard('copy')} data-testid="doc-cmd-edit.copy" />
          <CommandButton command="edit.formatPainter" icon={Paintbrush} label={p('Format Painter')} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Font')} icon={Type}>
        <RibbonStack>
          <RibbonRow>
            <RibbonItem primary>
              <select
                className="kui-ribbon-field"
                style={{ width: 150 }}
                value={format.font ?? ''}
                aria-label={p('Font')}
                disabled={!enabled('format.fontName')}
                onChange={(event) => void run('format.fontName', { name: event.target.value })}
                data-testid="doc-font"
              >
                {fonts.map((font) => (
                  <option key={font} value={font}>
                    {font}
                  </option>
                ))}
              </select>
            </RibbonItem>
            <RibbonItem primary>
              <select
                className="kui-ribbon-field"
                style={{ width: 58 }}
                value={size ?? ''}
                aria-label={p('Font Size')}
                disabled={!enabled('format.fontSize')}
                onChange={(event) => void run('format.fontSize', { size: Number(event.target.value) })}
                data-testid="doc-size"
              >
                {size != null && !SIZES.includes(size) ? <option value={size}>{size}</option> : null}
                {size == null ? <option value="" /> : null}
                {SIZES.map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </select>
            </RibbonItem>
            <CommandButton command="format.increaseFontSize" icon={AArrowUp} size="icon" label={p('Increase Font Size')} title={p('Increase Font Size')} />
            <CommandButton command="format.decreaseFontSize" icon={AArrowDown} size="icon" label={p('Decrease Font Size')} title={p('Decrease Font Size')} />
            <RibbonMenu icon={CaseSensitive} label={p('Change Case')} size="icon">
              <CommandItem command="format.changeCase" params={{ mode: 'sentence' }} label={p('Sentence case.')} />
              <CommandItem command="format.changeCase" params={{ mode: 'lower' }} label={p('lowercase')} />
              <CommandItem command="format.changeCase" params={{ mode: 'upper' }} label={p('UPPERCASE')} />
              <CommandItem command="format.changeCase" params={{ mode: 'title' }} label={p('Capitalize Each Word')} />
              <CommandItem command="format.changeCase" params={{ mode: 'toggle' }} label={p('tOGGLE cASE')} />
            </RibbonMenu>
            <CommandButton command="format.clear" icon={Eraser} size="icon" label={p('Clear All Formatting')} title={p('Clear All Formatting')} />
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.bold" primary icon={Bold} size="icon" pressed={format.bold} label={p('Bold')} title={p('Bold')} />
            <CommandButton command="format.italic" primary icon={Italic} size="icon" pressed={format.italic} label={p('Italic')} title={p('Italic')} />
            <CommandButton command="format.underline" primary icon={Underline} size="icon" pressed={format.underline} label={p('Underline')} title={p('Underline')} />
            <CommandButton command="format.strikethrough" icon={Strikethrough} size="icon" pressed={format.strike} label={p('Strikethrough')} title={p('Strikethrough')} />
            <CommandButton command="format.subscript" icon={Subscript} size="icon" pressed={format.subscript} label={p('Subscript')} title={p('Subscript')} />
            <CommandButton command="format.superscript" icon={Superscript} size="icon" pressed={format.superscript} label={p('Superscript')} title={p('Superscript')} />
            <CommandButton command="format.border" icon={SquareDashed} size="icon" pressed={format.border} label={p('Character Border')} title={p('Character Border')} />
            <RibbonMenu icon={Type} label={p('Text Effects and Typography')} size="icon">
              <CommandItem command="format.outline" label={p('Outline')} />
              <CommandItem command="format.shadow" label={p('Shadow')} />
              <CommandItem command="format.smallCaps" label={p('Small Caps')} />
              <CommandItem command="format.allCaps" label={p('All Caps')} />
              <CommandItem command="format.doubleStrikethrough" label={p('Double Strikethrough')} />
            </RibbonMenu>
            <RibbonMenu icon={Highlighter} label={p('Text Highlight Color')} size="icon" disabled={!enabled('format.highlight')} testId="doc-highlight" primary>
              <ColorGrid colors={HIGHLIGHTS.map(([, color]) => color)} onPick={(color) => void run('format.highlight', { color: HIGHLIGHTS.find(([, value]) => value === color)?.[0] ?? 'yellow' })} />
              <CommandItem command="format.highlight" params={{ color: 'none' }} label={p('No Color')} />
            </RibbonMenu>
            <RibbonMenu icon={Baseline} label={p('Font Color')} size="icon" disabled={!enabled('format.color')} testId="doc-font-color" primary>
              <CommandItem command="format.color" params={{ color: 'auto' }} label={p('Automatic')} />
              <ColorGrid onPick={(color) => void run('format.color', { color: hex(color) })} testId="doc-font-colors" />
            </RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Paragraph')} icon={Pilcrow}>
        <RibbonStack>
          <RibbonRow>
            <RibbonMenu icon={List} label={p('Bullets')} size="icon" testId="doc-bullets" primary>
              {['•', '○', '▪', '◆', '✓', '–'].map((kind) => (
                <CommandItem key={kind} command="paragraph.bullets" params={{ kind }} label={kind} />
              ))}
              <MenuSeparator />
              <CommandItem command="paragraph.bullets" params={{ off: true }} label={p('None')} />
            </RibbonMenu>
            <RibbonMenu icon={ListOrdered} label={p('Numbering')} size="icon" testId="doc-numbering" primary>
              {[
                ['1. 2. 3.', 'numbered'],
                ['1) 2) 3)', 'numberedParen'],
                ['I. II. III.', 'outline'],
                ['A. B. C.', 'upperLetter'],
                ['a) b) c)', 'lowerLetter'],
                ['i. ii. iii.', 'lowerRoman'],
              ].map(([label, kind]) => (
                <CommandItem key={kind} command="paragraph.numbering" params={{ kind }} label={label} />
              ))}
              <MenuSeparator />
              <CommandItem command="paragraph.restartNumbering" label={p('Restart at 1')} />
              <CommandItem command="paragraph.numbering" params={{ off: true }} label={p('None')} />
            </RibbonMenu>
            <RibbonMenu icon={ListTree} label={p('Multilevel List')} size="icon">
              <CommandItem command="paragraph.multilevel" params={{ kind: 'legal' }} label="1. 1.1. 1.1.1." />
              <CommandItem command="paragraph.multilevel" params={{ kind: 'outline' }} label="I. A. 1. a." />
              <MenuSeparator />
              {[0, 1, 2, 3, 4].map((level) => (
                <CommandItem key={level} command="paragraph.listLevel" params={{ level }} label={p('Change to Level {n}').replace('{n}', String(level + 1))} />
              ))}
            </RibbonMenu>
            <CommandButton command="paragraph.outdent" icon={IndentDecrease} size="icon" label={p('Decrease Indent')} title={p('Decrease Indent')} />
            <CommandButton command="paragraph.indent" icon={IndentIncrease} size="icon" label={p('Increase Indent')} title={p('Increase Indent')} />
            <CommandButton command="paragraph.sort" icon={ArrowDownAZ} size="icon" label={p('Sort')} title={p('Sort')} />
            <CommandButton command="view.marks" icon={Pilcrow} size="icon" pressed={view.marks} label={p('Show/Hide ¶')} title={p('Show/Hide ¶')} />
          </RibbonRow>
          <RibbonRow>
            <CommandButton command="format.alignLeft" primary icon={AlignLeft} size="icon" label={p('Align Left')} title={p('Align Left')} />
            <CommandButton command="format.alignCenter" primary icon={AlignCenter} size="icon" label={p('Center')} title={p('Center')} />
            <CommandButton command="format.alignRight" icon={AlignRight} size="icon" label={p('Align Right')} title={p('Align Right')} />
            <CommandButton command="format.justify" icon={AlignJustify} size="icon" label={p('Justify')} title={p('Justify')} />
            <RibbonMenu icon={UnfoldVertical} label={p('Line and Paragraph Spacing')} size="icon">
              {[1, 1.15, 1.5, 2, 2.5, 3].map((value) => (
                <CommandItem key={value} command="paragraph.lineSpacing" params={{ value }} label={String(value)} />
              ))}
              <MenuSeparator />
              <CommandItem command="paragraph.addSpaceBefore" label={p('Add Space Before Paragraph')} />
              <CommandItem command="paragraph.removeSpaceAfter" label={p('Remove Space After Paragraph')} />
            </RibbonMenu>
            <RibbonMenu icon={PaintBucket} label={p('Shading')} size="icon" disabled={!enabled('paragraph.shading')}>
              <CommandItem command="paragraph.shading" params={{ color: null }} label={p('No Color')} />
              <ColorGrid onPick={(color) => void run('paragraph.shading', { color: hex(color) })} />
            </RibbonMenu>
            <RibbonMenu icon={Grid2x2} label={p('Borders')} size="icon">
              {[
                ['Bottom Border', 'bottom'],
                ['Top Border', 'top'],
                ['Left Border', 'left'],
                ['Right Border', 'right'],
                ['No Border', 'none'],
                ['All Borders', 'all'],
                ['Outside Borders', 'outside'],
                ['Inside Borders', 'inside'],
              ].map(([label, kind]) => (
                <CommandItem key={kind} command="paragraph.borders" params={{ kind }} label={p(label)} />
              ))}
              <MenuSeparator />
              <CommandItem command="horizontalLine.insert" label={p('Horizontal Line')} />
            </RibbonMenu>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Styles')} icon={Heading}>
        <div className="kui-gallery" data-testid="doc-styles">
          {styles.slice(0, 8).map((style) => (
            <button
              key={style.id}
              type="button"
              className="kui-gallery-item doc-style"
              data-style={style.id}
              aria-pressed={format.styleName === style.name}
              disabled={!enabled('paragraph.style')}
              onClick={() => {
                void run('paragraph.style', { style: style.id })
                // In Single line the gallery is a panel: a choice closes it, as a menu's does.
                closeLinePanels()
              }}
              data-testid={`doc-style-${style.id}`}
            >
              {p(style.name)}
            </button>
          ))}
        </div>
      </RibbonGroup>

      <RibbonGroup label={p('Editing')} icon={Search}>
        <RibbonStack>
          <RibbonButton icon={Search} label={p('Find')} onClick={() => onFind(false)} data-testid="doc-find" primary />
          <RibbonButton icon={Replace} label={p('Replace')} onClick={() => onFind(true)} data-testid="doc-replace" />
          <RibbonMenu icon={MousePointer2} label={p('Select')}>
            <CommandItem command="edit.selectAll" label={p('Select All')} />
            <CommandItem command="select.paragraph" label={p('Select Paragraph')} />
            <CommandItem command="select.sentence" label={p('Select Sentence')} />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>
    </>
  )
}
