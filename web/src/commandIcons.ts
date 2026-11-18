/**
 * Tessera icons for the Doc engine's commands that kit-editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/kit-editor'
import { ALargeSmallIcon, ArrowLeftRightIcon, BookmarkIcon, CaseLowerIcon, CaseUpperIcon, ContactIcon, EraserIcon, EyeOffIcon, GaugeIcon, HighlighterIcon, LibraryIcon, LinkIcon, ListChecksIcon, MailIcon, MoveIcon, PackageIcon, PuzzleIcon, QuoteIcon, RepeatIcon, RotateCwIcon, StampIcon, StarIcon, StrikethroughIcon, TerminalIcon, TypeIcon, UnderlineIcon, WandSparklesIcon } from '@kition/tessera-icons/react'

registerCommandIcons({
  "arrange.position": MoveIcon,
  "arrange.rotation": RotateCwIcon,
  "design.effects": WandSparklesIcon,
  "design.setDefault": StarIcon,
  "design.watermark": StampIcon,
  "draw.eraser": EraserIcon,
  "edit.repeat": RepeatIcon,
  "format.allCaps": CaseUpperIcon,
  "format.doubleStrikethrough": StrikethroughIcon,
  "format.doubleUnderline": UnderlineIcon,
  "format.emboss": TypeIcon,
  "format.engrave": TypeIcon,
  "format.hidden": EyeOffIcon,
  "format.shadow": TypeIcon,
  "format.smallCaps": CaseLowerIcon,
  "insert.crossReference": LinkIcon,
  "insert.dropCap": ALargeSmallIcon,
  "insert.object": PackageIcon,
  "insert.quickParts": PuzzleIcon,
  "mailings.addressBlock": ContactIcon,
  "mailings.envelopes": MailIcon,
  "mailings.finish": MailIcon,
  "mailings.highlightFields": HighlighterIcon,
  "mailings.matchFields": ArrowLeftRightIcon,
  "mailings.rules": ListChecksIcon,
  "readAloud.speed": GaugeIcon,
  "references.markCitation": QuoteIcon,
  "references.markEntry": BookmarkIcon,
  "references.sources": LibraryIcon,
  "tools.macros": TerminalIcon,
})
