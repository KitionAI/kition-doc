//! Run-font classification from MS-OI29500 section 2.1.88 (rFonts).
//! Complex-script formatting takes precedence in the paragraph shaper.

use super::ResolvedChar;

impl ResolvedChar {
    /// Whether this scalar uses the inherited East Asian font slot. Ambiguous
    /// symbols follow the font hint; some Latin ranges also require Chinese.
    pub fn uses_east_asian(&self, c: char) -> bool {
        let value = c as u32;
        // Word's legacy symbol codes identify glyphs within the explicit symbol font.
        // Routing them through a different font changes their meaning.
        if (0xf000..=0xf0ff).contains(&value) && ["Symbol", "Wingdings", "Webdings"].iter().any(|family| self.font.eq_ignore_ascii_case(family)) {
            return false;
        }
        if matches!(value,
            0x1100..=0x11ff | 0x2f00..=0x2fdf | 0x2ff0..=0x4dbf
            | 0x4e00..=0x9fff | 0xa000..=0xa4cf | 0xac00..=0xd7af
            | 0xf900..=0xfaff | 0xfe30..=0xfe6f | 0xff00..=0xffef
            | 0x20000..=0x3ffff)
        {
            return true;
        }
        if self.font_hint.as_deref() != Some("eastAsia") {
            return false;
        }
        let chinese = self
            .lang_ea
            .as_deref()
            .or(self.lang.as_deref())
            .is_some_and(|language| language.get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case("zh")));
        matches!(value,
            0x00a1 | 0x00a4 | 0x00a7..=0x00a8 | 0x00aa | 0x00ad | 0x00af
            | 0x00b0..=0x00b4 | 0x00b6..=0x00ba | 0x00bc..=0x00bf | 0x00d7 | 0x00f7
            | 0x02b0..=0x03cf | 0x0400..=0x04ff | 0x2000..=0x27bf
            | 0x2e80..=0x2eff | 0xe000..=0xf8ff | 0xfb00..=0xfb1c)
            || (chinese
                && matches!(value,
                0x00e0..=0x00e1 | 0x00e8..=0x00ea | 0x00ec..=0x00ed
                | 0x00f2..=0x00f3 | 0x00f9..=0x00fa | 0x00fc | 0x0100..=0x02af | 0x1e00..=0x1eff))
    }
}
