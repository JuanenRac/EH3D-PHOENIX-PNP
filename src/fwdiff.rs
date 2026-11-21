// EH3D-PHOENIX-PNP - what differs between two dumps of the configuration of a firmware.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! After a controller is flashed again, or its settings are saved and restored, a machine may behave otherwise because a stored value is not the one it was. The text a Marlin
//! controller answers to `M503` (and the EEPROM report) is a list of lines of G-code that set its values back: `M92 X80.00 Y80.00 Z400.00 E500.00` for the steps per millimetre,
//! `M203` for the greatest feeds, `M201` for the accelerations, `M906` for the currents of the motors, `M914` for the sensitivity of homing without switches. This reads two such dumps
//! and says which values differ, as `config-diff` does for the settings of this program: a value is a letter of a line (`M92 X`), `80` and `80.00` are the same, a line the other dump does
//! not have shows as `-`. Commands that name a part (a motor, a heater) keep it in the key (`M906 I1 X`). When the two dumps come from different firmware, a warning says that the values
//! stored by the old one stay in force until the EEPROM is reset. It reads text and changes nothing; nothing is sent to a controller.

use std::collections::BTreeMap;

/// The most bytes of a dump and the most lines that are read.
pub const MAX_DUMP_BYTES: usize = 1_048_576;
pub const MAX_LINES: usize = 20_000;

/// The letter that names the part a line is for, by command (`M906 I1 X800`: the motor 1; `M145 S0 H180`: the preset 0). Any other letter of a line is a value.
const SELECTORS: [(&str, char); 7] = [
    ("M906", 'I'),
    ("M911", 'I'),
    ("M912", 'I'),
    ("M913", 'I'),
    ("M914", 'I'),
    ("M145", 'S'),
    ("M218", 'T'),
];

/// What a dump holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dump {
    /// The name of the firmware when the dump says it (`FIRMWARE_NAME:` of `M115`, or the line `Marlin 2.1.2`).
    pub firmware: Option<String>,
    /// The values by key (`M92 X`), as they were written.
    pub values: BTreeMap<String, String>,
    /// The lines that were not a command of G-code (comments, answers such as `ok`).
    pub ignored: usize,
}

fn is_code(word: &str) -> bool {
    let mut cs = word.chars();
    matches!(cs.next(), Some('G' | 'M'))
        && word.len() > 1
        && word[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Reads a dump. A dump with no value at all is not one.
pub fn parse(text: &str) -> Result<Dump, String> {
    if text.len() > MAX_DUMP_BYTES {
        return Err(format!("a dump is at most {MAX_DUMP_BYTES} bytes"));
    }
    let mut dump = Dump::default();
    for (n, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        if n >= MAX_LINES {
            return Err(format!("a dump is at most {MAX_LINES} lines"));
        }
        let mut line = raw.trim();
        if line
            .get(..5)
            .is_some_and(|head| head.eq_ignore_ascii_case("echo:"))
        {
            line = line[5..].trim();
        }
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("FIRMWARE_NAME:") {
            let name = rest
                .split(" SOURCE_CODE_URL:")
                .next()
                .and_then(|s| s.split(" PROTOCOL_VERSION:").next())
                .unwrap_or("")
                .trim();
            if !name.is_empty() && dump.firmware.is_none() {
                dump.firmware = Some(name.to_string());
            }
            continue;
        }
        if line.starts_with("Marlin ") && line[7..].starts_with(|c: char| c.is_ascii_digit()) {
            if dump.firmware.is_none() {
                dump.firmware = Some(line.to_string());
            }
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(code) = words.next().map(str::to_ascii_uppercase) else {
            continue;
        };
        if !is_code(&code) {
            dump.ignored += 1;
            continue;
        }
        let selector = SELECTORS.iter().find(|s| s.0 == code).map(|s| s.1);
        let mut base = code.clone();
        let mut params: Vec<(char, String)> = Vec::new();
        for w in words {
            let mut cs = w.chars();
            let Some(letter) = cs.next().map(|c| c.to_ascii_uppercase()) else {
                continue;
            };
            if !letter.is_ascii_alphabetic() {
                continue;
            }
            let value = cs.as_str().to_string();
            if Some(letter) == selector {
                base = format!("{base} {letter}{value}");
            } else {
                params.push((letter, value));
            }
        }
        for (letter, value) in params {
            dump.values.insert(format!("{base} {letter}"), value);
        }
    }
    if dump.values.is_empty() {
        return Err("the dump has no line of G-code that sets a value".to_string());
    }
    Ok(dump)
}

/// One value that the two dumps give differently; `None` where a dump does not have the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub key: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The result of comparing two dumps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FwDiff {
    pub changes: Vec<Change>,
    /// Values written differently that mean the same (`80` and `80.00`).
    pub same_meaning: usize,
    pub old_firmware: Option<String>,
    pub new_firmware: Option<String>,
}

fn same_value(a: &str, b: &str) -> bool {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 1e-9,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// Compares two dumps.
pub fn diff(old: &Dump, new: &Dump) -> FwDiff {
    let mut keys: Vec<&String> = old.values.keys().chain(new.values.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut out = FwDiff {
        old_firmware: old.firmware.clone(),
        new_firmware: new.firmware.clone(),
        ..FwDiff::default()
    };
    for key in keys {
        let (a, b) = (old.values.get(key), new.values.get(key));
        match (a, b) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) if same_value(x, y) => out.same_meaning += 1,
            _ => out.changes.push(Change {
                key: key.clone(),
                old: a.cloned(),
                new: b.cloned(),
            }),
        }
    }
    out
}

impl FwDiff {
    /// Whether the two dumps give the same values.
    pub fn is_same(&self) -> bool {
        self.changes.is_empty()
    }

    /// The firmware changed between the dumps: both say what they are and they are not the same. The values that the old firmware stored in the EEPROM stay in force in the new one until
    /// the EEPROM is reset, so a machine can behave as before the change or half as before.
    pub fn firmware_changed(&self) -> Option<(&str, &str)> {
        match (&self.old_firmware, &self.new_firmware) {
            (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b) => Some((a, b)),
            _ => None,
        }
    }

    /// How many values are only in the second dump, only in the first, and in both with another value.
    pub fn counts(&self) -> (usize, usize, usize) {
        let added = self.changes.iter().filter(|c| c.old.is_none()).count();
        let removed = self.changes.iter().filter(|c| c.new.is_none()).count();
        (added, removed, self.changes.len() - added - removed)
    }

    /// The lines of the differences, `key: old -> new`, with `-` for a line a dump does not have. The words of the summary and of the warning are the job of the command, which has the
    /// language.
    pub fn lines(&self) -> Vec<String> {
        let shown = |v: &Option<String>| match v {
            Some(v) if v.is_empty() => "(on)".to_string(),
            Some(v) => v.clone(),
            None => "-".to_string(),
        };
        self.changes
            .iter()
            .map(|c| format!("{}: {} -> {}", c.key, shown(&c.old), shown(&c.new)))
            .collect()
    }
}
