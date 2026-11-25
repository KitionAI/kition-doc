/**
 * Tessera icons for the Doc engine's commands that kit-editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/sdk/editor'
import { ALargeSmallIcon, ArrowLeftRightIcon, BookmarkIcon, CaseLowerIcon, CaseUpperIcon, ContactIcon, EraserIcon, EyeOffIcon, GaugeIcon, HighlighterIcon, LibraryIcon, LinkIcon, ListChecksIcon, MailIcon, MoveIcon, PackageIcon, PuzzleIcon, QuoteIcon, RepeatIcon, RotateCwIcon, StampIcon, StarIcon, StrikethroughIcon, TerminalIcon, TypeIcon, UnderlineIcon, WandSparklesIcon } from '@kition/tessera/icons/react'

registerCommandIcons({
  "arrange.position": MoveIcon,
  "arrange.rotation": RotateCwIcon,
  "citation.sources": LibraryIcon,
  "crossReference.insert": LinkIcon,
  "dropCap.set": ALargeSmallIcon,
  "edit.repeat": RepeatIcon,
  "format.allCaps": CaseUpperIcon,
  "format.doubleStrikethrough": StrikethroughIcon,
  "format.doubleUnderline": UnderlineIcon,
  "format.emboss": TypeIcon,
  "format.engrave": TypeIcon,
  "format.hidden": EyeOffIcon,
  "format.shadow": TypeIcon,
  "format.smallCaps": CaseLowerIcon,
  "index.markEntry": BookmarkIcon,
  "ink.eraser": EraserIcon,
  "macro.list": TerminalIcon,
  "mailMerge.addressBlock": ContactIcon,
  "mailMerge.envelopes": MailIcon,
  "mailMerge.finish": MailIcon,
  "mailMerge.highlightFields": HighlighterIcon,
  "mailMerge.matchFields": ArrowLeftRightIcon,
  "mailMerge.rules": ListChecksIcon,
  "object.insert": PackageIcon,
  "page.watermark": StampIcon,
  "quickPart.insert": PuzzleIcon,
  "readAloud.speed": GaugeIcon,
  "tableOfAuthorities.markCitation": QuoteIcon,
  "theme.effects": WandSparklesIcon,
  "theme.setDefault": StarIcon,
})
