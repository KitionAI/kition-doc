/**
 * File › Export: the engine draws the file (`file.export`, answered as base64) and the host writes
 * it as a new file beside the document (`host/write` mode `export`, Kit UI protocol). The editor
 * runs in the host's page, where it cannot write files itself; only the host's answer says the file
 * was written, and the path it answers is the one the host chose (it never replaces a file).
 */

export type ExportFormat = 'pdf' | 'png'

/** What `file.export` answers. */
export type ExportAnswer = { format: string; extension: string; mime: string; bytes: number; data: string; width?: number; height?: number }

/** What the export needs of the host connection. */
export type ExportHost = {
  init: { document: { path: string; name: string } }
  exportFile: (path: string, bytes: Uint8Array) => Promise<string>
}

/** The engine's `file.export` parameters: the whole document as PDF, one page (1-based) as PNG. */
export function exportParams(format: ExportFormat, page: number): Record<string, unknown> {
  return format === 'png' ? { format, page: Math.max(1, Math.floor(page) || 1) } : { format }
}

/**
 * The new file's workspace path: in the document's folder, named after the document; a page of a
 * document of several pages says which.
 */
export function exportPath(documentPath: string, documentName: string, extension: string, page: number | null = null): string {
  const folder = documentPath.includes('/') ? documentPath.slice(0, documentPath.lastIndexOf('/') + 1) : ''
  const own = documentName || documentPath.slice(documentPath.lastIndexOf('/') + 1)
  const stem = own.replace(/\.[^.]+$/, '').replace(/[\\/:*?"<>|]/g, '_').trim() || 'Document'
  return `${folder}${page == null ? stem : `${stem} - ${page}`}.${extension}`
}

/** The file's bytes from the engine's base64. */
export function decodeBase64(text: string): Uint8Array {
  const binary = atob(text)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index)
  return bytes
}

/** Checks the engine's answer: the format asked for, with its bytes. */
export function parseAnswer(text: string, format: ExportFormat): ExportAnswer {
  const answer = JSON.parse(text) as Partial<ExportAnswer> | null
  if (!answer || typeof answer.data !== 'string' || answer.extension !== format) throw new Error(`the engine answered no ${format.toUpperCase()} file`)
  return answer as ExportAnswer
}

/**
 * Exports the document: the engine's bytes (`execute`, which throws the engine's error), written
 * by the host (which throws the host's). Answers the path the host wrote.
 */
export async function exportThroughHost(execute: (id: string, params: Record<string, unknown>) => Promise<string>, host: ExportHost, format: ExportFormat, page: number, pages: number): Promise<string> {
  const params = exportParams(format, page)
  const answer = parseAnswer(await execute('file.export', params), format)
  const { path, name } = host.init.document
  const numbered = format === 'png' && pages > 1 ? (params.page as number) : null
  return host.exportFile(exportPath(path, name, answer.extension, numbered), decodeBase64(answer.data))
}
