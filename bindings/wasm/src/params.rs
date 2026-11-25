//! The parameters commands read that their notation does not spell out, added to the schemas the
//! catalogue offers agents. Every command refuses a key its schema does not name (engine rule E7a),
//! so the schema must name every key the command reads: the keys a
//! handler reads beside its listed ones.

use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Kind {
    Integer,
    Number,
    String,
    Boolean,
    Array,
    Object,
    IndexOrName,
    StringOrObject,
}

impl Kind {
    fn node(self) -> Value {
        let description = "read by the command beside its listed parameters";
        let kind = match self {
            Kind::IndexOrName => return json!({ "oneOf": [{ "type": "integer" }, { "type": "string" }], "description": description }),
            Kind::StringOrObject => {
                return json!({ "oneOf": [{ "type": "string" }, { "type": "object", "additionalProperties": true }], "description": description });
            }
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::String => "string",
            Kind::Boolean => "boolean",
            Kind::Array => "array",
            Kind::Object => return json!({ "type": "object", "additionalProperties": true, "description": description }),
        };
        json!({ "type": kind, "description": description })
    }
}

/// Keys a command's handler reads (directly or through a helper it passes its parameters to) that
/// its notation leaves out, found by reading each handler.
const EXTRA: &[(&str, &[(&str, Kind)])] = &[
    // KitDrawing's operations (kition-office-core `DrawingOp::parse`, `ChartChange::parse`) read these too.
    ("drawing.insert", &[("paragraphs", Kind::Array), ("body", Kind::Object), ("warp", Kind::Object)]),
    ("drawing.transform", &[("id", Kind::IndexOrName), ("frame", Kind::Object), ("dx", Kind::Number)]),
    ("drawing.format", &[("id", Kind::IndexOrName), ("effects", Kind::Object)]),
    ("drawing.text", &[("text", Kind::StringOrObject), ("paragraphs", Kind::Array), ("body", Kind::Object), ("warp", Kind::Object)]),
    ("drawing.delete", &[("id", Kind::IndexOrName)]),
    (
        "drawing.chart",
        &[
            ("ids", Kind::Array),
            ("seriesFormat", Kind::Object),
            ("xAxis", Kind::Object),
            ("yAxis", Kind::Object),
            ("dataLabelPosition", Kind::String),
        ],
    ),
    ("caret.set", &[("story", Kind::StringOrObject)]),
    ("comment.goto", &[("page", Kind::Integer), ("bookmark", Kind::String), ("paragraph", Kind::Integer)]),
    ("comment.reply", &[("id", Kind::Integer)]),
    ("theme.effects", &[("name", Kind::String)]),
    ("theme.colors", &[("name", Kind::String)]),
    ("file.text", &[("story", Kind::StringOrObject)]),
    ("ink.state", &[("x", Kind::Number), ("y", Kind::Number), ("page", Kind::Integer)]),
    ("drawing.describe", &[("id", Kind::IndexOrName)]),
    ("drawing.erase", &[("page", Kind::Integer)]),
    ("drawing.ink", &[("page", Kind::Integer)]),
    ("drawing.insert", &[("page", Kind::Integer)]),
    ("edit.advancedFind", &[("with", Kind::String)]),
    ("edit.find", &[("with", Kind::String)]),
    ("edit.replace", &[("matchCase", Kind::Boolean), ("wholeWord", Kind::Boolean), ("regex", Kind::Boolean)]),
    ("equation.insertStructure", &[("builtin", Kind::String)]),
    ("equation.set", &[("id", Kind::String), ("builtin", Kind::String)]),
    ("file.autosave", &[("value", Kind::Boolean)]),
    ("format.allCaps", &[("value", Kind::Boolean)]),
    ("format.doubleStrikethrough", &[("value", Kind::Boolean)]),
    ("format.doubleUnderline", &[("style", Kind::String), ("value", Kind::Boolean)]),
    ("format.emboss", &[("value", Kind::Boolean)]),
    ("format.engrave", &[("value", Kind::Boolean)]),
    ("format.fontDialog", &[("name", Kind::String)]),
    ("format.hidden", &[("value", Kind::Boolean)]),
    ("format.outline", &[("value", Kind::Boolean)]),
    ("format.shadow", &[("value", Kind::Boolean)]),
    ("format.smallCaps", &[("value", Kind::Boolean)]),
    ("format.subscript", &[("value", Kind::Boolean)]),
    ("format.superscript", &[("value", Kind::Boolean)]),
    ("format.wordUnderline", &[("style", Kind::String), ("value", Kind::Boolean)]),
    ("chart.insert", &[("kind", Kind::String)]),
    ("equation.insert", &[("id", Kind::String)]),
    ("link.remove", &[("url", Kind::String), ("text", Kind::String)]),
    ("text.insertFromFile", &[("text", Kind::String)]),
    ("page.differentFirstPage", &[("value", Kind::Boolean)]),
    ("page.differentOddEven", &[("value", Kind::Boolean)]),
    ("page.hyphenation", &[("value", Kind::Boolean)]),
    ("page.verticalAlign", &[("value", Kind::String)]),
    ("mailMerge.editRecipients", &[("csv", Kind::String), ("path", Kind::String)]),
    ("mailMerge.findRecipient", &[("text", Kind::String)]),
    ("mailMerge.highlightFields", &[("value", Kind::Boolean), ("color", Kind::String)]),
    ("mailMerge.insertField", &[("instr", Kind::String), ("result", Kind::String)]),
    ("mailMerge.preview", &[("value", Kind::Boolean)]),
    ("paragraph.align", &[("jc", Kind::String)]),
    ("paragraph.heading1", &[("style", Kind::String)]),
    ("paragraph.heading2", &[("style", Kind::String)]),
    ("paragraph.heading3", &[("style", Kind::String)]),
    ("format.justify", &[("jc", Kind::String)]),
    ("paragraph.keepLines", &[("value", Kind::Boolean)]),
    ("paragraph.keepNext", &[("value", Kind::Boolean)]),
    ("paragraph.multilevel", &[("off", Kind::Boolean)]),
    ("paragraph.normal", &[("style", Kind::String)]),
    ("paragraph.outlineLevel", &[("level", Kind::Integer)]),
    ("paragraph.pageBreakBefore", &[("value", Kind::Boolean)]),
    ("paragraph.widowControl", &[("value", Kind::Boolean)]),
    ("readAloud.playPause", &[("skipCitations", Kind::Boolean)]),
    ("readAloud.speed", &[("rate", Kind::Number)]),
    ("change.list", &[("description", Kind::String), ("props", Kind::Object)]),
    ("spelling.ignoreAll", &[("word", Kind::String)]),
    ("change.markup", &[("value", Kind::String)]),
    ("spelling.proofing", &[("value", Kind::Boolean)]),
    ("comment.resolveCurrent", &[("id", Kind::Integer), ("value", Kind::Boolean)]),
    ("change.showMarkup", &[("value", Kind::Boolean)]),
    ("spelling.thesaurus", &[("word", Kind::String)]),
    ("select.collapse", &[("end", Kind::Boolean)]),
    ("select.extend", &[("extend", Kind::Boolean)]),
    ("select.line", &[("extend", Kind::Boolean)]),
    ("select.text", &[("story", Kind::StringOrObject)]),
    ("style.addToGallery", &[("style", Kind::String), ("value", Kind::Boolean)]),
    ("table.borderPainter", &[("kind", Kind::String), ("width", Kind::Number), ("color", Kind::String)]),
    ("table.repeatHeader", &[("value", Kind::Boolean)]),
    ("text.insert", &[("raw", Kind::Boolean)]),
    ("view.commentsPane", &[("value", Kind::Boolean)]),
    ("view.darkMode", &[("value", Kind::Boolean)]),
    ("view.focus", &[("value", Kind::Boolean)]),
    ("view.marks", &[("value", Kind::Boolean)]),
    ("view.multiplePages", &[("value", Kind::Boolean)]),
    ("view.navigationPane", &[("value", Kind::Boolean)]),
    ("view.onePage", &[("value", Kind::Boolean)]),
    ("view.pageWidth", &[("value", Kind::Boolean)]),
    ("view.ruler", &[("value", Kind::Boolean)]),
    ("view.split", &[("value", Kind::Boolean)]),
    ("view.stylesPane", &[("value", Kind::Boolean)]),
    ("view.zoom100", &[("value", Kind::Boolean)]),
    ("view.zoomIn", &[("value", Kind::Boolean)]),
    ("view.zoomOut", &[("value", Kind::Boolean)]),
];

/// Adds to `schema` (a command's `ParameterSchemaSpec`) the keys `id` reads that its notation
/// leaves out.
pub fn complete(id: &str, schema: &mut Value) {
    let Some(properties) = schema.as_object_mut() else { return };
    for (key, kind) in EXTRA.iter().filter(|(command, _)| *command == id).flat_map(|(_, keys)| keys.iter()) {
        properties.entry(*key).or_insert_with(|| kind.node());
    }
}
