import type { KitModuleManifest } from '@kition/kit-editor'
import kit from '../../kit.json'

/** The engine, from this package's own folder, on the host's canvas color. Called after the theme is applied. */
/** `commentOnly`: a commenter's session (Kit UI protocol `document.comments`): the engine writes only comments. */
export function engineManifest(commentOnly = false): KitModuleManifest {
  const canvas = getComputedStyle(document.documentElement).getPropertyValue('--kit-canvas').trim()
  return {
    kitId: 'kit.doc',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_doc_kit',
    documentClass: 'DocDocument',
    // The editor draws pages with their editing look (dimmed headers and footers, squiggles);
    // thumbnails and agents get them as printed.
    setup: [
      { method: 'setEditing', args: [true] },
      ...(/^#[0-9a-f]{6}$/i.test(canvas) ? [{ method: 'setBackdrop', args: [canvas] }] : []),
      ...(commentOnly ? [{ method: 'setCommentOnly', args: [true] }] : []),
    ],
    // The web build embeds no CJK face: Chinese, and the metric-compatible stand-ins for Office's
    // fonts (decision F1), copied from kition-fonts by build-engine.sh.
    fonts: [
      'DroidSansFallbackFull.ttf',
      'Carlito-Regular.ttf',
      'Carlito-Bold.ttf',
      'Carlito-Italic.ttf',
      'Carlito-BoldItalic.ttf',
      'Caladea-Regular.ttf',
      'Caladea-Bold.ttf',
      'Caladea-Italic.ttf',
      'Caladea-BoldItalic.ttf',
      'Arimo-Variable.ttf',
      'Arimo-Italic-Variable.ttf',
      'Tinos-Regular.ttf',
      'Tinos-Bold.ttf',
      'Tinos-Italic.ttf',
      'Cousine-Regular.ttf',
    ].map((file) => new URL(`./fonts/${file}`, document.baseURI).href),
    // What a commenter's read-only session runs (kit.json `collaboration.comments.commands`).
    commentCommands: kit.collaboration.comments.commands,
  }
}
