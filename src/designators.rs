// EH3D-PHOENIX-PNP - the order and the writing of lists of designators.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A bill of materials lists the designators of a row (`R1, R2, R10`) and a house that assembles the board reads them one by one. `natural_cmp` puts `R2` before `R10` (the digits are compared as
//! numbers, whatever their length, so that a number of any size is safe), `sort` uses it, `join` writes a list and `compress` writes runs of three or more as a range (`R1-R3`) that
//! `import::expand_references` reads back. Only a designator that is letters followed by digits and nothing else, with no leading zero, takes part in a range; the others are written as they are.

use std::cmp::Ordering;

/// The most designators that `compress` and `join` take (a longer list is cut and the caller is told by the length).
pub const MAX_LIST: usize = 1_000_000;

/// A text as runs of digits and runs of anything else.
fn chunks(text: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut digits: Option<bool> = None;
    for (i, c) in text.char_indices() {
        let is = c.is_ascii_digit();
        match digits {
            Some(d) if d == is => {}
            Some(d) => {
                out.push((d, &text[start..i]));
                start = i;
                digits = Some(is);
            }
            None => digits = Some(is),
        }
    }
    if let Some(d) = digits {
        out.push((d, &text[start..]));
    }
    out
}

/// Two runs of digits compared as numbers: the longer one (without leading zeros) is the larger.
fn digits_cmp(a: &str, b: &str) -> Ordering {
    let (a2, b2) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a2.len()
        .cmp(&b2.len())
        .then_with(|| a2.cmp(b2))
        .then_with(|| b.len().cmp(&a.len()))
}

/// The order of designators for a person: letters first, then the number as a number (`C9` before `C10`), then what follows. Equal designators are `Equal`; two that differ only by
/// the zeros in front of a number are ordered, so that the order is total.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let o = match (x.0, y.0) {
            (true, true) => digits_cmp(x.1, y.1),
            (false, false) => x.1.cmp(y.1),
            // a run of digits comes after a run of letters at the same place
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len())
}

/// Sorts a list of designators in the natural order.
pub fn sort(list: &mut [String]) {
    list.sort_by(|a, b| natural_cmp(a, b));
}

/// The designators written one after the other with a separator between them (the list is sorted first).
pub fn join(list: &[String], separator: &str) -> String {
    let mut v: Vec<String> = list.iter().take(MAX_LIST).cloned().collect();
    sort(&mut v);
    v.join(separator)
}

/// A designator as its letters, its number and the width of the number, when it is letters and digits and nothing else and the number has no zero in front.
fn numbered(text: &str) -> Option<(&str, u64)> {
    let at = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(at);
    if letters.is_empty()
        || digits.starts_with('0')
        || !digits.chars().all(|c| c.is_ascii_digit())
        || digits.len() > 15
        || letters.contains(['-', ',', ';', ' '])
    {
        return None;
    }
    Some((letters, digits.parse().ok()?))
}

/// The designators sorted and written with a comma and a space, a run of `min_run` or more consecutive numbers written as a range (`R1-3`: the letters once). A `min_run` under 3 is
/// taken as 3, so that a pair is never written as a range. The same list gives the same text whatever its order, and `import::expand_references` gives back the designators that were
/// given (without repeats).
pub fn compress(list: &[String], min_run: usize) -> String {
    let min_run = min_run.max(3);
    let mut v: Vec<String> = list.iter().take(MAX_LIST).cloned().collect();
    sort(&mut v);
    v.dedup();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < v.len() {
        let Some((letters, first)) = numbered(&v[i]) else {
            out.push(v[i].clone());
            i += 1;
            continue;
        };
        let mut end = i;
        while end + 1 < v.len() {
            match numbered(&v[end + 1]) {
                Some((l, n)) if l == letters && n == first + (end + 1 - i) as u64 => end += 1,
                _ => break,
            }
        }
        if end + 1 - i >= min_run {
            out.push(format!("{}-{}", v[i], &v[end][letters.len()..]));
            i = end + 1;
        } else {
            out.push(v[i].clone());
            i += 1;
        }
    }
    out.join(", ")
}

/// Whether a text holds one designator and nothing else: no separator, no space, no range.
pub fn is_single(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && !t.contains([',', ';', ' ', '\t', '\n', '\r']) && numbered_range(t).is_none()
}

/// A text of the form `R1-R5` or `R1-5` (a range), when it is one.
fn numbered_range(text: &str) -> Option<()> {
    let (a, b) = text.split_once('-')?;
    let left = numbered(a)?;
    let right = b.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    (!right.is_empty() && right.chars().all(|c| c.is_ascii_digit()) && left.1 > 0).then_some(())
}
