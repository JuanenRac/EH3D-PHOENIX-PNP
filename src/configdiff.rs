// EH3D-PHOENIX-PNP - what differs between two configuration files of the machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A line was changed in the file of the machine and the job came out different. This says which settings two files give differently, in what they mean and not in how they are written:
//! `yes` and `on` are the same, `5` and `5.0` are the same, and a line that gives the value a setting has by default is the same as no line. The answer for a key is found by putting the
//! lines of the second file for that key into the first and reading both: if the machine that comes out is the same, the key is not a difference.

use crate::config::{MachineConfig, MAX_CONFIG_BYTES};
use crate::error::PnpError;
use std::collections::BTreeMap;

/// One setting that the two files give differently. An empty list means that the file does not set it (the default holds).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChange {
    pub key: String,
    pub old: Vec<String>,
    pub new: Vec<String>,
}

/// The settings that differ, by key, and how many were written differently and mean the same.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigDiff {
    pub changes: Vec<KeyChange>,
    pub same_meaning: usize,
}

/// The `key = value` lines of a file as the key (lower case) and the values in the order they are written, with the spaces inside a value reduced to one.
fn lines_of(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            map.entry(k.trim().to_ascii_lowercase())
                .or_default()
                .push(v.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    map
}

/// The text of `base` with the lines of `key` taken out and the ones in `values` put in their place.
fn with_key(base: &str, key: &str, values: &[String]) -> String {
    let mut out: Vec<String> = base
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|raw| {
            let t = raw.trim();
            match t.split_once('=') {
                Some((k, _)) if !t.starts_with('#') => !k.trim().eq_ignore_ascii_case(key),
                _ => true,
            }
        })
        .map(str::to_string)
        .collect();
    for v in values {
        out.push(format!("{key} = {v}"));
    }
    out.join("\n")
}

/// Compares two configuration files. Both have to be valid: the error is the one of the file that is not (the number says which, `first` or `second`).
pub fn diff(first: &str, second: &str) -> Result<ConfigDiff, (&'static str, PnpError)> {
    let a = MachineConfig::parse(first).map_err(|e| ("first", e))?;
    MachineConfig::parse(second).map_err(|e| ("second", e))?;
    let (ka, kb) = (lines_of(first), lines_of(second));
    let mut keys: Vec<&String> = ka.keys().chain(kb.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut out = ConfigDiff::default();
    for key in keys {
        let old = ka.get(key).cloned().unwrap_or_default();
        let new = kb.get(key).cloned().unwrap_or_default();
        if old == new {
            continue;
        }
        // the first file with the lines of the second for this key: the same machine, or not
        let swapped = with_key(first, key, &new);
        let same = swapped.len() <= MAX_CONFIG_BYTES
            && MachineConfig::parse(&swapped).is_ok_and(|c| c == a);
        if same {
            out.same_meaning += 1;
        } else {
            out.changes.push(KeyChange {
                key: key.clone(),
                old,
                new,
            });
        }
    }
    Ok(out)
}

impl ConfigDiff {
    /// Whether the two files mean the same machine.
    pub fn is_same(&self) -> bool {
        self.changes.is_empty()
    }

    /// The values of a key as a person reads them; a token is never shown, only whether there is one.
    fn shown(key: &str, values: &[String]) -> String {
        if values.is_empty() {
            "(default)".to_string()
        } else if key == "server_token" {
            "(set)".to_string()
        } else {
            values.join(" ; ")
        }
    }

    /// One line for each key that differs, then a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.changes {
            out.push_str(&format!(
                "{}: {} -> {}\n",
                c.key,
                Self::shown(&c.key, &c.old),
                Self::shown(&c.key, &c.new)
            ));
        }
        out.push_str(&format!(
            "{} settings differ, {} are written differently and mean the same\n",
            self.changes.len(),
            self.same_meaning
        ));
        out
    }
}
