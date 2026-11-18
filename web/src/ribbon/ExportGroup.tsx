import { useState } from 'react'
import { FileOutputIcon as FileOutput, ImageDownIcon as ImageDown } from '@kition/tessera-icons/react'
import { RibbonButton, RibbonGroup, toast, useKitEditor } from '@kition/kit-editor'
import { exportThroughHost, type ExportFormat } from '../exportFile'

/**
 * File › Export: the document as PDF, or the current page as PNG, written beside the document as a
 * new file by the host (`exportFile.ts`). Hidden where the host withholds exporting.
 */
export function ExportGroup({ page, pages }: { page: number; pages: number }) {
  const { phrase: p, t, host, session } = useKitEditor()
  const [busy, setBusy] = useState(false)
  if (!host.permissions.export) return null
  const run = async (format: ExportFormat) => {
    const doc = session.doc
    if (!doc || busy) return
    setBusy(true)
    try {
      const written = await exportThroughHost((id, params) => doc.execute(id, params), host, format, page, pages)
      toast('success', t('exported'), written)
    } catch (error) {
      toast('error', t('exportFailed'), error instanceof Error ? error.message : String(error))
    } finally {
      setBusy(false)
    }
  }
  return (
    <RibbonGroup label={p('Export')} icon={FileOutput}>
      <RibbonButton icon={FileOutput} size="large" label={p('Export PDF')} disabled={busy || !session.doc} onClick={() => void run('pdf')} data-testid="doc-export-pdf" primary />
      <RibbonButton icon={ImageDown} size="large" label={p('Export Page as PNG')} disabled={busy || !session.doc} onClick={() => void run('png')} data-testid="doc-export-png" primary />
    </RibbonGroup>
  )
}
