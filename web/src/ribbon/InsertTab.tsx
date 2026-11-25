import { BookBookmarkIcon as BookMarked, CalendarClockIcon as CalendarClock, FileTextIcon as FileText, FilePlusCornerIcon as FilePlus2, ImageIcon as Image, LinkIcon as Link, MessageSquarePlusIcon as MessageSquarePlus, OmegaIcon as Omega, PanelBottomIcon as PanelBottom, PanelTopIcon as PanelTop, PilcrowIcon as Pilcrow, HashIcon as Hash, SigmaIcon as Sigma, SignatureIcon as Signature, FaceSlightlySmilingIcon as Smile, TableIcon as Table, TextCursorInputIcon as TextCursorInput, TypeIcon as Type, WorkflowIcon as Workflow, SeparatorHorizontalIcon as SeparatorHorizontal, BoxIcon as Box, CaseUpperIcon as CaseUpper, PuzzleIcon as Puzzle } from '@kition/tessera/icons/react'
import {
  CommandButton,
  CommandItem,
  ChartInsertMenu,
  DrawingInsertGroup,
  MenuSeparator,
  PICTURE_TYPES,
  RibbonButton,
  RibbonGroup,
  RibbonMenu,
  RibbonStack,
  TableSizePicker,
  base64,
  useHostPick,
  useKitEditor,
} from '@kition/sdk/editor'

const SYMBOLS = ['©', '®', '™', '§', '¶', '€', '£', '¥', '°', '±', '≠', '≤', '≥', '÷', '×', '∞', 'µ', 'α', 'β', 'π', 'Ω', '∑', '√', '→', '←', '✓', '★', '♥', '—', '…']


export function InsertTab() {
  const { run, enabled, ask, phrase: p, t, testId } = useKitEditor()
  const picker = useHostPick()

  const insertPicture = async () => {
    const file = await picker.pick('picture', PICTURE_TYPES)
    if (file) await run('image.insert', { data: base64(file.bytes) })
  }

  return (
    <>
      <RibbonGroup label={p('Pages')} icon={FileText}>
        <RibbonStack>
          <CommandButton command="page.insertCover" icon={FileText} label={p('Cover Page')} />
          <CommandButton command="page.insertBlank" icon={FilePlus2} label={p('Blank Page')} />
          <CommandButton command="page.insertBreak" primary icon={SeparatorHorizontal} label={p('Page Break')} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Tables')} icon={Table}>
        <RibbonMenu icon={Table} label={p('Table')} size="large" disabled={!enabled('table.insert')} testId={`${testId}-table`} primary>
          <TableSizePicker onPick={(rows, cols) => void run('table.insert', { rows, cols })} label={(rows, columns) => (rows ? t('tableSize', { rows, columns }) : t('insertTable'))} />
          <MenuSeparator />
          <CommandItem command="table.fromText" label={p('Convert Text to Table…')} />
          <CommandItem command="table.quick" params={{ kind: 'tabular' }} label={p('Tabular List')} />
          <CommandItem command="table.quick" params={{ kind: 'matrix' }} label={p('Matrix')} />
          <CommandItem command="table.quick" params={{ kind: 'calendar' }} label={p('Calendar')} />
        </RibbonMenu>
      </RibbonGroup>

      <RibbonGroup label={p('Illustrations')} icon={Image}>
        <RibbonButton icon={Image} label={p('Pictures')} size="large" disabled={!picker.can || !enabled('image.insert')} onClick={() => void insertPicture()} data-testid={`${testId}-picture`} primary />
        <RibbonStack>
          <CommandButton command="insert.icon" icon={Smile} label={p('Icons')} />
          <CommandButton command="insert.smartGraphic" icon={Workflow} label={p('Smart graphics')} />
          <ChartInsertMenu testId="doc-chart" size="small" primary />
        </RibbonStack>
      </RibbonGroup>

      <DrawingInsertGroup testId="doc-drawing" label={p('Shapes')} />

      <RibbonGroup label={p('Links')} icon={Link}>
        <RibbonStack>
          <CommandButton command="link.insert" primary icon={Link} label={p('Link')} ask />
          <CommandButton command="bookmark.insert" icon={BookMarked} label={p('Bookmark')} ask />
          <CommandButton command="crossReference.insert" icon={Pilcrow} label={p('Cross-reference')} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Comments')} icon={MessageSquarePlus}>
        <CommandButton command="comment.new" primary icon={MessageSquarePlus} size="large" label={p('Comment')} ask />
      </RibbonGroup>

      <RibbonGroup label={p('Header & Footer')} icon={PanelTop}>
        <RibbonMenu icon={PanelTop} label={p('Header')} size="large" primary>
          <CommandItem command="header.insert" params={{ preset: 'blank' }} label={p('Blank')} />
          <CommandItem command="header.insert" params={{ preset: 'blankThree' }} label={p('Blank (Three Columns)')} />
          <CommandItem command="header.insert" params={{ preset: 'title' }} label={p('Document Title')} />
          <MenuSeparator />
          <CommandItem command="header.edit" label={p('Edit Header')} />
          <CommandItem command="header.remove" label={p('Remove Header')} />
        </RibbonMenu>
        <RibbonMenu icon={PanelBottom} label={p('Footer')} size="large">
          <CommandItem command="footer.insert" params={{ preset: 'blank' }} label={p('Blank')} />
          <CommandItem command="footer.insert" params={{ preset: 'blankThree' }} label={p('Blank (Three Columns)')} />
          <CommandItem command="footer.insert" params={{ preset: 'pageNumber' }} label={p('Page Number')} />
          <MenuSeparator />
          <CommandItem command="footer.edit" label={p('Edit Footer')} />
          <CommandItem command="footer.remove" label={p('Remove Footer')} />
        </RibbonMenu>
        <RibbonMenu icon={Hash} label={p('Page Number')} size="large" primary>
          <CommandItem command="pageNumber.insert" params={{ position: 'top', align: 'right' }} label={p('Top of Page')} />
          <CommandItem command="pageNumber.insert" params={{ position: 'bottom', align: 'center' }} label={p('Bottom of Page')} />
          <CommandItem command="pageNumber.insert" params={{ position: 'bottom', format: 'x of y' }} label={p('Page X of Y')} />
          <CommandItem command="pageNumber.insert" params={{ position: 'current' }} label={p('Current Position')} />
        </RibbonMenu>
      </RibbonGroup>

      <RibbonGroup label={p('Text')} icon={Type}>
        <RibbonStack>
          <RibbonMenu icon={Puzzle} label={p('Quick Parts')}>
            <CommandItem command="field.insert" label={p('Field…')} />
            <CommandItem command="autoText.insert" label={p('AutoText')} />
            <CommandItem command="field.insertDocProperty" label={p('Document Property')} />
          </RibbonMenu>
          <CommandButton command="artText.insert" icon={Type} label={p('Art text')} />
          <CommandButton command="dropCap.set" icon={CaseUpper} label={p('Drop Cap')} />
        </RibbonStack>
        <RibbonStack>
          <CommandButton command="signatureLine.insert" icon={Signature} label={p('Signature Line')} />
          <CommandButton command="field.insertDateTime" icon={CalendarClock} label={p('Date & Time')} />
          <CommandButton command="object.insert" icon={Box} label={p('Object')} />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Symbols')} icon={Omega}>
        <CommandButton command="equation.insert" icon={Sigma} size="large" label={p('Equation')} />
        <RibbonMenu icon={Omega} label={p('Symbol')} size="large" disabled={!enabled('symbol.insert')} testId={`${testId}-symbol`} primary>
          <div className="doc-symbols">
            {SYMBOLS.map((char) => (
              <button key={char} type="button" className="doc-symbol" onClick={() => void run('symbol.insert', { char })} aria-label={char}>
                {char}
              </button>
            ))}
          </div>
          <MenuSeparator />
          <RibbonButton label={p('More Symbols…')} onClick={() => ask('symbol.insert')} />
        </RibbonMenu>
      </RibbonGroup>
    </>
  )
}
