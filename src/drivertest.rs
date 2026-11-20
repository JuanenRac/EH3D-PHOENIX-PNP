// EH3D-PHOENIX-PNP - recorded answers of a controller, played against a driver profile.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A file of `expected | line` rows: what the controller wrote, as a person copied it from a terminal, and how the profile ought to read it (`ok`, `busy`, `error`, `echo`, `other`, or
//! `position X Y Z [E]`). `check` reads every line with the patterns of the profile and names the lines it reads differently, so that a pattern that takes an `echo:` line for an answer, or a
//! position for the one of the `Count` block, is found before a machine is. The samples of `examples/` are the cases of the dialects written down by hand; none is a capture of a real machine.

use crate::error::Lang;
use crate::gcodedriver::{DriverProfile, Reply};
use crate::msg8::{num, say};

/// The most rows of a transcript.
pub const MAX_ROWS: usize = 20_000;

/// What a row expects.
#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    Ok,
    Busy,
    Error,
    Echo,
    Other,
    Position(f64, f64, f64, Option<f64>),
}

impl Expect {
    fn text(&self) -> String {
        match self {
            Expect::Ok => "ok".into(),
            Expect::Busy => "busy".into(),
            Expect::Error => "error".into(),
            Expect::Echo => "echo".into(),
            Expect::Other => "other".into(),
            Expect::Position(x, y, z, e) => match e {
                Some(e) => format!("position {} {} {} {}", num(*x), num(*y), num(*z), num(*e)),
                None => format!("position {} {} {}", num(*x), num(*y), num(*z)),
            },
        }
    }
}

/// How a reply reads, in the words of `Expect`.
pub fn describe(r: &Reply) -> String {
    match r {
        Reply::Ok => "ok".into(),
        Reply::Busy => "busy".into(),
        Reply::Error(_) => "error".into(),
        Reply::Echo(_) => "echo".into(),
        Reply::Other(_) => "other".into(),
        Reply::Position(p) => match p.rot {
            Some(e) => format!("position {} {} {} {}", num(p.x), num(p.y), num(p.z), num(e)),
            None => format!("position {} {} {}", num(p.x), num(p.y), num(p.z)),
        },
    }
}

/// A row that the profile reads differently, or a row that cannot be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub line: usize,
    pub expected: String,
    pub got: String,
    pub text: String,
}

/// The rows of a transcript: the line number, what is expected and the line.
pub fn parse(text: &str) -> Result<Vec<(usize, Expect, String)>, Mismatch> {
    let mut rows = Vec::new();
    for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line_no = i + 1;
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        if rows.len() >= MAX_ROWS {
            return Err(Mismatch {
                line: line_no,
                expected: String::new(),
                got: "too many rows".into(),
                text: String::new(),
            });
        }
        let bad = |why: &str| Mismatch {
            line: line_no,
            expected: String::new(),
            got: why.to_string(),
            text: raw.trim().to_string(),
        };
        let (want, line) = raw
            .split_once('|')
            .ok_or_else(|| bad("a row is 'expected | line'"))?;
        let words: Vec<&str> = want.split_whitespace().collect();
        let expect = match words.as_slice() {
            ["ok"] => Expect::Ok,
            ["busy"] => Expect::Busy,
            ["error"] => Expect::Error,
            ["echo"] => Expect::Echo,
            ["other"] => Expect::Other,
            ["position", rest @ ..] if (3..=4).contains(&rest.len()) => {
                let n: Vec<f64> = rest
                    .iter()
                    .filter_map(|w| w.parse::<f64>().ok().filter(|v| v.is_finite()))
                    .collect();
                if n.len() != rest.len() {
                    return Err(bad("the numbers of a position are not numbers"));
                }
                Expect::Position(n[0], n[1], n[2], n.get(3).copied())
            }
            _ => {
                return Err(bad(
                    "the expected word is ok, busy, error, echo, other or position X Y Z [E]",
                ))
            }
        };
        rows.push((line_no, expect, line.trim().to_string()));
    }
    Ok(rows)
}

fn matches_expect(e: &Expect, r: &Reply) -> bool {
    match (e, r) {
        (Expect::Ok, Reply::Ok)
        | (Expect::Busy, Reply::Busy)
        | (Expect::Error, Reply::Error(_))
        | (Expect::Echo, Reply::Echo(_))
        | (Expect::Other, Reply::Other(_)) => true,
        (Expect::Position(x, y, z, e), Reply::Position(p)) => {
            (x - p.x).abs() < 1e-6
                && (y - p.y).abs() < 1e-6
                && (z - p.z).abs() < 1e-6
                && match (e, p.rot) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-6,
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => false,
    }
}

/// Plays a transcript against a profile; the rows that are read differently, in order. An error of the file itself is the first and only row.
pub fn check(profile: &DriverProfile, text: &str) -> (usize, Vec<Mismatch>) {
    let rows = match parse(text) {
        Ok(r) => r,
        Err(m) => return (0, vec![m]),
    };
    let mut out = Vec::new();
    for (line, expect, text) in &rows {
        let reply = profile.classify(text);
        if !matches_expect(expect, &reply) {
            out.push(Mismatch {
                line: *line,
                expected: expect.text(),
                got: describe(&reply),
                text: text.clone(),
            });
        }
    }
    (rows.len(), out)
}

/// A mismatch in a language.
pub fn message(m: &Mismatch, lang: Lang) -> String {
    if m.expected.is_empty() {
        let row: [&str; 8] = [
            "line {n}: {why} ({text})",
            "línea {n}: {why} ({text})",
            "Zeile {n}: {why} ({text})",
            "ligne {n} : {why} ({text})",
            "riga {n}: {why} ({text})",
            "{n} 行目：{why}（{text}）",
            "第 {n} 行：{why}（{text}）",
            "linha {n}: {why} ({text})",
        ];
        return say(
            lang,
            &row,
            &[
                ("n", m.line.to_string()),
                ("why", m.got.clone()),
                ("text", m.text.clone()),
            ],
        );
    }
    let row: [&str; 8] = [
        "line {n}: '{text}' should read as {want} and the profile reads it as {got}",
        "línea {n}: '{text}' debería leerse como {want} y el perfil lo lee como {got}",
        "Zeile {n}: '{text}' sollte als {want} gelesen werden, das Profil liest es als {got}",
        "ligne {n} : '{text}' devrait se lire comme {want} et le profil la lit comme {got}",
        "riga {n}: '{text}' dovrebbe essere letta come {want} e il profilo la legge come {got}",
        "{n} 行目：「{text}」は {want} と読まれるはずですが、プロファイルは {got} と読みます",
        "第 {n} 行：“{text}”应读作 {want}，而配置档读作 {got}",
        "linha {n}: '{text}' devia ler-se como {want} e o perfil lê-o como {got}",
    ];
    say(
        lang,
        &row,
        &[
            ("n", m.line.to_string()),
            ("text", m.text.clone()),
            ("want", m.expected.clone()),
            ("got", m.got.clone()),
        ],
    )
}
