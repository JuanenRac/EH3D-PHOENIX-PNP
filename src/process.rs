// EH3D-PHOENIX-PNP - the process of a board: the phases it goes through and what each one asks of the person or of the machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board is not only placed: it is put on the machine, the parts of one side are placed, the board goes through the oven, the other side is placed, a few parts are soldered by hand,
//! and somebody looks at the result. A process says that in order. It is a list of phases; a phase has tasks, a state (pending, going or done) and a load-out, the feeders that were loaded
//! for it. The rules are few and are the ones a person would keep on paper: a phase starts only when every earlier one is done and none is going on; it finishes only when it is going; and
//! its load-out can be changed only while it is pending, so what was loaded for a phase that has begun cannot be rewritten afterward. A process is a file (`phase = NAME | task, task`) or
//! one of two presets (`pnp`, with the machine placing, and `manual`, with a person placing by hand). The guided manual mode walks a process phase by phase. None of it touches a machine.

use crate::error::Lang;
use crate::msg8::{pick, say};
use crate::parts::Side;

/// The most phases of a process, the most tasks of a phase, the longest name of a phase, the most feeders of a load-out and the largest process file (bytes).
pub const MAX_PHASES: usize = 16;
pub const MAX_TASKS: usize = 16;
pub const MAX_NAME: usize = 48;
pub const MAX_LOADOUT: usize = 64;
pub const MAX_PROCESS_BYTES: usize = 16_384;

/// Who places the parts of a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doer {
    Machine,
    Hand,
}

/// A task of a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// The board is put where it is worked on.
    Load,
    /// The parts of a side are placed.
    Place(Side, Doer),
    /// The board goes through the oven.
    Reflow,
    /// A few parts are soldered by hand.
    HandSolder,
    /// A part is glued, for a wave of solder.
    Glue,
    /// The parts that go through the board are put in.
    ThroughHole,
    /// Somebody looks at the board.
    Inspect,
}

impl Task {
    /// The word of a task in a process file: `load`, `place top`, `place bottom by hand`, `reflow`, `solder by hand`, `glue`, `through-hole`, `inspect`.
    pub fn word(self) -> String {
        match self {
            Task::Load => "load".to_string(),
            Task::Place(side, doer) => format!(
                "place {}{}",
                if side == Side::Top { "top" } else { "bottom" },
                if doer == Doer::Hand { " by hand" } else { "" }
            ),
            Task::Reflow => "reflow".to_string(),
            Task::HandSolder => "solder by hand".to_string(),
            Task::Glue => "glue".to_string(),
            Task::ThroughHole => "through-hole".to_string(),
            Task::Inspect => "inspect".to_string(),
        }
    }

    /// The task that a word names, without regard to case or to the spaces around the words.
    pub fn parse(text: &str) -> Option<Task> {
        let words: Vec<String> = text
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        match words.as_slice() {
            ["load"] => Some(Task::Load),
            ["reflow"] => Some(Task::Reflow),
            ["solder", "by", "hand"] => Some(Task::HandSolder),
            ["glue"] => Some(Task::Glue),
            ["through-hole"] => Some(Task::ThroughHole),
            ["inspect"] => Some(Task::Inspect),
            ["place", side] | ["place", side, "by", "machine"] => {
                Some(Task::Place(parse_side(side)?, Doer::Machine))
            }
            ["place", side, "by", "hand"] => Some(Task::Place(parse_side(side)?, Doer::Hand)),
            _ => None,
        }
    }

    /// What the person is asked to do (or told is done by the machine), in a language.
    pub fn describe(self, lang: Lang) -> String {
        let rows: [&str; 8] = match self {
            Task::Load => [
                "put the board on the machine or on the bench",
                "poner la placa en la máquina o en el banco",
                "die Platine auf die Maschine oder die Werkbank legen",
                "mettre la carte sur la machine ou sur l'établi",
                "mettere la scheda sulla macchina o sul banco",
                "基板を機械または作業台に載せる",
                "把电路板放到机器或工作台上",
                "pôr a placa na máquina ou na bancada",
            ],
            Task::Place(side, Doer::Hand) => {
                return say(
                    lang,
                    &[
                        "place the parts of the {side} side by hand",
                        "colocar a mano las piezas de la cara {side}",
                        "die Teile der Seite {side} von Hand bestücken",
                        "poser à la main les pièces de la face {side}",
                        "posizionare a mano i componenti del lato {side}",
                        "{side}面の部品を手で実装する",
                        "手工贴装{side}面的元件",
                        "colocar à mão as peças da face {side}",
                    ],
                    &[("side", side_word(side, lang).to_string())],
                )
            }
            Task::Place(side, Doer::Machine) => {
                return say(
                    lang,
                    &[
                        "the machine places the parts of the {side} side",
                        "la máquina coloca las piezas de la cara {side}",
                        "die Maschine bestückt die Teile der Seite {side}",
                        "la machine pose les pièces de la face {side}",
                        "la macchina posiziona i componenti del lato {side}",
                        "機械が{side}面の部品を実装する",
                        "机器贴装{side}面的元件",
                        "a máquina coloca as peças da face {side}",
                    ],
                    &[("side", side_word(side, lang).to_string())],
                )
            }
            Task::Reflow => [
                "pass the board through the oven",
                "pasar la placa por el horno",
                "die Platine durch den Ofen schicken",
                "passer la carte au four",
                "far passare la scheda nel forno",
                "基板をリフロー炉に通す",
                "让电路板过回流炉",
                "passar a placa pelo forno",
            ],
            Task::HandSolder => [
                "solder the parts that were left for the iron",
                "soldar las piezas que quedaron para el soldador",
                "die für den Lötkolben vorgesehenen Teile löten",
                "souder les pièces laissées pour le fer",
                "saldare i componenti lasciati per il saldatore",
                "はんだごてで付けるために残した部品をはんだ付けする",
                "焊接留给烙铁的元件",
                "soldar as peças que ficaram para o ferro",
            ],
            Task::Glue => [
                "glue the parts that go under the wave",
                "pegar las piezas que pasan por la ola",
                "die Teile für die Welle ankleben",
                "coller les pièces qui passent à la vague",
                "incollare i componenti che passano nell'onda",
                "ウェーブはんだ用の部品を接着する",
                "粘接过波峰焊的元件",
                "colar as peças que passam pela onda",
            ],
            Task::ThroughHole => [
                "put in the parts that go through the board",
                "poner las piezas que atraviesan la placa",
                "die durchsteckbaren Teile einsetzen",
                "mettre en place les pièces traversantes",
                "inserire i componenti passanti",
                "スルーホール部品を挿入する",
                "插入穿孔元件",
                "pôr as peças que atravessam a placa",
            ],
            Task::Inspect => [
                "look at the board",
                "mirar la placa",
                "die Platine ansehen",
                "regarder la carte",
                "guardare la scheda",
                "基板を目視で確認する",
                "检查电路板",
                "olhar para a placa",
            ],
        };
        pick(lang, &rows).to_string()
    }
}

fn parse_side(word: &str) -> Option<Side> {
    match word {
        "top" => Some(Side::Top),
        "bottom" => Some(Side::Bottom),
        _ => None,
    }
}

fn side_word(side: Side, lang: Lang) -> &'static str {
    let top: [&str; 8] = [
        "top",
        "superior",
        "obere",
        "supérieure",
        "superiore",
        "上",
        "顶",
        "superior",
    ];
    let bottom: [&str; 8] = [
        "bottom",
        "inferior",
        "untere",
        "inférieure",
        "inferiore",
        "下",
        "底",
        "inferior",
    ];
    pick(lang, if side == Side::Top { &top } else { &bottom })
}

/// Where a phase is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    Pending,
    Going,
    Done,
}

impl PhaseState {
    pub fn word(self) -> &'static str {
        match self {
            PhaseState::Pending => "pending",
            PhaseState::Going => "going",
            PhaseState::Done => "done",
        }
    }

    fn from_word(text: &str) -> Option<PhaseState> {
        match text {
            "pending" => Some(PhaseState::Pending),
            "going" => Some(PhaseState::Going),
            "done" => Some(PhaseState::Done),
            _ => None,
        }
    }
}

/// A phase of a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub name: String,
    pub tasks: Vec<Task>,
    pub state: PhaseState,
    /// The feeders loaded for this phase; they can be changed only while the phase is pending.
    pub loadout: Vec<String>,
}

/// Why a process was refused, or why a step of it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError {
    /// A line that is not `phase = NAME | task, task`.
    Syntax(usize),
    /// A word that is no task, and its line.
    BadTask(usize, String),
    /// A limit of size (phases, tasks, name, load-out, file), with what it is.
    Limit(&'static str),
    /// A name used twice, and its line.
    Duplicate(usize),
    /// No phase.
    Empty,
    /// A phase that cannot start now: an earlier one is not done, or one is going on.
    NotYet(String),
    /// A phase that is not going cannot finish.
    NotGoing(String),
    /// The load-out of a phase that has begun cannot be changed.
    Locked(String),
    /// A phase that is not in the process.
    NoPhase(usize),
}

impl ProcessError {
    /// The refusal in a language.
    pub fn localized(&self, lang: Lang) -> String {
        let (rows, a, b): ([&str; 8], String, String) = match self {
            ProcessError::Syntax(line) => (
                [
                    "line {a}: not a line of the form phase = NAME | task, task",
                    "línea {a}: no es una línea de la forma phase = NOMBRE | tarea, tarea",
                    "Zeile {a}: keine Zeile der Form phase = NAME | Aufgabe, Aufgabe",
                    "ligne {a} : pas une ligne de la forme phase = NOM | tâche, tâche",
                    "riga {a}: non è una riga della forma phase = NOME | compito, compito",
                    "{a} 行目：「phase = 名前 | 作業, 作業」の形ではありません",
                    "第 {a} 行：不是“phase = 名称 | 任务, 任务”的形式",
                    "linha {a}: não é uma linha da forma phase = NOME | tarefa, tarefa",
                ],
                line.to_string(),
                String::new(),
            ),
            ProcessError::BadTask(line, word) => (
                [
                    "line {a}: '{b}' is not a task (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                    "línea {a}: '{b}' no es una tarea (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                    "Zeile {a}: '{b}' ist keine Aufgabe (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                    "ligne {a} : '{b}' n'est pas une tâche (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                    "riga {a}: '{b}' non è un compito (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                    "{a} 行目：「{b}」は作業ではありません（load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect）",
                    "第 {a} 行：“{b}”不是任务（load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect）",
                    "linha {a}: '{b}' não é uma tarefa (load, place top, place bottom by hand, reflow, solder by hand, glue, through-hole, inspect)",
                ],
                line.to_string(),
                word.clone(),
            ),
            ProcessError::Limit(what) => (
                [
                    "a process has too many {a}, or a {a} that is too long",
                    "un proceso tiene demasiados {a}, o un {a} demasiado largo",
                    "ein Ablauf hat zu viele {a} oder ein zu langes {a}",
                    "un processus a trop de {a}, ou un {a} trop long",
                    "un processo ha troppi {a}, o un {a} troppo lungo",
                    "プロセスの {a} が多すぎるか、長すぎます",
                    "流程的 {a} 太多，或有一个 {a} 太长",
                    "um processo tem demasiados {a}, ou um {a} demasiado longo",
                ],
                (*what).to_string(),
                String::new(),
            ),
            ProcessError::Duplicate(line) => (
                [
                    "line {a}: the name of a phase is used twice",
                    "línea {a}: el nombre de una fase está repetido",
                    "Zeile {a}: der Name einer Phase kommt doppelt vor",
                    "ligne {a} : le nom d'une phase est utilisé deux fois",
                    "riga {a}: il nome di una fase è usato due volte",
                    "{a} 行目：フェーズ名が重複しています",
                    "第 {a} 行：阶段名称重复",
                    "linha {a}: o nome de uma fase está repetido",
                ],
                line.to_string(),
                String::new(),
            ),
            ProcessError::Empty => (
                [
                    "the process has no phase",
                    "el proceso no tiene ninguna fase",
                    "der Ablauf hat keine Phase",
                    "le processus n'a aucune phase",
                    "il processo non ha alcuna fase",
                    "プロセスにフェーズがありません",
                    "流程没有任何阶段",
                    "o processo não tem nenhuma fase",
                ],
                String::new(),
                String::new(),
            ),
            ProcessError::NotYet(name) => (
                [
                    "'{a}' cannot start: an earlier phase is not done, or one is going on",
                    "'{a}' no puede empezar: una fase anterior no está hecha, o hay una en curso",
                    "'{a}' kann nicht beginnen: eine frühere Phase ist nicht fertig oder eine läuft",
                    "'{a}' ne peut pas commencer : une phase précédente n'est pas finie, ou une est en cours",
                    "'{a}' non può iniziare: una fase precedente non è finita, o una è in corso",
                    "「{a}」は開始できません：前のフェーズが完了していないか、実行中のフェーズがあります",
                    "“{a}”不能开始：前面的阶段未完成，或有阶段正在进行",
                    "'{a}' não pode começar: uma fase anterior não está feita, ou há uma em curso",
                ],
                name.clone(),
                String::new(),
            ),
            ProcessError::NotGoing(name) => (
                [
                    "'{a}' is not going, so it cannot finish",
                    "'{a}' no está en curso, así que no puede terminar",
                    "'{a}' läuft nicht, also kann es nicht enden",
                    "'{a}' n'est pas en cours, elle ne peut donc pas finir",
                    "'{a}' non è in corso, quindi non può finire",
                    "「{a}」は実行中ではないので完了できません",
                    "“{a}”没有进行，因此不能结束",
                    "'{a}' não está em curso, pelo que não pode terminar",
                ],
                name.clone(),
                String::new(),
            ),
            ProcessError::Locked(name) => (
                [
                    "the load-out of '{a}' cannot be changed: the phase has begun",
                    "la carga de '{a}' no se puede cambiar: la fase ya empezó",
                    "die Bestückung von '{a}' lässt sich nicht ändern: die Phase hat begonnen",
                    "le chargement de '{a}' ne peut plus changer : la phase a commencé",
                    "il carico di '{a}' non si può cambiare: la fase è iniziata",
                    "「{a}」の搭載は変更できません：フェーズは開始済みです",
                    "“{a}”的装料不能更改：该阶段已开始",
                    "a carga de '{a}' não pode mudar: a fase já começou",
                ],
                name.clone(),
                String::new(),
            ),
            ProcessError::NoPhase(i) => (
                [
                    "there is no phase {a}",
                    "no hay fase {a}",
                    "es gibt keine Phase {a}",
                    "il n'y a pas de phase {a}",
                    "non c'è la fase {a}",
                    "フェーズ {a} はありません",
                    "没有阶段 {a}",
                    "não há fase {a}",
                ],
                (i + 1).to_string(),
                String::new(),
            ),
        };
        say(lang, &rows, &[("a", a), ("b", b)])
    }
}

/// A process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub phases: Vec<Phase>,
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(|c| c.is_control() || c == '|' || c == ',')
}

impl Process {
    /// Reads a process file: one `phase = NAME | task, task, ...` line for each phase, in order; empty lines and `#` lines are skipped. A phase has at least one task.
    pub fn parse(text: &str) -> Result<Process, ProcessError> {
        if text.len() > MAX_PROCESS_BYTES {
            return Err(ProcessError::Limit("bytes"));
        }
        let mut phases: Vec<Phase> = Vec::new();
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line_no = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ProcessError::Syntax(line_no));
            };
            if !key.trim().eq_ignore_ascii_case("phase") {
                return Err(ProcessError::Syntax(line_no));
            }
            let Some((name, tasks)) = value.split_once('|') else {
                return Err(ProcessError::Syntax(line_no));
            };
            let name = name.trim();
            if !name_ok(name) {
                return Err(ProcessError::Syntax(line_no));
            }
            if phases.len() >= MAX_PHASES {
                return Err(ProcessError::Limit("phases"));
            }
            if phases.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
                return Err(ProcessError::Duplicate(line_no));
            }
            let mut list: Vec<Task> = Vec::new();
            for word in tasks.split(',') {
                let word = word.trim();
                if word.is_empty() {
                    return Err(ProcessError::Syntax(line_no));
                }
                if list.len() >= MAX_TASKS {
                    return Err(ProcessError::Limit("tasks"));
                }
                list.push(
                    Task::parse(word)
                        .ok_or_else(|| ProcessError::BadTask(line_no, word.to_string()))?,
                );
            }
            phases.push(Phase {
                name: name.to_string(),
                tasks: list,
                state: PhaseState::Pending,
                loadout: Vec::new(),
            });
        }
        if phases.is_empty() {
            return Err(ProcessError::Empty);
        }
        Ok(Process { phases })
    }

    /// The process as a file reads it.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for p in &self.phases {
            let tasks: Vec<String> = p.tasks.iter().map(|t| t.word()).collect();
            out.push_str(&format!("phase = {} | {}\n", p.name, tasks.join(", ")));
        }
        out
    }

    /// One of the two presets: `pnp` (the machine places each side) or `manual` (a person places each side by hand). Each has the top side, then the bottom side.
    pub fn preset(name: &str) -> Option<Process> {
        let doer = match name.trim().to_ascii_lowercase().as_str() {
            "pnp" => Doer::Machine,
            "manual" => Doer::Hand,
            _ => return None,
        };
        let side = |label: &str, side: Side| Phase {
            name: label.to_string(),
            tasks: vec![
                Task::Load,
                Task::Place(side, doer),
                Task::Reflow,
                Task::Inspect,
            ],
            state: PhaseState::Pending,
            loadout: Vec::new(),
        };
        Some(Process {
            phases: vec![
                side("Top side", Side::Top),
                side("Bottom side", Side::Bottom),
            ],
        })
    }

    /// Takes out the phases that only place a side the board has no parts on.
    pub fn only_sides(&mut self, has_top: bool, has_bottom: bool) {
        self.phases.retain(|p| {
            let places: Vec<Side> = p
                .tasks
                .iter()
                .filter_map(|t| match t {
                    Task::Place(side, _) => Some(*side),
                    _ => None,
                })
                .collect();
            places.is_empty()
                || places.iter().any(|s| match s {
                    Side::Top => has_top,
                    Side::Bottom => has_bottom,
                })
        });
    }

    fn phase_mut(&mut self, i: usize) -> Result<&mut Phase, ProcessError> {
        self.phases.get_mut(i).ok_or(ProcessError::NoPhase(i))
    }

    /// Starts a phase: it must be pending, every earlier phase must be done and none may be going.
    pub fn start(&mut self, i: usize) -> Result<(), ProcessError> {
        let ready = self
            .phases
            .iter()
            .enumerate()
            .all(|(k, p)| match k.cmp(&i) {
                std::cmp::Ordering::Less => p.state == PhaseState::Done,
                std::cmp::Ordering::Equal => p.state == PhaseState::Pending,
                std::cmp::Ordering::Greater => p.state == PhaseState::Pending,
            });
        let phase = self.phase_mut(i)?;
        if !ready {
            return Err(ProcessError::NotYet(phase.name.clone()));
        }
        phase.state = PhaseState::Going;
        Ok(())
    }

    /// Finishes a phase that is going.
    pub fn finish(&mut self, i: usize) -> Result<(), ProcessError> {
        let phase = self.phase_mut(i)?;
        if phase.state != PhaseState::Going {
            return Err(ProcessError::NotGoing(phase.name.clone()));
        }
        phase.state = PhaseState::Done;
        Ok(())
    }

    /// Sets the load-out of a phase (the ids of the feeders, once each); it can be done only while the phase is pending.
    pub fn set_loadout(&mut self, i: usize, feeders: &[String]) -> Result<(), ProcessError> {
        let phase = self.phase_mut(i)?;
        if phase.state != PhaseState::Pending {
            return Err(ProcessError::Locked(phase.name.clone()));
        }
        if feeders.len() > MAX_LOADOUT {
            return Err(ProcessError::Limit("feeders"));
        }
        let mut kept: Vec<String> = Vec::new();
        for f in feeders {
            let f = f.trim();
            if f.is_empty() || f.chars().any(|c| c.is_control() || c == ',' || c == '|') {
                return Err(ProcessError::Limit("feeders"));
            }
            if !kept.iter().any(|k| k == f) {
                kept.push(f.to_string());
            }
        }
        phase.loadout = kept;
        Ok(())
    }

    /// The phase that is going, or else the first one that is pending; `None` when every phase is done.
    pub fn current(&self) -> Option<usize> {
        self.phases
            .iter()
            .position(|p| p.state == PhaseState::Going)
            .or_else(|| {
                self.phases
                    .iter()
                    .position(|p| p.state == PhaseState::Pending)
            })
    }

    /// The phases done and the phases there are.
    pub fn progress(&self) -> (usize, usize) {
        (
            self.phases
                .iter()
                .filter(|p| p.state == PhaseState::Done)
                .count(),
            self.phases.len(),
        )
    }

    /// Whether every phase is done.
    pub fn finished(&self) -> bool {
        self.phases.iter().all(|p| p.state == PhaseState::Done)
    }

    /// The state to keep between two sessions: a line for each phase, `NAME | state | feeder, feeder`.
    pub fn state_text(&self) -> String {
        let mut out = String::from("# state of the process\n");
        for p in &self.phases {
            out.push_str(&format!(
                "{} | {} | {}\n",
                p.name,
                p.state.word(),
                p.loadout.join(", ")
            ));
        }
        out
    }

    /// Takes a state back in. The saved phases must be this process's phases, in the same order; anything else changes nothing. A state is valid when no phase is done or going after one
    /// that is pending and at most one is going.
    pub fn resume(&mut self, saved: &str) -> Result<(), String> {
        let mut rows: Vec<(PhaseState, Vec<String>)> = Vec::new();
        for (i, raw) in saved.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '|').map(str::trim).collect();
            let [name, state, loadout] = parts.as_slice() else {
                return Err(format!("line {}: not NAME | state | feeders", i + 1));
            };
            let at = rows.len();
            let Some(phase) = self.phases.get(at) else {
                return Err(format!(
                    "line {}: there are more phases than in the process",
                    i + 1
                ));
            };
            if !phase.name.eq_ignore_ascii_case(name) {
                return Err(format!(
                    "line {}: the phase '{name}' is not the phase '{}' of the process",
                    i + 1,
                    phase.name
                ));
            }
            let Some(state) = PhaseState::from_word(state) else {
                return Err(format!(
                    "line {}: '{state}' is not pending, going or done",
                    i + 1
                ));
            };
            let feeders: Vec<String> = loadout
                .split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect();
            if feeders.len() > MAX_LOADOUT {
                return Err(format!("line {}: too many feeders", i + 1));
            }
            rows.push((state, feeders));
        }
        if rows.len() != self.phases.len() {
            return Err("the saved state has another number of phases".to_string());
        }
        let mut seen_pending = false;
        let mut going = 0;
        for (state, _) in &rows {
            match state {
                PhaseState::Pending => seen_pending = true,
                PhaseState::Going => {
                    going += 1;
                    if seen_pending {
                        return Err("a phase is going after one that is pending".to_string());
                    }
                }
                PhaseState::Done => {
                    if seen_pending || going > 0 {
                        return Err(
                            "a phase is done after one that is pending or going".to_string()
                        );
                    }
                }
            }
        }
        if going > 1 {
            return Err("more than one phase is going".to_string());
        }
        for (phase, (state, feeders)) in self.phases.iter_mut().zip(rows) {
            phase.state = state;
            phase.loadout = feeders;
        }
        Ok(())
    }

    /// The process as a list for a person: a line for each phase with its state and its tasks, in a language.
    pub fn summary(&self, lang: Lang) -> String {
        let states = |s: PhaseState| -> &'static str {
            let rows: [&str; 8] = match s {
                PhaseState::Pending => [
                    "pending",
                    "pendiente",
                    "offen",
                    "à faire",
                    "da fare",
                    "未着手",
                    "待办",
                    "pendente",
                ],
                PhaseState::Going => [
                    "going",
                    "en curso",
                    "läuft",
                    "en cours",
                    "in corso",
                    "実行中",
                    "进行中",
                    "em curso",
                ],
                PhaseState::Done => [
                    "done",
                    "hecha",
                    "fertig",
                    "faite",
                    "fatta",
                    "完了",
                    "已完成",
                    "feita",
                ],
            };
            pick(lang, &rows)
        };
        let mut out = String::new();
        for (i, p) in self.phases.iter().enumerate() {
            out.push_str(&format!("{}. {} [{}]\n", i + 1, p.name, states(p.state)));
            for t in &p.tasks {
                out.push_str(&format!("     - {}\n", t.describe(lang)));
            }
        }
        out
    }
}

/// The line that opens a phase of the walk: `Phase 1 of 2: Top side`.
pub fn phase_header(lang: Lang, index: usize, total: usize, name: &str) -> String {
    let rows: [&str; 8] = [
        "Phase {i} of {n}: {name}",
        "Fase {i} de {n}: {name}",
        "Phase {i} von {n}: {name}",
        "Phase {i} sur {n} : {name}",
        "Fase {i} di {n}: {name}",
        "フェーズ {i}／{n}：{name}",
        "第 {i} 阶段，共 {n}：{name}",
        "Fase {i} de {n}: {name}",
    ];
    say(
        lang,
        &rows,
        &[
            ("i", index.to_string()),
            ("n", total.to_string()),
            ("name", name.to_string()),
        ],
    )
}

/// What is asked of the person after a task that is not the placing of parts: to say `done`.
pub fn confirm_prompt(lang: Lang) -> String {
    let rows: [&str; 8] = [
        "type done when it is done (or quit)",
        "escriba done cuando esté hecho (o quit)",
        "tippen Sie done, wenn es erledigt ist (oder quit)",
        "tapez done quand c'est fait (ou quit)",
        "digiti done quando è fatto (o quit)",
        "終わったら done と入力してください（中止は quit）",
        "完成后输入 done（或 quit）",
        "escreva done quando estiver feito (ou quit)",
    ];
    pick(lang, &rows).to_string()
}
