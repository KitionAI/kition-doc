import { type MutableRefObject } from 'react'
import { PICTURE_TYPES, base64, useHostPick, useKitEditor } from '@kition/sdk/editor'
import { FontDialog } from './FontDialog'
import { LinkDialog } from './LinkDialog'
import { ParagraphDialog } from './ParagraphDialog'
import { PictureSize } from './PictureSize'
import { TableProperties } from './TableProperties'
import { WordCount } from './WordCount'
import { useDocDialog } from './store'

/**
 * The editor's open dialog (`store.ts`), started again from the document at each opening; and
 * `pick`, Insert › Pictures' file picker (the host's), for the engine's request of it.
 */
export function DocDialogs({ pick }: { pick: MutableRefObject<() => void> }) {
  const { kind, count } = useDocDialog()
  const { run } = useKitEditor()
  const picker = useHostPick()
  pick.current = () => {
    if (!picker.can) return
    void picker.pick('picture', PICTURE_TYPES).then((file) => (file ? run('image.insert', { data: base64(file.bytes) }) : null))
  }
  switch (kind) {
    case 'font':
      return <FontDialog key={count} />
    case 'paragraph':
      return <ParagraphDialog key={count} />
    case 'link':
      return <LinkDialog key={count} />
    case 'wordCount':
      return <WordCount key={count} />
    case 'tableProperties':
      return <TableProperties key={count} />
    case 'pictureSize':
      return <PictureSize key={count} />
    default:
      return null
  }
}
