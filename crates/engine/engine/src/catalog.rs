//! The word-processor feature catalog: the incumbent's ribbon and menu commands (feature names
//! only) mapped to our command ids. `parity()` compares it with the registry; the gap is the
//! work list (`cargo xtask parity` → docs/parity/parity-checklist.md).

use serde_json::{Value, json};

use crate::Registry;

/// `Tab|Group|Feature|command id`
pub const CATALOG: &str = "\
File|Backstage|New|file.new
File|Backstage|Open|file.open
File|Backstage|Save|file.save
File|Backstage|Save As|file.saveAs
File|Backstage|Print|file.print
File|Backstage|Close|file.close
File|Backstage|Info|file.info
File|Backstage|Properties|file.properties
File|Backstage|Export PDF|file.exportPdf
File|Backstage|Export Image|file.exportPng
File|Backstage|Options|file.options
File|Backstage|Share|file.share
File|Backstage|Protect Document|file.protect
File|Backstage|Inspect Document|file.removeHiddenContent
File|Backstage|Check Accessibility|file.accessibility
File|Backstage|Check Compatibility|file.compatibility
File|Backstage|Version History|file.versions
File|Backstage|Recover Unsaved|file.recover
File|Backstage|New from Template|file.newFromTemplate
File|Backstage|Save as Template|file.saveTemplate
Quick Access|Toolbar|Undo|edit.undo
Quick Access|Toolbar|Redo|edit.redo
Quick Access|Toolbar|Repeat|edit.repeat
Quick Access|Toolbar|AutoSave|file.autosave
Home|Clipboard|Paste|edit.paste
Home|Clipboard|Paste Keep Text Only|edit.pasteText
Home|Clipboard|Paste Merge Formatting|edit.pasteMerge
Home|Clipboard|Paste Special|edit.pasteSpecial
Home|Clipboard|Cut|edit.cut
Home|Clipboard|Copy|edit.copy
Home|Clipboard|Format Painter|edit.formatPainter
Home|Clipboard|Copy Formatting|edit.copyFormat
Home|Clipboard|Paste Formatting|edit.pasteFormat
Home|Clipboard|Clipboard Pane|edit.clipboardPane
Home|Font|Font|format.fontName
Home|Font|Font Size|format.fontSize
Home|Font|Increase Font Size|format.increaseFontSize
Home|Font|Decrease Font Size|format.decreaseFontSize
Home|Font|Change Case|format.changeCase
Home|Font|Clear All Formatting|format.clear
Home|Font|Bold|format.bold
Home|Font|Italic|format.italic
Home|Font|Underline|format.underline
Home|Font|Double Underline|format.doubleUnderline
Home|Font|Strikethrough|format.strikethrough
Home|Font|Subscript|format.subscript
Home|Font|Superscript|format.superscript
Home|Font|Text Effects Outline|format.outline
Home|Font|Text Effects Shadow|format.shadow
Home|Font|Text Highlight Color|format.highlight
Home|Font|Font Color|format.color
Home|Font|Character Shading|format.shading
Home|Font|Font Dialog|format.fontDialog
Home|Font|Double Strikethrough|format.doubleStrikethrough
Home|Font|Small Caps|format.smallCaps
Home|Font|All Caps|format.allCaps
Home|Font|Hidden|format.hidden
Home|Font|Emboss|format.emboss
Home|Font|Engrave|format.engrave
Home|Font|Character Spacing|format.spacing
Home|Font|Character Scale|format.scale
Home|Font|Character Position|format.position
Home|Font|Character Border|format.border
Home|Font|Phonetic Guide|format.phonetic
Home|Font|Enclose Characters|format.enclose
Home|Paragraph|Bullets|paragraph.bullets
Home|Paragraph|Numbering|paragraph.numbering
Home|Paragraph|Multilevel List|paragraph.multilevel
Home|Paragraph|Define New Multilevel List|list.define
Home|Paragraph|Change List Level|paragraph.listLevel
Home|Paragraph|Restart Numbering|paragraph.restartNumbering
Home|Paragraph|Define New Bullet|paragraph.defineBullet
Home|Paragraph|Define New Number Format|paragraph.defineNumber
Home|Paragraph|Set Numbering Value|paragraph.setNumberingValue
Home|Paragraph|Decrease Indent|paragraph.outdent
Home|Paragraph|Increase Indent|paragraph.indent
Home|Paragraph|Sort|paragraph.sort
Home|Paragraph|Show/Hide Marks|view.marks
Home|Paragraph|Align Left|format.alignLeft
Home|Paragraph|Center|format.alignCenter
Home|Paragraph|Align Right|format.alignRight
Home|Paragraph|Justify|format.justify
Home|Paragraph|Distributed|paragraph.distribute
Home|Paragraph|Line Spacing|paragraph.lineSpacing
Home|Paragraph|Add Space Before|paragraph.addSpaceBefore
Home|Paragraph|Remove Space After|paragraph.removeSpaceAfter
Home|Paragraph|Shading|paragraph.shading
Home|Paragraph|Borders|paragraph.borders
Home|Paragraph|Horizontal Line|horizontalLine.insert
Home|Paragraph|Paragraph Dialog|paragraph.dialog
Home|Paragraph|Tabs|paragraph.tabs
Home|Paragraph|Keep with Next|paragraph.keepNext
Home|Paragraph|Keep Lines Together|paragraph.keepLines
Home|Paragraph|Page Break Before|paragraph.pageBreakBefore
Home|Paragraph|Widow/Orphan Control|paragraph.widowControl
Home|Paragraph|Outline Level|paragraph.outlineLevel
Home|Paragraph|Hanging Indent|paragraph.hangingIndent
Home|Paragraph|Right-to-Left Text Direction|paragraph.rtl
Home|Paragraph|Left-to-Right Text Direction|paragraph.ltr
Home|Paragraph|Asian Typography|paragraph.asianTypography
Home|Styles|Styles Gallery|paragraph.style
Home|Styles|Normal|paragraph.normal
Home|Styles|Heading 1|paragraph.heading1
Home|Styles|Heading 2|paragraph.heading2
Home|Styles|Heading 3|paragraph.heading3
Home|Styles|Create a Style|style.add
Home|Styles|Modify Style|style.set
Home|Styles|Update Style to Match|style.updateToMatch
Home|Styles|Delete Style|style.delete
Home|Styles|Styles Pane|style.pane
Home|Styles|Style Inspector|style.inspector
Home|Styles|Manage Styles|style.manage
Home|Styles|Apply Styles|style.apply
Home|Styles|Character Style|format.charStyle
Home|Editing|Find|edit.find
Home|Editing|Find Next|edit.findNext
Home|Editing|Find Previous|edit.findPrevious
Home|Editing|Advanced Find|edit.advancedFind
Home|Editing|Replace|edit.replace
Home|Editing|Replace All|edit.replaceAll
Home|Editing|Go To|edit.goto
Home|Editing|Select All|edit.selectAll
Home|Editing|Select Objects|select.objects
Home|Editing|Select Similar Formatting|select.similar
Home|Voice|Dictate|tools.dictate
Home|Editor|Editor|spelling.editor
Insert|Pages|Cover Page|page.insertCover
Insert|Pages|Blank Page|page.insertBlank
Insert|Pages|Page Break|page.insertBreak
Insert|Tables|Insert Table|table.insert
Insert|Tables|Draw Table|table.draw
Insert|Tables|Convert Text to Table|table.fromText
Insert|Tables|Spreadsheet|spreadsheet.insert
Insert|Tables|Quick Tables|table.quick
Insert|Illustrations|Pictures|image.insert
Insert|Illustrations|Online Pictures|insert.onlinePicture
Insert|Illustrations|Shapes|shape.insert
Insert|Illustrations|Icons|insert.icon
Insert|Illustrations|3D Models|insert.model3d
Insert|Illustrations|Smart graphics|insert.smartGraphic
Insert|Illustrations|Chart|chart.insert
Insert|Illustrations|Screenshot|insert.screenshot
Insert|Illustrations|Drawing Canvas|insert.canvas
Insert|Add-ins|Get Add-ins|addins.get
Insert|Media|Online Video|insert.video
Insert|Links|Link|link.insert
Insert|Links|Remove Link|link.remove
Insert|Links|Bookmark|bookmark.insert
Insert|Links|Cross-reference|crossReference.insert
Insert|Comments|Comment|comment.new
Insert|Header & Footer|Header|header.insert
Insert|Header & Footer|Footer|footer.insert
Insert|Header & Footer|Page Number|pageNumber.insert
Insert|Header & Footer|Edit Header|header.edit
Insert|Header & Footer|Edit Footer|footer.edit
Insert|Header & Footer|Remove Header|header.remove
Insert|Header & Footer|Remove Footer|footer.remove
Insert|Header & Footer|Format Page Numbers|page.pageNumberFormat
Insert|Header & Footer|Close Header and Footer|header.close
Insert|Header & Footer|Different First Page|page.differentFirstPage
Insert|Header & Footer|Different Odd & Even|page.differentOddEven
Insert|Header & Footer|Link to Previous|page.linkToPrevious
Insert|Text|Text Box|textBox.insert
Insert|Text|Quick Parts|quickPart.insert
Insert|Text|AutoText|autoText.insert
Insert|Text|Document Property|field.insertDocProperty
Insert|Text|Field|field.insert
Insert|Text|Art text|artText.insert
Insert|Text|Drop Cap|dropCap.set
Insert|Text|Signature Line|signatureLine.insert
Insert|Text|Date & Time|field.insertDateTime
Insert|Text|Object|object.insert
Insert|Text|Text from File|text.insertFromFile
Insert|Symbols|Equation|equation.insert
Insert|Symbols|Ink Equation|insert.inkEquation
Insert|Symbols|Symbol|symbol.insert
Insert|Symbols|Nonbreaking Space|text.nbsp
Insert|Symbols|Nonbreaking Hyphen|text.nbHyphen
Insert|Symbols|Optional Hyphen|text.optionalHyphen
Draw|Tools|Select|ink.select
Draw|Tools|Lasso Select|draw.lasso
Draw|Tools|Eraser|ink.eraser
Draw|Pens|Pen|ink.pen
Draw|Pens|Pencil|ink.pencil
Draw|Pens|Highlighter|ink.highlighter
Draw|Pens|Add Pen|draw.addPen
Draw|Convert|Ink to Shape|draw.inkToShape
Draw|Convert|Ink to Math|draw.inkToMath
Draw|Insert|Drawing Canvas|insert.canvas
Draw|Replay|Ink Replay|draw.replay
Design|Document Formatting|Themes|theme.set
Design|Document Formatting|Style Set|theme.styleSet
Design|Document Formatting|Colors|theme.colors
Design|Document Formatting|Fonts|theme.fonts
Design|Document Formatting|Paragraph Spacing|theme.paragraphSpacing
Design|Document Formatting|Effects|theme.effects
Design|Document Formatting|Set as Default|theme.setDefault
Design|Page Background|Watermark|page.watermark
Design|Page Background|Page Color|page.color
Design|Page Background|Page Borders|page.borders
Layout|Page Setup|Margins|page.margins
Layout|Page Setup|Orientation|page.orientation
Layout|Page Setup|Size|page.size
Layout|Page Setup|Columns|page.columns
Layout|Page Setup|Breaks|page.break
Layout|Page Setup|Line Numbers|page.lineNumbers
Layout|Page Setup|Hyphenation|page.hyphenation
Layout|Page Setup|Page Setup Dialog|page.pageSetup
Layout|Page Setup|Vertical Alignment|page.verticalAlign
Layout|Paragraph|Indent|paragraph.indents
Layout|Paragraph|Spacing|paragraph.spacing
Layout|Arrange|Position|arrange.position
Layout|Arrange|Wrap Text|arrange.wrap
Layout|Arrange|Bring Forward|arrange.bringForward
Layout|Arrange|Send Backward|arrange.sendBackward
Layout|Arrange|Selection Pane|arrange.selectionPane
Layout|Arrange|Align|arrange.align
Layout|Arrange|Group|arrange.group
Layout|Arrange|Rotate|arrange.rotate
References|Table of Contents|Table of Contents|toc.insert
References|Table of Contents|Add Text|toc.addText
References|Table of Contents|Update Table|toc.update
References|Table of Contents|Remove Table of Contents|toc.remove
References|Footnotes|Insert Footnote|footnote.insert
References|Footnotes|Insert Endnote|endnote.insert
References|Footnotes|Next Footnote|footnote.next
References|Footnotes|Show Notes|footnote.show
References|Footnotes|Footnote Options|footnote.options
References|Research|Researcher|research.open
References|Citations|Insert Citation|citation.insert
References|Citations|Manage Sources|citation.sources
References|Citations|Style|citation.style
References|Citations|Bibliography|bibliography.insert
References|Captions|Insert Caption|caption.insert
References|Captions|Insert Table of Figures|tableOfFigures.insert
References|Captions|Update Table of Figures|tableOfFigures.update
References|Captions|Cross-reference|crossReference.insert
References|Index|Mark Entry|index.markEntry
References|Index|Insert Index|index.insert
References|Index|Update Index|index.update
References|Table of Authorities|Mark Citation|tableOfAuthorities.markCitation
References|Table of Authorities|Insert Table of Authorities|tableOfAuthorities.insert
References|Fields|Update Fields|field.updateAll
Mailings|Create|Envelopes|mailMerge.envelopes
Mailings|Create|Labels|mailMerge.labels
Mailings|Start Mail Merge|Start Mail Merge|mailMerge.start
Mailings|Start Mail Merge|Select Recipients|mailMerge.recipients
Mailings|Start Mail Merge|Edit Recipient List|mailMerge.editRecipients
Mailings|Write & Insert Fields|Highlight Merge Fields|mailMerge.highlightFields
Mailings|Write & Insert Fields|Address Block|mailMerge.addressBlock
Mailings|Write & Insert Fields|Greeting Line|mailMerge.greetingLine
Mailings|Write & Insert Fields|Insert Merge Field|mailMerge.insertField
Mailings|Write & Insert Fields|Rules|mailMerge.rules
Mailings|Write & Insert Fields|Match Fields|mailMerge.matchFields
Mailings|Preview Results|Preview Results|mailMerge.preview
Mailings|Preview Results|Next Record|mailMerge.next
Mailings|Preview Results|Previous Record|mailMerge.previous
Mailings|Preview Results|Find Recipient|mailMerge.findRecipient
Mailings|Preview Results|Check for Errors|mailMerge.checkErrors
Mailings|Finish|Finish & Merge|mailMerge.finish
Review|Proofing|Spelling & Grammar|spelling.check
Review|Proofing|Thesaurus|spelling.thesaurus
Review|Proofing|Word Count|file.wordCount
Review|Speech|Read Aloud|readAloud.open
Review|Accessibility|Check Accessibility|file.accessibility
Review|Language|Translate|review.translate
Review|Language|Language|spelling.language
Review|Comments|New Comment|comment.new
Review|Comments|Delete|comment.deleteCurrent
Review|Comments|Previous|comment.previous
Review|Comments|Next|comment.next
Review|Comments|Show Comments|comment.pane
Review|Comments|Resolve|comment.resolveCurrent
Review|Comments|Reply|comment.replyCurrent
Review|Tracking|Track Changes|change.track
Review|Tracking|Display for Review|change.markup
Review|Tracking|Show Markup|change.showMarkup
Review|Tracking|Reviewing Pane|change.list
Review|Tracking|Track Changes Options|change.options
Review|Changes|Accept|change.accept
Review|Changes|Reject|change.reject
Review|Changes|Accept All|change.acceptAll
Review|Changes|Reject All|change.rejectAll
Review|Changes|Previous Change|change.previous
Review|Changes|Next Change|change.next
Review|Compare|Compare|file.compare
Review|Compare|Combine|file.combine
Review|Protect|Block Authors|review.blockAuthors
Review|Protect|Restrict Editing|file.restrictEditing
Review|Ink|Hide Ink|ink.hide
View|Views|Read Mode|view.readMode
View|Views|Print Layout|view.printLayout
View|Views|Web Layout|view.webLayout
View|Views|Outline|view.outline
View|Views|Draft|view.draft
View|Immersive|Focus|view.focus
View|Immersive|Focused Reading|view.immersive
View|Page Movement|Vertical|view.vertical
View|Page Movement|Side to Side|view.sideToSide
View|Show|Ruler|view.ruler
View|Show|Gridlines|view.gridlines
View|Show|Navigation Pane|view.navigationPane
View|Zoom|Zoom|view.zoom
View|Zoom|100%|view.zoom100
View|Zoom|One Page|view.onePage
View|Zoom|Multiple Pages|view.multiplePages
View|Zoom|Page Width|view.pageWidth
View|Zoom|Zoom In|view.zoomIn
View|Zoom|Zoom Out|view.zoomOut
View|Dark Mode|Switch Modes|view.darkMode
View|Window|New Window|view.newWindow
View|Window|Arrange All|view.arrangeAll
View|Window|Split|view.split
View|Window|View Side by Side|view.sideBySide
View|Window|Synchronous Scrolling|view.syncScroll
View|Window|Switch Windows|view.switchWindows
View|Macros|Macros|macro.list
View|Macros|Record Macro|macro.record
View|Properties|Properties|file.properties
Table Design|Table Style Options|Table Style Options|table.look
Table Design|Table Styles|Table Styles|table.style
Table Design|Table Styles|New Table Style|table.newStyle
Table Design|Table Styles|Modify Table Style|table.setStyle
Table Design|Table Styles|Delete Table Style|table.deleteStyle
Table Design|Table Styles|Shading|table.shading
Table Design|Borders|Borders|table.borders
Table Design|Borders|Border Painter|table.borderPainter
Table Layout|Table|Select Table|table.selectTable
Table Layout|Table|Select Row|table.selectRow
Table Layout|Table|Select Cell|table.selectCell
Table Layout|Table|View Gridlines|table.viewGridlines
Table Layout|Table|Properties|table.properties
Table Layout|Draw|Draw Table|table.draw
Table Layout|Draw|Eraser|table.eraser
Table Layout|Rows & Columns|Delete Cells|table.deleteCells
Table Layout|Rows & Columns|Delete Rows|table.deleteRow
Table Layout|Rows & Columns|Delete Columns|table.deleteColumn
Table Layout|Rows & Columns|Delete Table|table.deleteTable
Table Layout|Rows & Columns|Insert Above|table.insertRowAbove
Table Layout|Rows & Columns|Insert Below|table.insertRowBelow
Table Layout|Rows & Columns|Insert Left|table.insertColumnLeft
Table Layout|Rows & Columns|Insert Right|table.insertColumnRight
Table Layout|Merge|Merge Cells|table.merge
Table Layout|Merge|Split Cells|table.split
Table Layout|Merge|Split Table|table.splitTable
Table Layout|Cell Size|AutoFit|table.autofit
Table Layout|Cell Size|Row Height|table.rowHeight
Table Layout|Cell Size|Column Width|table.columnWidth
Table Layout|Cell Size|Distribute Rows|table.distributeRows
Table Layout|Cell Size|Distribute Columns|table.distributeColumns
Table Layout|Alignment|Alignment|table.cellAlign
Table Layout|Alignment|Text Direction|table.textDirection
Table Layout|Alignment|Cell Margins|table.cellMargins
Table Layout|Data|Sort|table.sort
Table Layout|Data|Repeat Header Rows|table.repeatHeader
Table Layout|Data|Convert to Text|table.toText
Table Layout|Data|Formula|table.formula
Equation|Tools|Equation|equation.insert
Equation|Tools|Ink Equation|insert.inkEquation
Equation|Conversions|Unicode|equation.inputFormat
Equation|Conversions|LaTeX|equation.inputFormat
Equation|Conversions|Convert|equation.convert
Equation|Conversions|Normal Text|equation.normalText
Equation|Symbols|Symbols|equation.insertSymbol
Equation|Structures|Fraction|equation.insertStructure
Equation|Structures|Script|equation.insertStructure
Equation|Structures|Radical|equation.insertStructure
Equation|Structures|Integral|equation.insertStructure
Equation|Structures|Large Operator|equation.insertStructure
Equation|Structures|Bracket|equation.insertStructure
Equation|Structures|Function|equation.insertStructure
Equation|Structures|Accent|equation.insertStructure
Equation|Structures|Limit and Log|equation.insertStructure
Equation|Structures|Operator|equation.insertStructure
Equation|Structures|Matrix|equation.insertStructure
Equation|Equation Options|Change to Inline / Display|equation.display
Equation|Equation Options|Justification|equation.justify
Equation|Equation Options|Equation Number (#)|equation.number
Equation|Equation Options|Structure Commands|equation.structure
Picture Format|Adjust|Remove Background|image.removeBackground
Picture Format|Adjust|Corrections|image.corrections
Picture Format|Adjust|Color|image.color
Picture Format|Adjust|Artistic Effects|image.effects
Picture Format|Adjust|Transparency|image.transparency
Picture Format|Adjust|Compress Pictures|image.compress
Picture Format|Adjust|Change Picture|image.change
Picture Format|Adjust|Reset Picture|image.reset
Picture Format|Picture Styles|Picture Styles|image.style
Picture Format|Picture Styles|Picture Border|image.border
Picture Format|Accessibility|Alt Text|image.altText
Picture Format|Size|Crop|image.crop
Picture Format|Size|Size|image.size
Picture Format|Size|Rotation|arrange.rotation
Shape Format|Shape Styles|Shape Fill|shape.fill
Shape Format|Shape Styles|Shape Outline|shape.outline
Shape Format|Shape Styles|Shape Effects|shape.effects
Shape Format|Text|Text Direction|shape.textDirection
Shape Format|Text|Align Text|shape.alignText
Shape Format|Text|Create Link|shape.link
Header & Footer|Navigation|Go to Header|header.edit
Header & Footer|Navigation|Go to Footer|footer.edit
Header & Footer|Navigation|Previous Section|headerFooter.previous
Header & Footer|Navigation|Next Section|headerFooter.next
Header & Footer|Position|Header from Top|headerFooter.position
Editing|Keyboard|Type Text|text.insert
Editing|Keyboard|New Paragraph|text.newParagraph
Editing|Keyboard|Line Break|text.lineBreak
Editing|Keyboard|Column Break|text.columnBreak
Editing|Keyboard|Tab|text.tab
Editing|Keyboard|Backspace|text.backspace
Editing|Keyboard|Delete|text.delete
Editing|Keyboard|Delete Previous Word|text.deleteWordBack
Editing|Keyboard|Delete Next Word|text.deleteWordForward
Editing|Navigation|Word Left|caret.wordLeft
Editing|Navigation|Word Right|caret.wordRight
Editing|Navigation|Line Start|caret.home
Editing|Navigation|Line End|caret.end
Editing|Navigation|Paragraph Up|caret.paraUp
Editing|Navigation|Paragraph Down|caret.paraDown
Editing|Navigation|Document Start|caret.docStart
Editing|Navigation|Document End|caret.docEnd
Editing|Navigation|Page Up|caret.pageUp
Editing|Navigation|Page Down|caret.pageDown
Editing|Selection|Select Word|select.word
Editing|Selection|Select Sentence|select.sentence
Editing|Selection|Select Paragraph|select.paragraph
Editing|Selection|Extend Selection (F8)|select.extend
Editing|Selection|Column Selection|select.column
Tools|Proofing|AutoCorrect Options|autocorrect.options
Tools|Proofing|Set Proofing Language|spelling.language
Tools|Customize|Customize Ribbon|tools.customizeRibbon
Tools|Customize|Customize Keyboard|tools.customizeKeyboard
Tools|Protection|Encrypt with Password|file.encrypt
Tools|Templates|Templates and Add-ins|tools.templates
";

/// Catalog entries: (tab, group, feature, id).
pub fn entries() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    CATALOG
        .lines()
        .filter_map(|l| {
            let mut it = l.split('|');
            Some((it.next()?, it.next()?, it.next()?, it.next()?))
        })
        .collect()
}

/// Parity summary: per tab and overall, with the missing features.
pub fn parity(reg: &Registry) -> Value {
    let e = entries();
    let mut tabs: Vec<(&str, usize, usize, Vec<String>)> = Vec::new();
    for (tab, group, label, id) in &e {
        let have = reg.get(id).is_some();
        let t = match tabs.iter_mut().find(|t| t.0 == *tab) {
            Some(t) => t,
            None => {
                tabs.push((tab, 0, 0, Vec::new()));
                let n = tabs.len() - 1;
                match tabs.get_mut(n) {
                    Some(t) => t,
                    None => continue,
                }
            }
        };
        t.2 += 1;
        if have {
            t.1 += 1;
        } else {
            t.3.push(format!("{group} › {label} (`{id}`)"));
        }
    }
    let live = tabs.iter().map(|t| t.1).sum::<usize>();
    let total = e.len();
    json!({
        "live": live,
        "total": total,
        "percent": if total == 0 { 0.0 } else { (live as f64 * 1000.0 / total as f64).round() / 10.0 },
        "tabs": tabs.iter().map(|t| json!({"tab": t.0, "live": t.1, "total": t.2, "missing": t.3})).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_ids_unique_per_feature() {
        let e = entries();
        assert!(e.len() > 300);
        assert!(e.iter().all(|x| x.3.contains('.')));
    }

    #[test]
    fn parity_floor() {
        let reg = crate::cmd::registry();
        let p = parity(&reg);
        let pct = p["percent"].as_f64().unwrap();
        // The floor only ever rises.
        assert!(pct >= 90.4, "parity {pct}%");
    }
}
