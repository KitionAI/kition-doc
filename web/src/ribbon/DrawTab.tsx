import { CommandTab, InkGroup } from '@kition/sdk/editor'

/** The shared pens (`draw.*`; the pointer draws and erases with `drawing.ink` and `drawing.erase`), then the Draw tab's other commands. */
export function DrawTab() {
  return (
    <>
      <InkGroup testId="doc-ink" />
      <CommandTab menu="Draw" exclude={['draw.select', 'draw.eraser', 'draw.pen', 'draw.pencil', 'draw.highlighter', 'draw.erase', 'draw.stroke', 'draw.state']} />
    </>
  )
}
