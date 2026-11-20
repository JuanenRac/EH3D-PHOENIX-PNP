// EH3D-PHOENIX-PNP - the guided manual mode: the parts of a board one after the other, for a person who places them by hand.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Not everyone has a machine. This takes the table of parts (and, if there is one, the table of feeders to say which reel a part is taken from) and walks a person through the board: the
//! parts of one reel together, one side after the other, each in the natural order of its reference, each with where it goes, how it is turned and whether it has a direction. The person
//! types the reference of the part just placed, and the walk goes on only if it is the right one; `skip` leaves a part for later, `back` takes the last step back, `quit` stops. The state can be
//! saved as a checklist and read again to go on. No hardware is involved: it is a list with a check.

use crate::designators::natural_cmp;
use crate::error::Lang;
use crate::feeders::Feeder;
use crate::msg8::{num, say};
use crate::parts::{csv_field, Part, Side};
use crate::polarity::is_polarized;

/// The order of the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// The parts of one reel together (value and package), each side in turn.
    Reel,
    /// The references in their natural order, each side in turn.
    Reference,
}

impl Order {
    pub fn parse(text: &str) -> Option<Order> {
        match text.trim().to_ascii_lowercase().as_str() {
            "reel" => Some(Order::Reel),
            "reference" => Some(Order::Reference),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Placed,
    Skipped,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Placed => "placed",
            Status::Skipped => "skipped",
        }
    }
}

/// One part of the walk.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub part: Part,
    pub feeder: Option<String>,
    pub pick: Option<(f64, f64)>,
    pub polar: bool,
}

/// What an answer of the person did.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Placed(String),
    Wrong { expected: String, got: String },
    Skipped(String),
    Undone(String),
    NothingToUndo,
    Done,
    Quit,
}

/// The walk.
#[derive(Debug, Clone)]
pub struct Assist {
    steps: Vec<Step>,
    status: Vec<Status>,
    /// The steps taken, last one at the end (for `back`).
    taken: Vec<usize>,
    quit: bool,
}

impl Assist {
    /// Prepares the walk. A reference that is in the table twice is refused: a person could not tell the two apart.
    pub fn new(
        parts: &[Part],
        feeders: &[Feeder],
        order: Order,
        polar_refs: &[String],
        polar_packages: &[String],
    ) -> Result<Assist, String> {
        let mut seen = std::collections::BTreeSet::new();
        for p in parts {
            if !seen.insert(p.reference.to_ascii_lowercase()) {
                return Err(format!(
                    "the reference {} is in the table twice",
                    p.reference
                ));
            }
        }
        let mut list: Vec<&Part> = parts.iter().collect();
        let side = |p: &Part| u8::from(p.side == Side::Bottom);
        list.sort_by(|a, b| {
            let by_side = side(a).cmp(&side(b));
            let kind = |p: &Part| (p.package.to_ascii_lowercase(), p.value.to_ascii_lowercase());
            let first = match order {
                Order::Reel => kind(a).cmp(&kind(b)),
                Order::Reference => std::cmp::Ordering::Equal,
            };
            by_side
                .then(first)
                .then_with(|| natural_cmp(&a.reference, &b.reference))
        });
        let steps: Vec<Step> = list
            .into_iter()
            .map(|p| {
                let f = feeders.iter().find(|f| f.key() == p.key() && f.count > 0);
                Step {
                    part: p.clone(),
                    feeder: f.map(|f| f.id.clone()),
                    pick: f.map(|f| (f.x, f.y)),
                    polar: is_polarized(p, polar_refs, polar_packages),
                }
            })
            .collect();
        let n = steps.len();
        Ok(Assist {
            steps,
            status: vec![Status::Pending; n],
            taken: Vec::new(),
            quit: false,
        })
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn status(&self) -> &[Status] {
        &self.status
    }

    /// The index of the part the person is to place now: the first one still pending.
    pub fn cursor(&self) -> Option<usize> {
        if self.quit {
            return None;
        }
        self.status.iter().position(|s| *s == Status::Pending)
    }

    /// The step to place now.
    pub fn current(&self) -> Option<&Step> {
        self.cursor().map(|i| &self.steps[i])
    }

    /// How many are placed, skipped and in all.
    pub fn progress(&self) -> (usize, usize, usize) {
        let c = |s: Status| self.status.iter().filter(|x| **x == s).count();
        (c(Status::Placed), c(Status::Skipped), self.steps.len())
    }

    /// Takes one line the person typed. The reference of the current part places it; the reference of another part is told as wrong (the walk does not move); `skip`, `back` and `quit` do
    /// what they say. When every part is placed or skipped, the answer is `Done`; skipped parts come back after the last pending one.
    pub fn respond(&mut self, input: &str) -> Answer {
        let word = input.trim();
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "quit" | "q" => {
                self.quit = true;
                return Answer::Quit;
            }
            "back" | "undo" | "b" => {
                return match self.taken.pop() {
                    Some(i) => {
                        self.status[i] = Status::Pending;
                        Answer::Undone(self.steps[i].part.reference.clone())
                    }
                    None => Answer::NothingToUndo,
                };
            }
            _ => {}
        }
        let Some(i) = self
            .cursor()
            .or_else(|| self.status.iter().position(|s| *s == Status::Skipped))
        else {
            return Answer::Done;
        };
        let reference = self.steps[i].part.reference.clone();
        if lower == "skip" || lower == "s" {
            if self.status[i] == Status::Pending {
                self.status[i] = Status::Skipped;
                self.taken.push(i);
                return Answer::Skipped(reference);
            }
            return Answer::Done;
        }
        if reference.eq_ignore_ascii_case(word) {
            self.status[i] = Status::Placed;
            self.taken.push(i);
            Answer::Placed(reference)
        } else {
            Answer::Wrong {
                expected: reference,
                got: word.to_string(),
            }
        }
    }

    /// Whether nothing is pending (the skipped parts are still to do, but the walk has gone through the board once).
    pub fn finished(&self) -> bool {
        self.status.iter().all(|s| *s != Status::Pending)
    }

    /// The parts that were skipped and are still to place.
    pub fn skipped(&self) -> Vec<&Step> {
        self.steps
            .iter()
            .zip(&self.status)
            .filter(|(_, s)| **s == Status::Skipped)
            .map(|(p, _)| p)
            .collect()
    }

    /// Puts the skipped parts back to pending so that the walk comes to them again.
    pub fn revisit_skipped(&mut self) {
        for s in &mut self.status {
            if *s == Status::Skipped {
                *s = Status::Pending;
            }
        }
    }

    /// The checklist: `reference,status` for every part, in the order of the walk. It is also the file that `resume` reads.
    pub fn checklist_csv(&self) -> String {
        let mut out = String::from("reference,status\n");
        for (s, st) in self.steps.iter().zip(&self.status) {
            out.push_str(&format!("{},{}\n", csv_field(&s.part.reference), st.name()));
        }
        out
    }

    /// Reads a checklist written before and goes on from it: the parts it marks placed stay placed. A reference that is not in the table, or a status that is not one of the three, is an
    /// error with its line, and nothing is changed.
    pub fn resume(&mut self, saved: &str) -> Result<usize, String> {
        let mut marks: Vec<(usize, Status)> = Vec::new();
        for (n, raw) in saved.lines().enumerate() {
            let line = raw.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || (n == 0 && line.starts_with("reference")) {
                continue;
            }
            let (r, st) = line
                .split_once(',')
                .ok_or_else(|| format!("line {}: not 'reference,status'", n + 1))?;
            let status = match st.trim().to_ascii_lowercase().as_str() {
                "placed" => Status::Placed,
                "skipped" => Status::Skipped,
                "pending" => Status::Pending,
                other => {
                    return Err(format!(
                        "line {}: '{other}' is not placed, skipped or pending",
                        n + 1
                    ))
                }
            };
            let i = self
                .steps
                .iter()
                .position(|s| {
                    s.part
                        .reference
                        .eq_ignore_ascii_case(r.trim_matches('"').trim())
                })
                .ok_or_else(|| {
                    format!("line {}: {} is not in the table of parts", n + 1, r.trim())
                })?;
            marks.push((i, status));
        }
        let mut changed = 0;
        for (i, s) in marks {
            if self.status[i] != s {
                changed += 1;
            }
            self.status[i] = s;
        }
        Ok(changed)
    }
}

/// What the person is told for a step.
pub fn prompt(a: &Assist, lang: Lang) -> String {
    let (placed, _, total) = a.progress();
    let Some(step) = a.current() else {
        return String::new();
    };
    let p = &step.part;
    let row: [&str; 8] = [
        "[{done}/{total}] {reference}: {value} ({package}), {from}X {x} Y {y}, turned {rot} degrees, {side}{polar}. Type the reference when it is placed (skip, back, quit).",
        "[{done}/{total}] {reference}: {value} ({package}), {from}X {x} Y {y}, girado {rot} grados, {side}{polar}. Escriba la referencia al colocarla (skip, back, quit).",
        "[{done}/{total}] {reference}: {value} ({package}), {from}X {x} Y {y}, um {rot} Grad gedreht, {side}{polar}. Geben Sie die Referenz ein, wenn es gesetzt ist (skip, back, quit).",
        "[{done}/{total}] {reference} : {value} ({package}), {from}X {x} Y {y}, tourné de {rot} degrés, {side}{polar}. Saisissez la référence une fois posée (skip, back, quit).",
        "[{done}/{total}] {reference}: {value} ({package}), {from}X {x} Y {y}, ruotato di {rot} gradi, {side}{polar}. Digiti il riferimento quando è posizionato (skip, back, quit).",
        "[{done}/{total}] {reference}：{value}（{package}）、{from}X {x} Y {y}、{rot} 度回転、{side}{polar}。置いたら部品番号を入力してください（skip, back, quit）。",
        "[{done}/{total}] {reference}：{value}（{package}），{from}X {x} Y {y}，旋转 {rot} 度，{side}{polar}。放好后输入位号（skip, back, quit）。",
        "[{done}/{total}] {reference}: {value} ({package}), {from}X {x} Y {y}, rodado {rot} graus, {side}{polar}. Escreva a referência quando estiver colocada (skip, back, quit).",
    ];
    let from_row: [&str; 8] = [
        "from {feeder} at X {fx} Y {fy}, to ",
        "de {feeder} en X {fx} Y {fy}, a ",
        "von {feeder} bei X {fx} Y {fy}, nach ",
        "de {feeder} à X {fx} Y {fy}, vers ",
        "da {feeder} a X {fx} Y {fy}, verso ",
        "{feeder}（X {fx} Y {fy}）から ",
        "从 {feeder}（X {fx} Y {fy}）取料，放到 ",
        "de {feeder} em X {fx} Y {fy}, para ",
    ];
    let from = match (&step.feeder, step.pick) {
        (Some(f), Some((fx, fy))) => say(
            lang,
            &from_row,
            &[("feeder", f.clone()), ("fx", num(fx)), ("fy", num(fy))],
        ),
        _ => String::new(),
    };
    let side_rows: [[&str; 8]; 2] = [
        [
            "top side",
            "cara superior",
            "Oberseite",
            "face dessus",
            "lato superiore",
            "上面",
            "顶面",
            "face superior",
        ],
        [
            "bottom side",
            "cara inferior",
            "Unterseite",
            "face dessous",
            "lato inferiore",
            "下面",
            "底面",
            "face inferior",
        ],
    ];
    let side = crate::msg8::pick(lang, &side_rows[usize::from(p.side == Side::Bottom)]);
    let polar_row: [&str; 8] = [
        " - mind its direction",
        " - cuidado con su sentido",
        " - auf die Richtung achten",
        " - attention au sens",
        " - attenzione al verso",
        "（向きに注意）",
        "（注意方向）",
        " - atenção ao sentido",
    ];
    let polar = if step.polar {
        crate::msg8::pick(lang, &polar_row)
    } else {
        ""
    };
    say(
        lang,
        &row,
        &[
            ("done", placed.to_string()),
            ("total", total.to_string()),
            ("reference", p.reference.clone()),
            ("value", p.value.clone()),
            ("package", p.package.clone()),
            ("from", from),
            ("x", num(p.x)),
            ("y", num(p.y)),
            ("rot", num(p.rotation)),
            ("side", side.to_string()),
            ("polar", polar.to_string()),
        ],
    )
}

/// The text of an answer.
pub fn feedback(ans: &Answer, a: &Assist, lang: Lang) -> String {
    match ans {
        Answer::Placed(r) => {
            let row: [&str; 8] = [
                "{r} placed",
                "{r} colocada",
                "{r} gesetzt",
                "{r} posée",
                "{r} posizionato",
                "{r} を配置しました",
                "{r} 已放置",
                "{r} colocada",
            ];
            say(lang, &row, &[("r", r.clone())])
        }
        Answer::Wrong { expected, got } => {
            let row: [&str; 8] = [
                "'{got}' is not the part to place now; the part is {expected}",
                "'{got}' no es la pieza que toca ahora; la pieza es {expected}",
                "'{got}' ist nicht das Teil, das jetzt dran ist; es ist {expected}",
                "'{got}' n'est pas la pièce à poser maintenant ; c'est {expected}",
                "'{got}' non è il componente da posizionare ora; è {expected}",
                "「{got}」は今置く部品ではありません。置く部品は {expected} です",
                "“{got}”不是现在要放的元件；应放的是 {expected}",
                "'{got}' não é a peça a colocar agora; a peça é {expected}",
            ];
            say(
                lang,
                &row,
                &[("got", got.clone()), ("expected", expected.clone())],
            )
        }
        Answer::Skipped(r) => {
            let row: [&str; 8] = [
                "{r} left for later",
                "{r} queda para después",
                "{r} bleibt für später",
                "{r} remise à plus tard",
                "{r} rimandato a dopo",
                "{r} は後回しにします",
                "{r} 留待稍后",
                "{r} fica para depois",
            ];
            say(lang, &row, &[("r", r.clone())])
        }
        Answer::Undone(r) => {
            let row: [&str; 8] = [
                "{r} is to be placed again",
                "{r} vuelve a estar por colocar",
                "{r} ist wieder zu setzen",
                "{r} est de nouveau à poser",
                "{r} è di nuovo da posizionare",
                "{r} は再び未配置になりました",
                "{r} 重新变为待放置",
                "{r} volta a estar por colocar",
            ];
            say(lang, &row, &[("r", r.clone())])
        }
        Answer::NothingToUndo => {
            let row: [&str; 8] = [
                "there is nothing to take back",
                "no hay nada que deshacer",
                "es gibt nichts zurückzunehmen",
                "il n'y a rien à annuler",
                "non c'è nulla da annullare",
                "取り消すものがありません",
                "没有可撤销的内容",
                "não há nada a desfazer",
            ];
            crate::msg8::pick(lang, &row).to_string()
        }
        Answer::Done => {
            let (placed, skipped, total) = a.progress();
            let row: [&str; 8] = [
                "every part has been gone through: {p} placed, {s} left for later, {t} in all",
                "se ha recorrido todas las piezas: {p} colocadas, {s} para después, {t} en total",
                "alle Teile sind durchgegangen: {p} gesetzt, {s} für später, {t} insgesamt",
                "toutes les pièces ont été passées : {p} posées, {s} remises à plus tard, {t} au total",
                "tutti i componenti sono stati percorsi: {p} posizionati, {s} rimandati, {t} in tutto",
                "すべての部品を確認しました：配置 {p}、後回し {s}、合計 {t}",
                "所有元件已走完：已放置 {p}，留待稍后 {s}，共 {t}",
                "todas as peças foram percorridas: {p} colocadas, {s} para depois, {t} no total",
            ];
            say(
                lang,
                &row,
                &[
                    ("p", placed.to_string()),
                    ("s", skipped.to_string()),
                    ("t", total.to_string()),
                ],
            )
        }
        Answer::Quit => {
            let row: [&str; 8] = [
                "stopped; the checklist keeps what was placed",
                "detenido; la lista guarda lo colocado",
                "angehalten; die Liste behält das Gesetzte",
                "arrêté ; la liste garde ce qui a été posé",
                "interrotto; l'elenco conserva ciò che è stato posizionato",
                "中断しました。チェックリストに配置済みが残ります",
                "已停止；清单保留已放置的元件",
                "parado; a lista guarda o que foi colocado",
            ];
            crate::msg8::pick(lang, &row).to_string()
        }
    }
}
