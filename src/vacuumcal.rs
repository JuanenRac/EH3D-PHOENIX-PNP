// EH3D-PHOENIX-PNP - the thresholds of the vacuum from readings of the sensor.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A nozzle is judged by the level its vacuum sensor reads: above `vacuum_pick_min_kpa` after a pick the part is held, under `vacuum_release_max_kpa` after a place it is gone. Those two figures
//! are guesses until they are measured. This takes readings of the three states (the nozzle open to the air, holding a part, and after the part was let go), finds the gap between the
//! strongest reading with no part and the weakest with one, and proposes the two thresholds a margin inside that gap. If the readings of the two kinds overlap, no threshold can tell them
//! apart and it says so. It reads numbers: it measures nothing.

use crate::config::MachineConfig;
use crate::error::Lang;
use crate::msg8::{num, say};
use crate::parts::{detect_delimiter, read_records_numbered};

/// The most rows of a table of readings.
pub const MAX_ROWS: usize = 200_000;

/// The state of the nozzle at a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Free,
    Held,
    Released,
}

impl State {
    pub fn parse(text: &str) -> Option<State> {
        match text.trim().to_ascii_lowercase().as_str() {
            "free" | "open" | "air" => Some(State::Free),
            "held" | "hold" | "holding" | "picked" => Some(State::Held),
            "released" | "release" | "dropped" | "placed" => Some(State::Released),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            State::Free => "free",
            State::Held => "held",
            State::Released => "released",
        }
    }
}

/// The readings of each state (kPa below the air).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Readings {
    pub free: Vec<f64>,
    pub held: Vec<f64>,
    pub released: Vec<f64>,
}

/// Reads a table `state,kpa` (the columns found by name; `free`, `held` and `released`).
pub fn parse(text: &str) -> Result<Readings, String> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().next().unwrap_or("");
    let rows = read_records_numbered(text, detect_delimiter(head))
        .map_err(|(l, e)| format!("line {l}: {e}"))?;
    let mut it = rows.into_iter();
    let (_, header) = it.next().ok_or("the table has no header")?;
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let (Some(cs), Some(ck)) = (col("state"), col("kpa")) else {
        return Err("the header needs the columns state and kpa".into());
    };
    let mut r = Readings::default();
    for (n, (line, rec)) in it.enumerate() {
        if n >= MAX_ROWS {
            return Err(format!("the table has over {MAX_ROWS} rows"));
        }
        let state = rec
            .get(cs)
            .and_then(|s| State::parse(s))
            .ok_or_else(|| format!("line {line}: the state is not free, held or released"))?;
        let kpa = rec
            .get(ck)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && v.abs() <= 1000.0)
            .ok_or_else(|| {
                format!("line {line}: the reading is not a number from -1000 to 1000")
            })?;
        match state {
            State::Free => r.free.push(kpa),
            State::Held => r.held.push(kpa),
            State::Released => r.released.push(kpa),
        }
    }
    Ok(r)
}

/// The figures of the proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub margin_pct: f64,
    pub min_gap_kpa: f64,
    pub min_readings: usize,
    pub current_pick_min: f64,
    pub current_release_max: f64,
}

impl Params {
    pub fn from_config(c: &MachineConfig) -> Params {
        Params {
            margin_pct: c.vacuum_margin_pct,
            min_gap_kpa: c.vacuum_min_gap_kpa,
            min_readings: c.vacuum_min_readings as usize,
            current_pick_min: c.vacuum_pick_min_kpa,
            current_release_max: c.vacuum_release_max_kpa,
        }
    }
}

/// What is told besides the figures.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    FewReadings(&'static str, usize),
    SmallGap(f64),
    Overlap(f64, f64),
    CurrentMisreads(usize, usize),
}

/// Statistics of one state.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub min: f64,
    pub mean: f64,
    pub max: f64,
}

fn stats(v: &[f64]) -> Option<Stats> {
    if v.is_empty() {
        return None;
    }
    Some(Stats {
        n: v.len(),
        min: v.iter().copied().fold(f64::INFINITY, f64::min),
        mean: v.iter().sum::<f64>() / v.len() as f64,
        max: v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

/// The proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub free: Option<Stats>,
    pub held: Option<Stats>,
    pub released: Option<Stats>,
    /// The thresholds, when the two kinds of reading do not overlap.
    pub thresholds: Option<(f64, f64)>,
    pub notes: Vec<Note>,
}

/// Works out the thresholds. With no reading of a state that is needed (held, and free or released) there is nothing to propose and the answer is an error.
pub fn calibrate(r: &Readings, p: &Params) -> Result<Proposal, String> {
    if r.held.is_empty() {
        return Err("there are no readings with a part held".into());
    }
    if r.free.is_empty() && r.released.is_empty() {
        return Err("there are no readings with no part (free or released)".into());
    }
    let low: Vec<f64> = r.free.iter().chain(&r.released).copied().collect();
    let (held, low_s) = (stats(&r.held), stats(&low));
    let mut notes = Vec::new();
    for (name, n) in [
        ("free", r.free.len()),
        ("held", r.held.len()),
        ("released", r.released.len()),
    ] {
        if n > 0 && n < p.min_readings {
            notes.push(Note::FewReadings(name, n));
        }
    }
    let (Some(h), Some(l)) = (held.clone(), low_s) else {
        return Err("there are no readings".into());
    };
    let gap = h.min - l.max;
    let thresholds = if gap <= 0.0 {
        notes.push(Note::Overlap(l.max, h.min));
        None
    } else {
        if gap < p.min_gap_kpa {
            notes.push(Note::SmallGap(gap));
        }
        let m = p.margin_pct / 100.0;
        Some((h.min - m * gap, l.max + m * gap))
    };
    // how the figures in use now would have read these very readings
    let held_missed = r.held.iter().filter(|v| **v < p.current_pick_min).count();
    let gone_missed = low.iter().filter(|v| **v > p.current_release_max).count();
    if held_missed + gone_missed > 0 {
        notes.push(Note::CurrentMisreads(held_missed, gone_missed));
    }
    Ok(Proposal {
        free: stats(&r.free),
        held,
        released: stats(&r.released),
        thresholds,
        notes,
    })
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The proposal as text in a language: the figures of each state, the notes and the two lines to put in the settings.
pub fn report(p: &Proposal, lang: Lang) -> String {
    let mut out = String::new();
    let head: [&str; 8] = [
        "state        n      min     mean      max (kPa)",
        "estado       n      mín    media      máx (kPa)",
        "Zustand      n      min   Mittel      max (kPa)",
        "état         n      min    moyenne     max (kPa)",
        "stato        n      min    media      max (kPa)",
        "状態         n      最小     平均     最大 (kPa)",
        "状态         n      最小     平均     最大 (kPa)",
        "estado       n      mín    média      máx (kPa)",
    ];
    out.push_str(crate::msg8::pick(lang, &head));
    out.push('\n');
    for (name, s) in [
        ("free", &p.free),
        ("held", &p.held),
        ("released", &p.released),
    ] {
        if let Some(s) = s {
            out.push_str(&format!(
                "{:<10} {:>4} {:>8} {:>8} {:>8}\n",
                name,
                s.n,
                num(r2(s.min)),
                num(r2(s.mean)),
                num(r2(s.max))
            ));
        }
    }
    for n in &p.notes {
        let line = match n {
            Note::FewReadings(name, c) => {
                let row: [&str; 8] = [
                    "only {c} readings of '{name}': take more before trusting the figures",
                    "solo {c} lecturas de '{name}': tome más antes de fiarse de las cifras",
                    "nur {c} Messwerte von '{name}': vor dem Vertrauen auf die Zahlen mehr aufnehmen",
                    "seulement {c} mesures de '{name}' : en faire plus avant de se fier aux chiffres",
                    "solo {c} letture di '{name}': ne servono di più prima di fidarsi dei valori",
                    "「{name}」の測定は {c} 回だけです。数値を信じる前に増やしてください",
                    "“{name}”仅有 {c} 个读数：请先多取一些再信任这些数值",
                    "apenas {c} leituras de '{name}': recolha mais antes de confiar nos valores",
                ];
                say(
                    lang,
                    &row,
                    &[("c", c.to_string()), ("name", (*name).to_string())],
                )
            }
            Note::SmallGap(g) => {
                let row: [&str; 8] = [
                    "the gap between the readings with and without a part is only {g} kPa",
                    "el hueco entre las lecturas con y sin pieza es de solo {g} kPa",
                    "der Abstand zwischen den Messwerten mit und ohne Teil beträgt nur {g} kPa",
                    "l'écart entre les mesures avec et sans pièce n'est que de {g} kPa",
                    "lo scarto tra le letture con e senza componente è solo di {g} kPa",
                    "部品あり・なしの測定値の差は {g} kPa しかありません",
                    "有元件与无元件读数之间的间隔只有 {g} kPa",
                    "o intervalo entre as leituras com e sem peça é de apenas {g} kPa",
                ];
                say(lang, &row, &[("g", num(r2(*g)))])
            }
            Note::Overlap(low, high) => {
                let row: [&str; 8] = [
                    "the readings overlap: with no part up to {l} kPa, with a part from {h} kPa; no threshold can tell them apart",
                    "las lecturas se solapan: sin pieza hasta {l} kPa, con pieza desde {h} kPa; ningún umbral las distingue",
                    "die Messwerte überlappen: ohne Teil bis {l} kPa, mit Teil ab {h} kPa; keine Schwelle trennt sie",
                    "les mesures se chevauchent : sans pièce jusqu'à {l} kPa, avec pièce dès {h} kPa ; aucun seuil ne les distingue",
                    "le letture si sovrappongono: senza componente fino a {l} kPa, con componente da {h} kPa; nessuna soglia le distingue",
                    "測定値が重なっています：部品なしは {l} kPa まで、ありは {h} kPa から。しきい値では区別できません",
                    "读数重叠：无元件最高 {l} kPa，有元件最低 {h} kPa；任何阈值都无法区分",
                    "as leituras sobrepõem-se: sem peça até {l} kPa, com peça a partir de {h} kPa; nenhum limiar as distingue",
                ];
                say(lang, &row, &[("l", num(r2(*low))), ("h", num(r2(*high)))])
            }
            Note::CurrentMisreads(a, b) => {
                let row: [&str; 8] = [
                    "the thresholds in use now would misread {a} readings with a part held and {b} with no part",
                    "los umbrales actuales leerían mal {a} lecturas con pieza y {b} sin pieza",
                    "die jetzigen Schwellen würden {a} Messwerte mit Teil und {b} ohne Teil falsch lesen",
                    "les seuils actuels liraient mal {a} mesures avec pièce et {b} sans pièce",
                    "le soglie attuali leggerebbero male {a} letture con componente e {b} senza",
                    "現在のしきい値では、部品ありの {a} 件と部品なしの {b} 件を誤判定します",
                    "当前阈值会误判 {a} 个有元件读数和 {b} 个无元件读数",
                    "os limiares atuais leriam mal {a} leituras com peça e {b} sem peça",
                ];
                say(lang, &row, &[("a", a.to_string()), ("b", b.to_string())])
            }
        };
        out.push_str(&line);
        out.push('\n');
    }
    if let Some((pick, release)) = p.thresholds {
        out.push_str(&format!("vacuum_pick_min_kpa = {}\n", num(r2(pick))));
        out.push_str(&format!("vacuum_release_max_kpa = {}\n", num(r2(release))));
    }
    out
}
