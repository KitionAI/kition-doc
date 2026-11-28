import { Columns2Icon as Columns2, FileDigitIcon as FileDigit, RulerIcon as Ruler, RectangleHorizontalIcon as RectangleHorizontal, ScalingIcon as Scaling, SeparatorHorizontalIcon as SeparatorHorizontal, TextWrapIcon as WrapText } from '@kition/tessera/icons/react'
import { CommandButton, CommandItem, CommandTab, RibbonGroup, RibbonMenu, RibbonStack, useKitEditor } from '@kition/sdk/editor'
import { DOC_PRIMARY } from '../doc'

const MARGINS: Array<[string, string]> = [
  ['Normal', 'normal'],
  ['Narrow', 'narrow'],
  ['Moderate', 'moderate'],
  ['Wide', 'wide'],
  ['Mirrored', 'mirrored'],
  ['Office 2003 Default', 'office2003'],
]

const SIZES = ['Letter', 'Legal', 'Executive', 'A3', 'A4', 'A5', 'B5']

const BREAKS: Array<[string, string]> = [
  ['Page', 'page'],
  ['Column', 'column'],
  ['Text Wrapping', 'textWrapping'],
  ['Next Page', 'nextPage'],
  ['Continuous', 'continuous'],
  ['Even Page', 'evenPage'],
  ['Odd Page', 'oddPage'],
]

export function LayoutTab() {
  const { phrase: p } = useKitEditor()
  return (
    <>
      <RibbonGroup label={p('Page Setup')} icon={Ruler}>
        <RibbonMenu icon={Ruler} label={p('Margins')} size="large" testId="doc-margins" primary>
          {MARGINS.map(([label, preset]) => (
            <CommandItem key={preset} command="page.margins" params={{ preset }} label={p(label)} />
          ))}
        </RibbonMenu>
        <RibbonMenu icon={RectangleHorizontal} label={p('Orientation')} size="large" testId="doc-orientation" primary>
          <CommandItem command="page.orientation" params={{ value: 'portrait' }} label={p('Portrait')} />
          <CommandItem command="page.orientation" params={{ value: 'landscape' }} label={p('Landscape')} />
        </RibbonMenu>
        <RibbonMenu icon={Scaling} label={p('Size')} size="large" testId="doc-page-size" primary>
          {SIZES.map((name) => (
            <CommandItem key={name} command="page.size" params={{ name }} label={name} />
          ))}
        </RibbonMenu>
        <RibbonMenu icon={Columns2} label={p('Columns')} size="large" testId="doc-columns" primary>
          <CommandItem command="page.columns" params={{ count: 1 }} label={p('One')} />
          <CommandItem command="page.columns" params={{ count: 2 }} label={p('Two')} />
          <CommandItem command="page.columns" params={{ count: 3 }} label={p('Three')} />
          <CommandItem command="page.columns" params={{ preset: 'left' }} label={p('Left')} />
          <CommandItem command="page.columns" params={{ preset: 'right' }} label={p('Right')} />
        </RibbonMenu>
        <RibbonStack>
          <RibbonMenu icon={SeparatorHorizontal} label={p('Breaks')} testId="doc-breaks" primary>
            {BREAKS.map(([label, kind]) => (
              <CommandItem key={kind} command="page.break" params={{ kind }} label={p(label)} />
            ))}
          </RibbonMenu>
          <CommandButton command="page.lineNumbers" icon={FileDigit} label={p('Line Numbers')} ask />
          <RibbonMenu icon={WrapText} label={p('Hyphenation')}>
            <CommandItem command="page.hyphenation" params={{ value: true }} label={p('Automatic')} />
            <CommandItem command="page.hyphenation" params={{ value: false }} label={p('None')} />
          </RibbonMenu>
        </RibbonStack>
      </RibbonGroup>
      <CommandTab menu="Layout" exclude={['page.margins', 'page.orientation', 'page.size', 'page.columns', 'page.break', 'page.lineNumbers', 'page.hyphenation', 'page.pageSetup', 'page.section', 'text.columnBreak']} primary={DOC_PRIMARY.Layout} />
    </>
  )
}
