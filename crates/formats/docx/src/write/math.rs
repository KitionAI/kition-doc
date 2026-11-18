//! The [`kition_office_core::math`] tree → Office Math (OMML) in a DOCX, by the codec the Office
//! kits share ([`kition_office_core::math::omml`]).

use kition_office_core::math::{Arg, Math, MathProps, omml, parse_linear};

use crate::xml::W;

/// Write an equation: `m:oMath`, inside `m:oMathPara` for a display equation. The source OMML
/// is written back unchanged when the equation came from a file.
pub fn write_equation(w: &mut W, linear: &str, display: bool, math: &Math) {
    if display {
        w.open("m:oMathPara", &[]);
        w.raw(&omml::para_pr(math.jc));
    }
    if !math.omml.is_empty() && math.omml.starts_with("<m:oMath") && crate::xml::parse(wrap_ns(&math.omml).as_bytes()).is_ok() {
        w.raw(&math.omml);
    } else {
        let parsed;
        let nodes: &Arg = if math.nodes.is_empty() {
            parsed = parse_linear(linear);
            &parsed
        } else {
            &math.nodes
        };
        w.raw(&omml::write(nodes));
    }
    if display {
        w.close("m:oMathPara");
    }
}

/// The kept XML with namespace declarations, for validating it on its own.
fn wrap_ns(xml: &str) -> String {
    let decl: String = crate::xml::body_ns().iter().filter(|(k, _)| k.starts_with("xmlns:")).map(|(k, v)| format!(" {k}=\"{v}\"")).collect();
    format!("<w:x{decl}>{xml}</w:x>")
}

/// The settings part's `m:mathPr`.
pub fn math_pr(w: &mut W, m: &MathProps) {
    w.raw(&omml::math_pr(m));
}
