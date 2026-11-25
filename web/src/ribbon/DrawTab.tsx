import { CommandTab, InkGroup } from '@kition/sdk/editor'

/** The shared pens (`ink.*`; the pointer draws and erases with `drawing.ink` and `drawing.erase`), then the Draw tab's other commands. */
export function DrawTab() {
  return (
    <>
      <InkGroup testId="doc-ink" />
      <CommandTab menu="Draw" exclude={['ink.select', 'ink.eraser', 'ink.pen', 'ink.pencil', 'ink.highlighter', 'ink.erase', 'ink.stroke', 'ink.state']} />
    </>
  )
}
