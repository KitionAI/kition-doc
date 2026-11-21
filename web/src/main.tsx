import { createRoot } from 'react-dom/client'
import { connectHost, createKitStrings, loadPhrases } from '@kition/kit-editor'
import '@kition/kit-editor/styles.css'
import './app.css'
import { App } from './App'
import { CATALOGS } from './strings'
import manifest from '../../kit.json'
import './commandIcons'

const root = createRoot(document.getElementById('root')!)

// Opened outside a host, the page says where it belongs instead of waiting forever.
const alone = window.setTimeout(() => {
  const t = createKitStrings(CATALOGS, navigator.language)
  root.render(<div className="kui-alone">{t('noHost')}</div>)
}, 3000)

// The upstream editor's translations of its English labels and command titles.
const PHRASES = import.meta.glob('./i18n/upstream/*.tsv', { query: '?raw', import: 'default' })

// `comment-anchors` (Kit UI protocol section 7k): with a host that keeps comment threads, the kit anchors them (`HostComments.tsx`).
void connectHost({ id: 'kit.doc', version: '0.1.3', capabilities: ['ai', 'replica', 'presence', 'comment-anchors'], brand: manifest.ui.brand, display: manifest.ui.display }).then(async (host) => {
  window.clearTimeout(alone)
  const phrase = await loadPhrases(PHRASES, host.init.locale)
  // No StrictMode: its doubled effects would open the document twice, in two engine workers.
  root.render(<App host={host} phrase={phrase} />)
})
