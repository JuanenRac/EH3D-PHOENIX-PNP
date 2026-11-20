// EH3D-PHOENIX-PNP - texts in the eight languages of the suite.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A text of the program is a row of eight strings in the order of `Lang::ALL` (en, es, de, fr, it, ja, zh, pt of Portugal); `{name}` marks a place that `fill` replaces.

use crate::error::Lang;

/// The string of a row in a language.
pub fn pick(lang: Lang, row: &[&'static str; 8]) -> &'static str {
    let i = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    row[i]
}

/// The text with every `{name}` of `pairs` replaced by its value.
pub fn fill(text: &str, pairs: &[(&str, String)]) -> String {
    let mut out = text.to_string();
    for (name, value) in pairs {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// A row in a language with its places filled in.
pub fn say(lang: Lang, row: &[&'static str; 8], pairs: &[(&str, String)]) -> String {
    fill(pick(lang, row), pairs)
}

/// A number for a message: whole when it is, else three decimals without the zeros at the end.
pub fn num(v: f64) -> String {
    if !v.is_finite() {
        return "?".into();
    }
    if (v - v.round()).abs() < 1e-9 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_short() {
        assert_eq!(num(2.0), "2");
        assert_eq!(num(2.5), "2.5");
        assert_eq!(num(0.12345), "0.123");
        assert_eq!(num(f64::NAN), "?");
    }

    #[test]
    fn a_place_is_filled_in() {
        let row = [
            "a {x}", "b {x}", "c {x}", "d {x}", "e {x}", "f {x}", "g {x}", "h {x}",
        ];
        assert_eq!(say(Lang::Pt, &row, &[("x", "1".into())]), "h 1");
        assert_eq!(say(Lang::En, &row, &[("x", "1".into())]), "a 1");
    }
}
