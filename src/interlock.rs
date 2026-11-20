// EH3D-PHOENIX-PNP - interlocks of the axes: conditions that must hold before a move.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A machine must not move some of its axes while something is not as it should be: not in X and Y while the nozzles are not lifted, not at all while a door is open, not while the light
//! tower says stop. A rule of the profile of the driver (`interlock = SIGNAL | on|off | xy|z|rot|all`) says that the axes it names may only move while the signal is in the state it names.
//! The signals are the names of the actuators of the profile (the session knows what it last switched) and the names that a sensor is given (`Session::set_signal`). A signal that nobody
//! has reported is unknown, and an unknown signal does not satisfy a rule: the move is refused. A refused move is a failure of safety, the same as a move outside the limits: the session
//! sends the lines of `safe_stop` and stops. Everything here is a calculation on names and numbers, so it is tested without a controller; whether a real door sensor reports what it should
//! is the one thing that only a machine can show.

use crate::error::Lang;
use crate::gcodedriver::Target;
use crate::msg8::{pick, say};
use std::collections::BTreeMap;

/// The longest name of a signal.
pub const MAX_SIGNAL: usize = 32;

/// The signals as the session knows them: a name and whether it is on.
pub type Signals = BTreeMap<String, bool>;

/// The axes a rule guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axes {
    /// The two axes of the carriage.
    Xy,
    /// The axis that lifts and lowers the nozzles.
    Z,
    /// The axis that turns the nozzle.
    Rot,
    /// Every axis.
    All,
}

impl Axes {
    /// The word of the axes in a profile file.
    pub fn name(self) -> &'static str {
        match self {
            Axes::Xy => "xy",
            Axes::Z => "z",
            Axes::Rot => "rot",
            Axes::All => "all",
        }
    }

    /// The axes that a word names, without regard to case.
    pub fn parse(text: &str) -> Option<Axes> {
        match text.trim().to_ascii_lowercase().as_str() {
            "xy" => Some(Axes::Xy),
            "z" => Some(Axes::Z),
            "rot" => Some(Axes::Rot),
            "all" => Some(Axes::All),
            _ => None,
        }
    }

    /// Whether a move asks any of these axes to move. An axis the move does not name stays where it is.
    pub fn touched_by(self, target: &Target) -> bool {
        let xy = target.x.is_some() || target.y.is_some();
        let z = target.z.is_some();
        let rot = target.rot.is_some();
        match self {
            Axes::Xy => xy,
            Axes::Z => z,
            Axes::Rot => rot,
            Axes::All => xy || z || rot,
        }
    }
}

/// A rule: the axes may only move while the signal is in the required state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interlock {
    pub signal: String,
    pub required: bool,
    pub axes: Axes,
}

impl Interlock {
    /// Reads `SIGNAL | on|off | xy|z|rot|all`. The name has letters, digits and `_` and is at most `MAX_SIGNAL` long.
    pub fn parse(value: &str) -> Result<Interlock, String> {
        let parts: Vec<&str> = value.split('|').map(str::trim).collect();
        let [name, state, axes] = parts.as_slice() else {
            return Err("signal | on or off | xy, z, rot or all".to_string());
        };
        let name_ok = !name.is_empty()
            && name.len() <= MAX_SIGNAL
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let required = match state.to_ascii_lowercase().as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        };
        let axes = Axes::parse(axes);
        match (name_ok, required, axes) {
            (true, Some(required), Some(axes)) => Ok(Interlock {
                signal: (*name).to_string(),
                required,
                axes,
            }),
            _ => Err("signal | on or off | xy, z, rot or all".to_string()),
        }
    }

    /// The rule as a profile file writes it.
    pub fn to_line(&self) -> String {
        format!(
            "{} | {} | {}",
            self.signal,
            if self.required { "on" } else { "off" },
            self.axes.name()
        )
    }
}

/// Why a move was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub signal: String,
    pub required: bool,
    /// The state the signal is in, or `None` when nobody has reported it.
    pub found: Option<bool>,
    pub axes: Axes,
}

impl Violation {
    /// The refusal in a language.
    pub fn localized(&self, lang: Lang) -> String {
        let on: [&str; 8] = [
            "on", "activa", "an", "active", "attiva", "オン", "开", "ativa",
        ];
        let off: [&str; 8] = [
            "off", "inactiva", "aus", "inactive", "inattiva", "オフ", "关", "inativa",
        ];
        let unknown: [&str; 8] = [
            "unknown",
            "desconocida",
            "unbekannt",
            "inconnue",
            "sconosciuta",
            "不明",
            "未知",
            "desconhecida",
        ];
        let state = |s: Option<bool>| match s {
            Some(true) => pick(lang, &on),
            Some(false) => pick(lang, &off),
            None => pick(lang, &unknown),
        };
        let rows: [&str; 8] = [
            "interlock: the {axes} axes may only move while '{signal}' is {state}, and it is {found}",
            "bloqueo: los ejes {axes} solo pueden moverse mientras '{signal}' esté {state}, y está {found}",
            "Verriegelung: die Achsen {axes} dürfen sich nur bewegen, solange '{signal}' {state} ist, und es ist {found}",
            "verrouillage : les axes {axes} ne peuvent bouger que lorsque '{signal}' est {state}, et il est {found}",
            "interblocco: gli assi {axes} possono muoversi solo mentre '{signal}' è {state}, ed è {found}",
            "インターロック：「{signal}」が{state}の間だけ {axes} 軸を動かせますが、現在は{found}です",
            "互锁：仅当“{signal}”为{state}时，{axes} 轴才可移动，而它现在是{found}",
            "bloqueio: os eixos {axes} só se podem mover enquanto '{signal}' estiver {state}, e está {found}",
        ];
        say(
            lang,
            &rows,
            &[
                ("axes", self.axes.name().to_string()),
                ("signal", self.signal.clone()),
                ("state", state(Some(self.required)).to_string()),
                ("found", state(self.found).to_string()),
            ],
        )
    }
}

/// Whether a move may be made: every rule that guards an axis the move touches must find its signal in the state it requires. The first rule that does not is the answer. A signal that is
/// not in `signals` is unknown, and unknown does not satisfy a rule.
pub fn check(rules: &[Interlock], signals: &Signals, target: &Target) -> Result<(), Violation> {
    for rule in rules {
        if !rule.axes.touched_by(target) {
            continue;
        }
        let found = signals.get(&rule.signal).copied();
        if found != Some(rule.required) {
            return Err(Violation {
                signal: rule.signal.clone(),
                required: rule.required,
                found,
                axes: rule.axes,
            });
        }
    }
    Ok(())
}
