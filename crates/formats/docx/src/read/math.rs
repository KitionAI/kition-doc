//! Office Math (OMML) in a DOCX → the [`kition_office_core::math`] tree, by the codec the Office
//! kits share ([`kition_office_core::math::omml`]), reading Doc's own XML tree.

use kition_office_core::math::{Math, MathJc, MathProps, omml};

use crate::xml::El;

/// An `m:oMath` element as an equation (structure plus its source XML).
pub fn read_omath(e: &El, jc: MathJc) -> Math {
    Math { nodes: omml::read(e), omml: e.to_xml(), jc, linear: false }
}

/// `m:oMathParaPr/m:jc`.
pub fn para_jc(para: &El) -> MathJc {
    omml::para_jc(para)
}

/// The settings part's `m:mathPr`: how display equations wrap.
pub fn read_math_pr(e: &El) -> MathProps {
    omml::read_math_pr(e)
}
