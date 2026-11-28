import type { KitModuleManifest } from '@kition/sdk/editor'
import { canvasColor } from './backdrop'
import { onMac } from './keys'
import kit from '../../kit.json'

/** The engine, from this package's own folder, on the host's canvas color. Called after the theme is applied. */
/** `commentOnly`: a commenter's session (Kit UI protocol `document.comments`): the engine writes only comments. */
export function engineManifest(commentOnly = false): KitModuleManifest {
  const canvas = canvasColor()
  return {
    kitId: 'kit.doc',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_doc_kit',
    documentClass: 'DocDocument',
    // The editor draws pages with their editing look (dimmed headers and footers, squiggles);
    // thumbnails and agents get them as printed.
    setup: [
      { method: 'setEditing', args: [true] },
      // A Mac's keys: Cmd+Left and Cmd+Right go to the ends of the line, Option+Left and
      // Option+Right a word (the engine has no platform of its own).
      ...(onMac() ? [{ method: 'setMac', args: [true] }] : []),
      ...(canvas ? [{ method: 'setBackdrop', args: [canvas] }] : []),
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
