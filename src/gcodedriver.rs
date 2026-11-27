// EH3D-PHOENIX-PNP - the profile of a G-code driver: what a Marlin or a Klipper controller is told and how it answers, kept in a file.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A machine whose controller speaks G-code (Marlin through a serial line, Klipper through Moonraker) is described by a file of `key = value` lines and not by code: the lines that
//! connect, home and stop it, the template of a move, the lines that switch a pump or a valve, the patterns of an answer (`ok`, an error, a busy line, a position), the times and the
//! retries. `DriverProfile` reads that file and checks every value against its range, `Op` is a step the driver can be asked to do, and `lines` turns a step into the lines that go to the
//! controller (a move is always followed by the line that waits for it). Nothing here opens a port: the only controller it talks to is the simulated one (`gcodesim`), and what it
//! shows is evidence of a simulator.

use crate::error::Lang;
use crate::interlock::Interlock;
use crate::msg8::{num, say};
use crate::plan::Placement;
use crate::textmatch::{Captures, Pattern};
use serde_json::{json, Value};

/// The version of the file of a profile that this program reads.
pub const SCHEMA_VERSION: u32 = 1;
/// The most lines of a list key (`connect`, `home`, ...), and the most actuators.
pub const MAX_LIST: usize = 32;
/// The most letters of a line of G-code.
pub const MAX_LINE: usize = 200;
/// The largest profile file (bytes).
pub const MAX_PROFILE_BYTES: usize = 65_536;

/// Every key of a profile file, in the order of the examples.
pub const KEYS: [&str; 62] = [
    "schema_version",
    "dialect",
    "transport",
    "axis_x",
    "axis_y",
    "axis_z",
    "axis_rot",
    "connect",
    "disable",
    "home",
    "safe_stop",
    "wait",
    "position_request",
    "absolute_mode",
    "move_command",
    "dwell",
    "actuator",
    "pump",
    "release",
    "pump_settle_ms",
    "release_settle_ms",
    "interlock",
    "ok_pattern",
    "error_pattern",
    "busy_pattern",
    "ignore_pattern",
    "position_pattern",
    "feed_travel_mm_min",
    "feed_z_mm_min",
    "feed_max_mm_min",
    "z_travel_mm",
    "z_pick_mm",
    "z_place_mm",
    "limit_x_min_mm",
    "limit_y_min_mm",
    "limit_x_mm",
    "limit_y_mm",
    "limit_z_min_mm",
    "limit_z_max_mm",
    "nozzles",
    "command_timeout_s",
    "move_timeout_s",
    "connect_timeout_s",
    "retries",
    "retry_wait_s",
    "position_tol_mm",
    "moonraker_script_path",
    "moonraker_query_path",
    "safety_careful_events",
    "safety_slow_events",
    "safety_hold_events",
    "safety_careful_speed_pct",
    "safety_slow_speed_pct",
    "baud",
    "homing_spread_mm",
    "homing_mode",
    "homing_sensitivity_x",
    "homing_sensitivity_y",
    "homing_sensitivity_line",
    "bringup_probe_mm",
    "tolerate_unknown_echo",
    "stop_on_unknown_command",
];

/// The keys that may be written more than once.
const LISTS: [&str; 6] = [
    "connect",
    "disable",
    "home",
    "safe_stop",
    "actuator",
    "interlock",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Marlin,
    Klipper,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Marlin => "marlin",
            Dialect::Klipper => "klipper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Serial,
    Moonraker,
}

impl Transport {
    pub fn name(self) -> &'static str {
        match self {
            Transport::Serial => "serial",
            Transport::Moonraker => "moonraker",
        }
    }
}

/// How the machine finds its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomingMode {
    /// The axes run to end switches (the lines of `home` alone).
    Switch,
    /// The axes run to a stop and the driver of the motor feels it: a sensitivity for X and one for Y are sent first, with `homing_sensitivity_line`.
    Sensorless,
}

impl HomingMode {
    pub fn name(self) -> &'static str {
        match self {
            HomingMode::Switch => "switch",
            HomingMode::Sensorless => "sensorless",
        }
    }
}

/// The lowest and the highest sensitivity a profile may give for sensorless homing (the drivers of the motors take different ranges; the profile says which number its line wants).
pub const SENSITIVITY_RANGE: (f64, f64) = (-64.0, 255.0);

/// A pin, a valve or a pump that two lines switch.
#[derive(Debug, Clone, PartialEq)]
pub struct Actuator {
    pub name: String,
    pub on: String,
    pub off: String,
}

/// What is wrong in a line of a profile.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    Syntax,
    UnknownKey,
    Duplicate,
    BadValue(String),
    OutOfRange(f64, f64),
    BadPattern(String),
    NoActuator(String),
    TooMany(usize),
    LineTooLong(usize),
    Order(String),
    Missing(String),
    TooBig(usize),
}

/// An error of a profile: the line (0 when it is of the whole file), the key and what is wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileError {
    pub line: usize,
    pub key: String,
    pub fault: Fault,
}

impl ProfileError {
    fn new(line: usize, key: &str, fault: Fault) -> ProfileError {
        ProfileError {
            line,
            key: key.to_string(),
            fault,
        }
    }

    /// The error in a language.
    pub fn localized(&self, lang: Lang) -> String {
        let (a, b) = match &self.fault {
            Fault::BadValue(w) | Fault::BadPattern(w) | Fault::Order(w) | Fault::Missing(w) => {
                (w.clone(), String::new())
            }
            Fault::OutOfRange(lo, hi) => (num(*lo), num(*hi)),
            Fault::NoActuator(n) => (n.clone(), String::new()),
            Fault::TooMany(n) | Fault::LineTooLong(n) | Fault::TooBig(n) => {
                (n.to_string(), String::new())
            }
            _ => (String::new(), String::new()),
        };
        let row: [&str; 8] = match &self.fault {
            Fault::Syntax => [
                "line {line}: not a line of the form key = value",
                "línea {line}: no es una línea de la forma clave = valor",
                "Zeile {line}: keine Zeile der Form Schlüssel = Wert",
                "ligne {line} : pas une ligne de la forme clé = valeur",
                "riga {line}: non è una riga della forma chiave = valore",
                "{line} 行目：「キー = 値」の形ではありません",
                "第 {line} 行：不是“键 = 值”的形式",
                "linha {line}: não é uma linha da forma chave = valor",
            ],
            Fault::UnknownKey => [
                "line {line}: '{key}' is not a key of a profile",
                "línea {line}: '{key}' no es una clave de un perfil",
                "Zeile {line}: '{key}' ist kein Schlüssel eines Profils",
                "ligne {line} : '{key}' n'est pas une clé d'un profil",
                "riga {line}: '{key}' non è una chiave di un profilo",
                "{line} 行目：「{key}」はプロファイルのキーではありません",
                "第 {line} 行：“{key}”不是配置档的键",
                "linha {line}: '{key}' não é uma chave de um perfil",
            ],
            Fault::Duplicate => [
                "line {line}: '{key}' is given twice",
                "línea {line}: '{key}' está dada dos veces",
                "Zeile {line}: '{key}' ist doppelt angegeben",
                "ligne {line} : '{key}' est donnée deux fois",
                "riga {line}: '{key}' è data due volte",
                "{line} 行目：「{key}」が二重に指定されています",
                "第 {line} 行：“{key}”重复给出",
                "linha {line}: '{key}' está dada duas vezes",
            ],
            Fault::BadValue(_) => [
                "line {line}: '{key}' needs {a}",
                "línea {line}: '{key}' necesita {a}",
                "Zeile {line}: '{key}' braucht {a}",
                "ligne {line} : '{key}' demande {a}",
                "riga {line}: '{key}' richiede {a}",
                "{line} 行目：「{key}」には {a} が必要です",
                "第 {line} 行：“{key}”需要 {a}",
                "linha {line}: '{key}' precisa de {a}",
            ],
            Fault::OutOfRange(..) => [
                "line {line}: '{key}' must be from {a} to {b}",
                "línea {line}: '{key}' debe estar entre {a} y {b}",
                "Zeile {line}: '{key}' muss von {a} bis {b} reichen",
                "ligne {line} : '{key}' doit aller de {a} à {b}",
                "riga {line}: '{key}' deve andare da {a} a {b}",
                "{line} 行目：「{key}」は {a} から {b} の範囲で指定します",
                "第 {line} 行：“{key}”必须在 {a} 到 {b} 之间",
                "linha {line}: '{key}' tem de estar entre {a} e {b}",
            ],
            Fault::BadPattern(_) => [
                "line {line}: the pattern of '{key}' is not valid: {a}",
                "línea {line}: el patrón de '{key}' no es válido: {a}",
                "Zeile {line}: das Muster von '{key}' ist ungültig: {a}",
                "ligne {line} : le motif de '{key}' n'est pas valide : {a}",
                "riga {line}: il modello di '{key}' non è valido: {a}",
                "{line} 行目：「{key}」のパターンが正しくありません：{a}",
                "第 {line} 行：“{key}”的模式无效：{a}",
                "linha {line}: o padrão de '{key}' não é válido: {a}",
            ],
            Fault::NoActuator(_) => [
                "line {line}: '{key}' names '{a}', which no 'actuator' line defines",
                "línea {line}: '{key}' nombra '{a}', que ninguna línea 'actuator' define",
                "Zeile {line}: '{key}' nennt '{a}', das keine 'actuator'-Zeile festlegt",
                "ligne {line} : '{key}' nomme '{a}', qu'aucune ligne 'actuator' ne définit",
                "riga {line}: '{key}' nomina '{a}', che nessuna riga 'actuator' definisce",
                "{line} 行目：「{key}」が指す「{a}」を定義する actuator 行がありません",
                "第 {line} 行：“{key}”指向“{a}”，但没有 actuator 行定义它",
                "linha {line}: '{key}' nomeia '{a}', que nenhuma linha 'actuator' define",
            ],
            Fault::TooMany(_) => [
                "line {line}: '{key}' has more than {a} lines",
                "línea {line}: '{key}' tiene más de {a} líneas",
                "Zeile {line}: '{key}' hat mehr als {a} Zeilen",
                "ligne {line} : '{key}' a plus de {a} lignes",
                "riga {line}: '{key}' ha più di {a} righe",
                "{line} 行目：「{key}」が {a} 行を超えています",
                "第 {line} 行：“{key}”超过 {a} 行",
                "linha {line}: '{key}' tem mais de {a} linhas",
            ],
            Fault::LineTooLong(_) => [
                "line {line}: the line of '{key}' is over {a} letters",
                "línea {line}: la línea de '{key}' pasa de {a} letras",
                "Zeile {line}: die Zeile von '{key}' hat über {a} Zeichen",
                "ligne {line} : la ligne de '{key}' dépasse {a} caractères",
                "riga {line}: la riga di '{key}' supera {a} caratteri",
                "{line} 行目：「{key}」の行が {a} 文字を超えています",
                "第 {line} 行：“{key}”的行超过 {a} 个字符",
                "linha {line}: a linha de '{key}' passa de {a} letras",
            ],
            Fault::Order(_) => [
                "'{key}': {a}",
                "'{key}': {a}",
                "'{key}': {a}",
                "'{key}' : {a}",
                "'{key}': {a}",
                "「{key}」：{a}",
                "“{key}”：{a}",
                "'{key}': {a}",
            ],
            Fault::Missing(_) => [
                "the profile has no '{key}' ({a})",
                "el perfil no tiene '{key}' ({a})",
                "das Profil hat kein '{key}' ({a})",
                "le profil n'a pas de '{key}' ({a})",
                "il profilo non ha '{key}' ({a})",
                "プロファイルに「{key}」がありません（{a}）",
                "配置档缺少“{key}”（{a}）",
                "o perfil não tem '{key}' ({a})",
            ],
            Fault::TooBig(_) => [
                "the profile is over {a} bytes",
                "el perfil pasa de {a} bytes",
                "das Profil ist größer als {a} Bytes",
                "le profil dépasse {a} octets",
                "il profilo supera {a} byte",
                "プロファイルが {a} バイトを超えています",
                "配置档超过 {a} 字节",
                "o perfil passa de {a} bytes",
            ],
        };
        say(
            lang,
            &row,
            &[
                ("line", self.line.to_string()),
                ("key", self.key.clone()),
                ("a", a),
                ("b", b),
            ],
        )
    }
}

/// A profile that was read and checked.
#[derive(Debug, Clone)]
pub struct DriverProfile {
    pub dialect: Dialect,
    pub transport: Transport,
    pub axis_x: String,
    pub axis_y: String,
    pub axis_z: String,
    /// The axis that turns the nozzle; empty when the machine has none.
    pub axis_rot: String,
    pub connect: Vec<String>,
    pub disable: Vec<String>,
    pub home: Vec<String>,
    pub safe_stop: Vec<String>,
    pub wait: String,
    pub position_request: String,
    pub absolute_mode: String,
    pub move_command: String,
    pub dwell: String,
    pub actuators: Vec<Actuator>,
    pub pump: String,
    pub release: String,
    pub pump_settle_ms: u32,
    pub release_settle_ms: u32,
    /// The rules that keep an axis from moving while a signal is not as it should be (`interlock = SIGNAL | on|off | xy|z|rot|all`).
    pub interlocks: Vec<Interlock>,
    pub ok: Pattern,
    pub error: Pattern,
    pub busy: Pattern,
    pub ignore: Pattern,
    pub position: Pattern,
    pub feed_travel: f64,
    pub feed_z: f64,
    pub feed_max: f64,
    pub z_travel: f64,
    pub z_pick: f64,
    pub z_place: f64,
    pub limit_x_min: f64,
    pub limit_y_min: f64,
    pub limit_x: f64,
    pub limit_y: f64,
    pub limit_z_min: f64,
    pub limit_z_max: f64,
    pub nozzles: usize,
    pub command_timeout_s: f64,
    pub move_timeout_s: f64,
    pub connect_timeout_s: f64,
    pub retries: u32,
    pub retry_wait_s: f64,
    pub position_tol_mm: f64,
    pub moonraker_script_path: String,
    pub moonraker_query_path: String,
    pub careful_events: u32,
    pub slow_events: u32,
    pub hold_events: u32,
    pub careful_speed_pct: f64,
    pub slow_speed_pct: f64,
    pub baud: u32,
    pub homing_spread_mm: f64,
    /// Homing with end switches or without them.
    pub homing_mode: HomingMode,
    /// The sensitivity of X and of Y for sensorless homing; none when the profile gives none.
    pub homing_sensitivity: [Option<i32>; 2],
    /// The line that sets a sensitivity: `{axis}` is the letter of the axis (`{axis_lower}` its lower case) and `{value}` the sensitivity.
    pub homing_sensitivity_line: String,
    pub bringup_probe_mm: f64,
    pub tolerate_unknown_echo: bool,
    /// A controller that says it does not know a line (and then answers `ok`) is read as an error, so that a pump that was never switched is not taken for switched.
    pub stop_on_unknown_command: bool,
}

/// A place the head is asked to go to; an axis that is `None` is left where it is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Target {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub rot: Option<f64>,
}

/// A position that the controller reported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub rot: Option<f64>,
}

/// A step that the driver can be asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Move(Target, f64),
    Actuator(String, bool),
    Dwell(u32),
    Wait,
    Query,
}

/// How a line from the controller reads.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok,
    Error(String),
    Busy,
    Echo(String),
    Position(Pos),
    Other(String),
}

fn strip(v: &str) -> String {
    v.trim().to_string()
}

impl Default for DriverProfile {
    /// The profile of a Marlin controller on a serial line; the examples of `examples/driver_*.cfg` give the same values with their reasons.
    fn default() -> DriverProfile {
        let pat = |s: &str| Pattern::new(s).expect("a pattern of the defaults is valid");
        DriverProfile {
            dialect: Dialect::Marlin,
            transport: Transport::Serial,
            axis_x: "X".into(),
            axis_y: "Y".into(),
            axis_z: "Z".into(),
            axis_rot: "E".into(),
            connect: vec!["M115".into()],
            disable: vec!["M84".into()],
            home: vec!["G28".into()],
            safe_stop: vec!["M42 P5 S0".into(), "M42 P6 S0".into(), "M84".into()],
            wait: "M400".into(),
            position_request: "M114".into(),
            absolute_mode: "G90".into(),
            move_command: "G1".into(),
            dwell: "G4 P{ms}".into(),
            actuators: vec![
                Actuator {
                    name: "pump".into(),
                    on: "M42 P5 S255".into(),
                    off: "M42 P5 S0".into(),
                },
                Actuator {
                    name: "blow".into(),
                    on: "M42 P6 S255".into(),
                    off: "M42 P6 S0".into(),
                },
            ],
            pump: "pump".into(),
            release: "blow".into(),
            pump_settle_ms: 150,
            release_settle_ms: 80,
            interlocks: Vec::new(),
            ok: pat(r"^ok"),
            error: pat(r"(?i)^(error|!!)"),
            busy: pat(r"(?i)^(echo:)?busy"),
            ignore: pat(r"^echo:"),
            position: pat(r"X:(-?\d+\.?\d*) Y:(-?\d+\.?\d*) Z:(-?\d+\.?\d*)(?: E:(-?\d+\.?\d*))?"),
            feed_travel: 12000.0,
            feed_z: 3000.0,
            feed_max: 20000.0,
            z_travel: 10.0,
            z_pick: 0.5,
            z_place: 1.0,
            limit_x_min: 0.0,
            limit_y_min: 0.0,
            limit_x: 400.0,
            limit_y: 300.0,
            limit_z_min: 0.0,
            limit_z_max: 40.0,
            nozzles: 1,
            command_timeout_s: 5.0,
            move_timeout_s: 30.0,
            connect_timeout_s: 10.0,
            retries: 2,
            retry_wait_s: 0.25,
            position_tol_mm: 0.05,
            moonraker_script_path: "/printer/gcode/script".into(),
            moonraker_query_path: "/printer/objects/query?toolhead".into(),
            careful_events: 1,
            slow_events: 3,
            hold_events: 5,
            careful_speed_pct: 60.0,
            slow_speed_pct: 25.0,
            baud: 115_200,
            homing_spread_mm: 0.05,
            homing_mode: HomingMode::Switch,
            homing_sensitivity: [None, None],
            homing_sensitivity_line: "M914 {axis}{value}".into(),
            bringup_probe_mm: 20.0,
            tolerate_unknown_echo: true,
            stop_on_unknown_command: true,
        }
    }
}

fn yes_no(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "yes" | "on" | "true" | "1" => Some(true),
        "no" | "off" | "false" | "0" => Some(false),
        _ => None,
    }
}

impl DriverProfile {
    /// Reads a profile. Every key has a range or a short list of words; a key that is not in the list, a key given twice (but a list key) and a pattern that does not compile are errors with
    /// their line. The profile that is read starts from `DriverProfile::default()` and each line changes one thing.
    pub fn parse(text: &str) -> Result<DriverProfile, ProfileError> {
        if text.len() > MAX_PROFILE_BYTES {
            return Err(ProfileError::new(0, "", Fault::TooBig(MAX_PROFILE_BYTES)));
        }
        let mut p = DriverProfile::default();
        let mut seen: Vec<String> = Vec::new();
        let mut lists_started: Vec<&str> = Vec::new();
        let mut patterns: Vec<(usize, String, String)> = Vec::new();
        let mut last_line = 0usize;
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line_no = i + 1;
            last_line = line_no;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(ProfileError::new(line_no, "", Fault::Syntax));
            };
            let key = k.trim().to_ascii_lowercase();
            let v = strip(v);
            let Some(&known) = KEYS.iter().find(|n| **n == key) else {
                return Err(ProfileError::new(line_no, &key, Fault::UnknownKey));
            };
            if v.chars().count() > MAX_LINE {
                return Err(ProfileError::new(
                    line_no,
                    known,
                    Fault::LineTooLong(MAX_LINE),
                ));
            }
            if !LISTS.contains(&known) {
                if seen.iter().any(|s| s == known) {
                    return Err(ProfileError::new(line_no, known, Fault::Duplicate));
                }
                seen.push(known.to_string());
            }
            p.set(line_no, known, &v, &mut lists_started, &mut patterns)?;
        }
        for (line, key, source) in patterns {
            let compiled = Pattern::new(&source)
                .map_err(|e| ProfileError::new(line, &key, Fault::BadPattern(e)))?;
            match key.as_str() {
                "ok_pattern" => p.ok = compiled,
                "error_pattern" => p.error = compiled,
                "busy_pattern" => p.busy = compiled,
                "ignore_pattern" => p.ignore = compiled,
                _ => {
                    if compiled.groups() < 2 {
                        return Err(ProfileError::new(
                            line,
                            &key,
                            Fault::BadPattern(
                                "it needs groups for X and Y (and Z, and the turn)".into(),
                            ),
                        ));
                    }
                    p.position = compiled;
                }
            }
        }
        p.check_whole(last_line)?;
        Ok(p)
    }

    fn set(
        &mut self,
        line: usize,
        key: &str,
        v: &str,
        started: &mut Vec<&'static str>,
        patterns: &mut Vec<(usize, String, String)>,
    ) -> Result<(), ProfileError> {
        let err = |f: Fault| ProfileError::new(line, key, f);
        let number = |lo: f64, hi: f64| -> Result<f64, ProfileError> {
            let n: f64 = v
                .parse()
                .ok()
                .filter(|n: &f64| n.is_finite())
                .ok_or_else(|| err(Fault::BadValue("a number".into())))?;
            if (lo..=hi).contains(&n) {
                Ok(n)
            } else {
                Err(err(Fault::OutOfRange(lo, hi)))
            }
        };
        let whole = |lo: f64, hi: f64| -> Result<u32, ProfileError> {
            let n = number(lo, hi)?;
            if n.fract() != 0.0 {
                return Err(err(Fault::BadValue("a whole number".into())));
            }
            Ok(n as u32)
        };
        let word = |max: usize| -> Result<String, ProfileError> {
            if v.is_empty() || v.chars().count() > max || v.chars().any(char::is_control) {
                return Err(err(Fault::BadValue(format!("text of 1 to {max} letters"))));
            }
            Ok(v.to_string())
        };
        let gcode = || -> Result<String, ProfileError> {
            if v.is_empty() || v.chars().any(|c| c.is_control() || c == ';') {
                return Err(err(Fault::BadValue(
                    "a line of G-code without ';' or control letters".into(),
                )));
            }
            Ok(v.to_string())
        };
        let mut push_list =
            |list: &mut Vec<String>, name: &'static str| -> Result<(), ProfileError> {
                if !started.contains(&name) {
                    started.push(name);
                    list.clear();
                }
                if list.len() >= MAX_LIST {
                    return Err(err(Fault::TooMany(MAX_LIST)));
                }
                list.push(gcode()?);
                Ok(())
            };
        match key {
            "schema_version" => {
                if whole(1.0, 1000.0)? > SCHEMA_VERSION {
                    return Err(err(Fault::BadValue(format!(
                        "a schema up to {SCHEMA_VERSION} (this file is newer)"
                    ))));
                }
            }
            "dialect" => {
                self.dialect = match v.to_ascii_lowercase().as_str() {
                    "marlin" => Dialect::Marlin,
                    "klipper" => Dialect::Klipper,
                    _ => return Err(err(Fault::BadValue("marlin or klipper".into()))),
                }
            }
            "transport" => {
                self.transport = match v.to_ascii_lowercase().as_str() {
                    "serial" => Transport::Serial,
                    "moonraker" => Transport::Moonraker,
                    _ => return Err(err(Fault::BadValue("serial or moonraker".into()))),
                }
            }
            "axis_x" => {
                self.axis_x =
                    axis_name(v).ok_or_else(|| err(Fault::BadValue("one letter A to Z".into())))?
            }
            "axis_y" => {
                self.axis_y =
                    axis_name(v).ok_or_else(|| err(Fault::BadValue("one letter A to Z".into())))?
            }
            "axis_z" => {
                self.axis_z =
                    axis_name(v).ok_or_else(|| err(Fault::BadValue("one letter A to Z".into())))?
            }
            "axis_rot" => {
                self.axis_rot = if v.eq_ignore_ascii_case("none") {
                    String::new()
                } else {
                    axis_name(v)
                        .ok_or_else(|| err(Fault::BadValue("one letter A to Z, or none".into())))?
                }
            }
            "connect" => push_list(&mut self.connect, "connect")?,
            "disable" => push_list(&mut self.disable, "disable")?,
            "home" => push_list(&mut self.home, "home")?,
            "safe_stop" => push_list(&mut self.safe_stop, "safe_stop")?,
            "wait" => self.wait = gcode()?,
            "position_request" => self.position_request = gcode()?,
            "absolute_mode" => self.absolute_mode = gcode()?,
            "move_command" => self.move_command = gcode()?,
            "dwell" => {
                let g = gcode()?;
                if !g.contains("{ms}") {
                    return Err(err(Fault::BadValue(
                        "a line with {ms} where the milliseconds go".into(),
                    )));
                }
                self.dwell = g;
            }
            "actuator" => {
                if !started.contains(&"actuator") {
                    started.push("actuator");
                    self.actuators.clear();
                }
                if self.actuators.len() >= MAX_LIST {
                    return Err(err(Fault::TooMany(MAX_LIST)));
                }
                let parts: Vec<&str> = v.split('|').map(str::trim).collect();
                let valid = parts.len() == 3
                    && parts
                        .iter()
                        .all(|s| !s.is_empty() && !s.chars().any(|c| c.is_control() || c == ';'))
                    && parts[0]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid {
                    return Err(err(Fault::BadValue(
                        "name | line that switches it on | line that switches it off".into(),
                    )));
                }
                if self.actuators.iter().any(|a| a.name == parts[0]) {
                    return Err(err(Fault::Duplicate));
                }
                self.actuators.push(Actuator {
                    name: parts[0].to_string(),
                    on: parts[1].to_string(),
                    off: parts[2].to_string(),
                });
            }
            "interlock" => {
                if !started.contains(&"interlock") {
                    started.push("interlock");
                    self.interlocks.clear();
                }
                if self.interlocks.len() >= MAX_LIST {
                    return Err(err(Fault::TooMany(MAX_LIST)));
                }
                let rule = Interlock::parse(v).map_err(|e| err(Fault::BadValue(e)))?;
                if self.interlocks.contains(&rule) {
                    return Err(err(Fault::Duplicate));
                }
                self.interlocks.push(rule);
            }
            "pump" => self.pump = word(32)?,
            "release" => {
                self.release = if v.eq_ignore_ascii_case("none") {
                    String::new()
                } else {
                    word(32)?
                }
            }
            "pump_settle_ms" => self.pump_settle_ms = whole(0.0, 10_000.0)?,
            "release_settle_ms" => self.release_settle_ms = whole(0.0, 10_000.0)?,
            "ok_pattern" | "error_pattern" | "busy_pattern" | "ignore_pattern"
            | "position_pattern" => patterns.push((line, key.to_string(), v.to_string())),
            "feed_travel_mm_min" => self.feed_travel = number(1.0, 200_000.0)?,
            "feed_z_mm_min" => self.feed_z = number(1.0, 200_000.0)?,
            "feed_max_mm_min" => self.feed_max = number(1.0, 200_000.0)?,
            "z_travel_mm" => self.z_travel = number(-500.0, 500.0)?,
            "z_pick_mm" => self.z_pick = number(-500.0, 500.0)?,
            "z_place_mm" => self.z_place = number(-500.0, 500.0)?,
            "limit_x_min_mm" => self.limit_x_min = number(-5000.0, 5000.0)?,
            "limit_y_min_mm" => self.limit_y_min = number(-5000.0, 5000.0)?,
            "limit_x_mm" => self.limit_x = number(1.0, 5000.0)?,
            "limit_y_mm" => self.limit_y = number(1.0, 5000.0)?,
            "limit_z_min_mm" => self.limit_z_min = number(-500.0, 500.0)?,
            "limit_z_max_mm" => self.limit_z_max = number(-500.0, 500.0)?,
            "nozzles" => self.nozzles = whole(1.0, 6.0)? as usize,
            "command_timeout_s" => self.command_timeout_s = number(0.05, 600.0)?,
            "move_timeout_s" => self.move_timeout_s = number(0.05, 3600.0)?,
            "connect_timeout_s" => self.connect_timeout_s = number(0.05, 600.0)?,
            "retries" => self.retries = whole(0.0, 10.0)?,
            "retry_wait_s" => self.retry_wait_s = number(0.0, 60.0)?,
            "position_tol_mm" => self.position_tol_mm = number(0.0, 5.0)?,
            "moonraker_script_path" => {
                self.moonraker_script_path = path_text(v)
                    .ok_or_else(|| err(Fault::BadValue("a path that starts with '/'".into())))?
            }
            "moonraker_query_path" => {
                self.moonraker_query_path = path_text(v)
                    .ok_or_else(|| err(Fault::BadValue("a path that starts with '/'".into())))?
            }
            "safety_careful_events" => self.careful_events = whole(1.0, 1000.0)?,
            "safety_slow_events" => self.slow_events = whole(1.0, 1000.0)?,
            "safety_hold_events" => self.hold_events = whole(1.0, 1000.0)?,
            "safety_careful_speed_pct" => self.careful_speed_pct = number(1.0, 100.0)?,
            "safety_slow_speed_pct" => self.slow_speed_pct = number(1.0, 100.0)?,
            "baud" => self.baud = whole(1200.0, 4_000_000.0)?,
            "homing_spread_mm" => self.homing_spread_mm = number(0.0, 5.0)?,
            "homing_mode" => {
                self.homing_mode = match v.to_ascii_lowercase().as_str() {
                    "switch" => HomingMode::Switch,
                    "sensorless" => HomingMode::Sensorless,
                    _ => return Err(err(Fault::BadValue("switch or sensorless".into()))),
                }
            }
            "homing_sensitivity_x" | "homing_sensitivity_y" => {
                let n = number(SENSITIVITY_RANGE.0, SENSITIVITY_RANGE.1)?;
                if n.fract() != 0.0 {
                    return Err(err(Fault::BadValue("a whole number".into())));
                }
                self.homing_sensitivity[usize::from(key == "homing_sensitivity_y")] =
                    Some(n as i32);
            }
            "homing_sensitivity_line" => {
                let g = gcode()?;
                if !g.contains("{value}") || !(g.contains("{axis}") || g.contains("{axis_lower}")) {
                    return Err(err(Fault::BadValue(
                        "a line with {axis} (or {axis_lower}) and {value}".into(),
                    )));
                }
                self.homing_sensitivity_line = g;
            }
            "bringup_probe_mm" => self.bringup_probe_mm = number(1.0, 500.0)?,
            "tolerate_unknown_echo" => {
                self.tolerate_unknown_echo =
                    yes_no(v).ok_or_else(|| err(Fault::BadValue("yes or no".into())))?
            }
            "stop_on_unknown_command" => {
                self.stop_on_unknown_command =
                    yes_no(v).ok_or_else(|| err(Fault::BadValue("yes or no".into())))?
            }
            _ => return Err(err(Fault::UnknownKey)),
        }
        Ok(())
    }

    /// The checks of the whole profile: values that depend on each other.
    fn check_whole(&self, last_line: usize) -> Result<(), ProfileError> {
        let order = |key: &str, what: &str| {
            ProfileError::new(last_line, key, Fault::Order(what.to_string()))
        };
        if self.feed_travel > self.feed_max || self.feed_z > self.feed_max {
            return Err(order(
                "feed_max_mm_min",
                "the feeds of the moves may not be over the greatest feed",
            ));
        }
        if self.limit_x_min >= self.limit_x || self.limit_y_min >= self.limit_y {
            return Err(order(
                "limit_x_min_mm",
                "the least place of X and of Y must be under the greatest",
            ));
        }
        if self.limit_z_min >= self.limit_z_max {
            return Err(order(
                "limit_z_min_mm",
                "the least height must be under the greatest",
            ));
        }
        for (key, z) in [
            ("z_travel_mm", self.z_travel),
            ("z_pick_mm", self.z_pick),
            ("z_place_mm", self.z_place),
        ] {
            if !(self.limit_z_min..=self.limit_z_max).contains(&z) {
                return Err(order(key, "the height is outside the limits of Z"));
            }
        }
        if self.z_travel < self.z_pick || self.z_travel < self.z_place {
            return Err(order(
                "z_travel_mm",
                "the height of travel may not be under the height of a pick or a place",
            ));
        }
        if !(self.careful_events < self.slow_events && self.slow_events < self.hold_events) {
            return Err(order(
                "safety_slow_events",
                "the events for careful, slow and hold must grow",
            ));
        }
        if self.slow_speed_pct > self.careful_speed_pct {
            return Err(order(
                "safety_slow_speed_pct",
                "a slower step may not be faster than the one before it",
            ));
        }
        for name in [&self.pump, &self.release] {
            if !name.is_empty() && !self.actuators.iter().any(|a| &a.name == name) {
                let key = if name == &self.pump {
                    "pump"
                } else {
                    "release"
                };
                return Err(ProfileError::new(0, key, Fault::NoActuator(name.clone())));
            }
        }
        if self.transport == Transport::Moonraker {
            // the link writes `ok`, `!! message` and a position line; the patterns have to read exactly those
            let reads = self.ok.is_match("ok")
                && self.error.is_match("!! a message")
                && self
                    .position
                    .find("X:1.000 Y:2.000 Z:3.000 E:4.000")
                    .is_some();
            if !reads {
                return Err(order("ok_pattern", "through Moonraker the patterns must read `ok`, `!! message` and `X:1.000 Y:2.000 Z:3.000 E:4.000`"));
            }
        }
        match self.homing_mode {
            HomingMode::Sensorless => {
                for (value, axis, key) in [
                    (
                        self.homing_sensitivity[0],
                        &self.axis_x,
                        "homing_sensitivity_x",
                    ),
                    (
                        self.homing_sensitivity[1],
                        &self.axis_y,
                        "homing_sensitivity_y",
                    ),
                ] {
                    if value.is_none() {
                        return Err(ProfileError::new(
                            0,
                            key,
                            Fault::Missing(format!(
                                "the sensitivity of the axis {axis} for homing without switches"
                            )),
                        ));
                    }
                }
            }
            HomingMode::Switch => {
                if self.homing_sensitivity.iter().any(Option::is_some) {
                    return Err(order(
                        "homing_mode",
                        "a sensitivity is for homing_mode = sensorless",
                    ));
                }
            }
        }
        if self.home.is_empty() {
            return Err(ProfileError::new(
                0,
                "home",
                Fault::Missing("the lines that home the machine".into()),
            ));
        }
        if self.safe_stop.is_empty() {
            return Err(ProfileError::new(
                0,
                "safe_stop",
                Fault::Missing("the lines that leave the machine safe when something fails".into()),
            ));
        }
        Ok(())
    }

    /// The lines that home the machine: the sensitivities of X and Y first when the homing is without switches, then the lines of `home`.
    pub fn home_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.homing_mode == HomingMode::Sensorless {
            for (value, axis) in self
                .homing_sensitivity
                .iter()
                .zip([&self.axis_x, &self.axis_y])
            {
                if let Some(value) = value {
                    out.push(
                        self.homing_sensitivity_line
                            .replace("{axis_lower}", &axis.to_ascii_lowercase())
                            .replace("{axis}", axis)
                            .replace("{value}", &value.to_string()),
                    );
                }
            }
        }
        out.extend(self.home.iter().cloned());
        out
    }

    /// The names of the axes (x, y, z, turn) in the order of the letters of a move.
    fn letters(&self) -> [&str; 4] {
        [&self.axis_x, &self.axis_y, &self.axis_z, &self.axis_rot]
    }

    /// The line of a move: the command, the axes that are given and the feed. A place that is not a number, outside the limits or with a feed over the greatest is refused: nothing is
    /// clamped, because a move that was not asked for is worse than no move.
    pub fn move_line(&self, t: &Target, feed: f64) -> Result<String, String> {
        if !feed.is_finite() || feed <= 0.0 || feed > self.feed_max {
            return Err(format!(
                "the feed {} is not from above 0 to {} mm/min",
                num(feed),
                num(self.feed_max)
            ));
        }
        let mut out = self.move_command.clone();
        let values = [t.x, t.y, t.z, t.rot];
        let ranges = [
            (self.limit_x_min, self.limit_x),
            (self.limit_y_min, self.limit_y),
            (self.limit_z_min, self.limit_z_max),
            (f64::NEG_INFINITY, f64::INFINITY),
        ];
        for ((value, letter), (lo, hi)) in values.iter().zip(self.letters()).zip(ranges) {
            let Some(v) = value else { continue };
            if !v.is_finite() || !(lo..=hi).contains(v) {
                return Err(format!(
                    "{letter} {} is outside {} to {}",
                    num(*v),
                    num(lo),
                    num(hi)
                ));
            }
            if letter.is_empty() {
                return Err("the machine has no axis to turn the nozzle".into());
            }
            out.push_str(&format!(" {letter}{v:.3}"));
        }
        out.push_str(&format!(" F{}", num(feed)));
        Ok(out)
    }

    /// The line that switches an actuator.
    pub fn actuator_line(&self, name: &str, on: bool) -> Result<String, String> {
        self.actuators
            .iter()
            .find(|a| a.name == name)
            .map(|a| if on { a.on.clone() } else { a.off.clone() })
            .ok_or_else(|| format!("no actuator is called '{name}'"))
    }

    /// The line that waits the milliseconds given.
    pub fn dwell_line(&self, ms: u32) -> String {
        self.dwell.replace("{ms}", &ms.to_string())
    }

    /// The lines of a step: a move is followed by the line that waits for it.
    pub fn lines(&self, op: &Op) -> Result<Vec<String>, String> {
        Ok(match op {
            Op::Move(t, feed) => vec![self.move_line(t, *feed)?, self.wait.clone()],
            Op::Actuator(n, on) => vec![self.actuator_line(n, *on)?],
            Op::Dwell(ms) => vec![self.dwell_line(*ms)],
            Op::Wait => vec![self.wait.clone()],
            Op::Query => vec![self.position_request.clone()],
        })
    }

    /// The steps that pick a part at its feeder and put it on the board: up, over the feeder, down, the pump on, up, over the place with the turn, down, the pump off and the blow,
    /// up. A placement of a nozzle the profile does not have is refused.
    pub fn placement_ops(&self, p: &Placement) -> Result<Vec<Op>, String> {
        // the nozzles of a plan are counted from 0
        if p.nozzle >= self.nozzles {
            return Err(format!(
                "{} uses nozzle {} and the profile has {}",
                p.reference,
                p.nozzle + 1,
                self.nozzles
            ));
        }
        let at = |x: f64, y: f64, rot: Option<f64>| Target {
            x: Some(x),
            y: Some(y),
            z: None,
            rot,
        };
        let z = |z: f64| Target {
            z: Some(z),
            ..Target::default()
        };
        // the axis of the nozzle goes to its angle of pick before the part is taken: left where the part before put it, this part would be placed turned by that angle
        let axis = !self.axis_rot.is_empty();
        let turn_pick = axis.then_some(p.nozzle_pick_deg);
        let turn_place = axis.then_some(p.nozzle_place_deg);
        let mut ops = vec![
            Op::Move(z(self.z_travel), self.feed_z),
            Op::Move(at(p.pick.0, p.pick.1, turn_pick), self.feed_travel),
            Op::Move(z(self.z_pick), self.feed_z),
            Op::Actuator(self.pump.clone(), true),
            Op::Dwell(self.pump_settle_ms),
            Op::Move(z(self.z_travel), self.feed_z),
            Op::Move(at(p.x, p.y, turn_place), self.feed_travel),
            Op::Move(z(self.z_place), self.feed_z),
            Op::Actuator(self.pump.clone(), false),
        ];
        if !self.release.is_empty() {
            ops.push(Op::Actuator(self.release.clone(), true));
            ops.push(Op::Dwell(self.release_settle_ms));
            ops.push(Op::Actuator(self.release.clone(), false));
        }
        ops.push(Op::Move(z(self.z_travel), self.feed_z));
        // every step must be one the profile can write before any of them is sent
        for op in &ops {
            self.lines(op)?;
        }
        Ok(ops)
    }

    /// How a line from the controller reads: an error first (a halted controller says it before anything), then busy, then ok, then a position, then the lines that are only an echo.
    /// The moves of the square of `bringup`: first up to the height of travel with nothing else moving, then the four corners in X and Y alone, so that the head never travels while it is
    /// still low. The square starts at the least place of X and Y (or at 0 when that is under it) and stays within the limits.
    pub fn probe_moves(&self) -> Vec<Target> {
        let (x0, y0) = (self.limit_x_min.max(0.0), self.limit_y_min.max(0.0));
        let side = self
            .bringup_probe_mm
            .min(self.limit_x - x0)
            .min(self.limit_y - y0)
            .max(0.0);
        let lift = Target {
            z: Some(self.z_travel),
            ..Target::default()
        };
        let corners = [(side, 0.0), (side, side), (0.0, side), (0.0, 0.0)];
        std::iter::once(lift)
            .chain(corners.iter().map(|(x, y)| Target {
                x: Some(x0 + x),
                y: Some(y0 + y),
                ..Target::default()
            }))
            .collect()
    }

    pub fn classify(&self, line: &str) -> Reply {
        let l = line.trim();
        if self.error.is_match(l) {
            return Reply::Error(l.to_string());
        }
        if self.stop_on_unknown_command && says_unknown_command(l) {
            return Reply::Error(l.to_string());
        }
        if self.busy.is_match(l) {
            return Reply::Busy;
        }
        if self.ok.is_match(l) {
            return Reply::Ok;
        }
        if let Some(c) = self.position.find(l) {
            if let Some(pos) = self.position_of(&c) {
                return Reply::Position(pos);
            }
        }
        if self.ignore.is_match(l) {
            return Reply::Echo(l.to_string());
        }
        Reply::Other(l.to_string())
    }

    /// A position from the groups of the position pattern: X, Y, Z and, when it is there, the turn (the `E` that Marlin writes for the extruder is the turn of the nozzle).
    pub fn position_of(&self, c: &Captures) -> Option<Pos> {
        Some(Pos {
            x: c.number(1)?,
            y: c.number(2)?,
            z: c.number(3).unwrap_or(0.0),
            rot: c.number(4),
        })
    }

    /// The request of Moonraker that runs lines of G-code: the path and the body `{"script": "..."}`.
    pub fn script_request(&self, lines: &[String]) -> (String, String) {
        let script = lines.join("\n");
        let body = json!({ "script": script });
        (self.moonraker_script_path.clone(), body.to_string())
    }

    /// The request that asks Moonraker where the head is.
    pub fn query_request(&self) -> String {
        self.moonraker_query_path.clone()
    }

    /// How an answer of Moonraker to a script reads: a status under 300 is ok; otherwise the message of its error.
    pub fn read_script_answer(&self, status: u16, body: &str) -> Reply {
        if (200..300).contains(&status) {
            return Reply::Ok;
        }
        let message = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("HTTP {status}"));
        Reply::Error(message)
    }

    /// The position in the answer of Moonraker to a query of the toolhead (`result.status.toolhead.position`, a list of at least three numbers).
    pub fn read_position_answer(&self, body: &str) -> Result<Pos, String> {
        let v: Value =
            serde_json::from_str(body).map_err(|e| format!("the answer is not JSON: {e}"))?;
        let list = v
            .get("result")
            .and_then(|r| r.get("status"))
            .and_then(|s| s.get("toolhead"))
            .and_then(|t| t.get("position"))
            .and_then(Value::as_array)
            .ok_or("the answer has no toolhead position")?;
        let at = |i: usize| {
            list.get(i)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
        };
        Ok(Pos {
            x: at(0).ok_or("the position has no X")?,
            y: at(1).ok_or("the position has no Y")?,
            z: at(2).ok_or("the position has no Z")?,
            rot: at(3),
        })
    }
}

/// Whether a line is the notice of a controller that it does not know what it was sent: `echo:Unknown command: "..."` of Marlin and `// Unknown command:"..."` of Klipper.
fn says_unknown_command(line: &str) -> bool {
    let rest = line
        .strip_prefix("echo:")
        .or_else(|| line.strip_prefix("//"));
    rest.is_some_and(|r| {
        r.trim_start()
            .to_ascii_lowercase()
            .starts_with("unknown command")
    })
}

fn axis_name(v: &str) -> Option<String> {
    let mut cs = v.chars();
    match (cs.next(), cs.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase().to_string()),
        _ => None,
    }
}

fn path_text(v: &str) -> Option<String> {
    (v.starts_with('/') && !v.chars().any(|c| c.is_control() || c == ' ')).then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_profile_writes_a_move_and_waits_for_it() {
        let p = DriverProfile::default();
        let t = Target {
            x: Some(10.0),
            y: Some(20.5),
            ..Target::default()
        };
        let lines = p.lines(&Op::Move(t, 6000.0)).unwrap();
        assert_eq!(
            lines,
            vec!["G1 X10.000 Y20.500 F6000".to_string(), "M400".to_string()]
        );
    }

    #[test]
    fn a_move_outside_the_limits_is_refused_and_not_clamped() {
        let p = DriverProfile::default();
        let t = Target {
            x: Some(401.0),
            ..Target::default()
        };
        assert!(p.move_line(&t, 1000.0).is_err());
        assert!(p.move_line(&Target::default(), p.feed_max + 1.0).is_err());
        assert!(p
            .move_line(
                &Target {
                    x: Some(f64::NAN),
                    ..Target::default()
                },
                100.0
            )
            .is_err());
    }
}
