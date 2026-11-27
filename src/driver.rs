// EH3D-PHOENIX-PNP - a driver session: the steps of a job sent to a controller with the timeouts, the retries and the stop that the profile asks for.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `Session` sends the lines of a profile over a `Link`. A query or a switch is sent again when no answer comes (the profile says how many times), a move is not: when the wait for a
//! move times out the session asks where the head is and goes on only if it is at the target. An error from the controller, a retry that is not enough, a position that is not the one
//! asked for or a refused move stop the session after the lines of `safe_stop` (the pump off, the motors released) and nothing more is sent until a new session: it fails closed. The
//! rules of `safety` slow the moves down after each trouble and never speed them up again. The only link there is today is a simulated one: this is evidence of a simulator.

use crate::error::Lang;
use crate::gcodedriver::{Dialect, DriverProfile, LineFault, Op, Pos, Reply, Target};
use crate::gcodesim::{Link, LinkError};
use crate::interlock::{self, Signals, Violation, MAX_SIGNAL};
use crate::msg8::{num, say};
use crate::plan::Placement;
use crate::safety::{Event, Governor, Level, Rules};

/// What stopped a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No answer, after the tries.
    Timeout,
    /// The controller answered with an error.
    Rejected,
    /// The session is stopped (an earlier failure).
    Halted,
    /// The rules hold the machine.
    Held,
    /// A step the profile cannot write or whose place is outside the limits.
    Refused,
    /// The head is not where it should be.
    Mismatch,
    /// The link is closed or the answer cannot be read.
    Link,
    /// A rule of the profile keeps this move from being made.
    Interlock,
}

/// Why a session failed, with the values the sentence needs, so that it can be said in every language.
#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    /// The session was stopped by an earlier failure.
    Stopped,
    NotConnected,
    Held(Level),
    Interlock(Violation),
    Profile(LineFault),
    LinkClosed(String),
    NoAnswer {
        line: String,
        seconds: f64,
        tries: u32,
    },
    Refused {
        line: String,
        said: String,
    },
    NoOk {
        line: String,
        tries: u32,
    },
    Unreadable {
        line: String,
        said: String,
    },
    NoPosition,
    /// The wait timed out and the head is not at the target.
    NotAtTarget,
    /// The controller says the head is at the target and the wait never said that the moves are over.
    NeverDone,
    Mismatch {
        x: f64,
        y: f64,
        z: f64,
    },
    /// The controller said `start`: it restarted and no longer knows where the head is.
    Restarted {
        line: String,
    },
}

impl Why {
    /// The sentence in a language.
    pub fn localized(&self, lang: Lang) -> String {
        let (row, pairs): ([&'static str; 8], Vec<(&str, String)>) = match self {
            Why::Interlock(v) => return v.localized(lang),
            Why::Profile(f) => return f.localized(lang),
            Why::Stopped => (
                [
                    "the session is stopped",
                    "la sesión está parada",
                    "die Sitzung ist angehalten",
                    "la session est arrêtée",
                    "la sessione è ferma",
                    "セッションは停止しています",
                    "会话已停止",
                    "a sessão está parada",
                ],
                vec![],
            ),
            Why::NotConnected => (
                [
                    "the session is not connected",
                    "la sesión no está conectada",
                    "die Sitzung ist nicht verbunden",
                    "la session n'est pas connectée",
                    "la sessione non è collegata",
                    "セッションは接続されていません",
                    "会话尚未连接",
                    "a sessão não está ligada",
                ],
                vec![],
            ),
            Why::Held(level) => (
                [
                    "the machine is held ({level})",
                    "la máquina está retenida ({level})",
                    "die Maschine ist angehalten ({level})",
                    "la machine est retenue ({level})",
                    "la macchina è ferma ({level})",
                    "機械は保留されています（{level}）",
                    "机器处于保持状态（{level}）",
                    "a máquina está retida ({level})",
                ],
                vec![("level", level.name().to_string())],
            ),
            Why::LinkClosed(why) => (
                [
                    "the link is closed: {why}",
                    "el enlace está cerrado: {why}",
                    "die Verbindung ist geschlossen: {why}",
                    "la liaison est fermée : {why}",
                    "il collegamento è chiuso: {why}",
                    "接続が閉じられました：{why}",
                    "连接已关闭：{why}",
                    "a ligação está fechada: {why}",
                ],
                vec![("why", why.clone())],
            ),
            Why::NoAnswer {
                line,
                seconds,
                tries,
            } => (
                [
                    "no answer to '{line}' in {s} s after {tries} tries",
                    "sin respuesta a '{line}' en {s} s tras {tries} intentos",
                    "keine Antwort auf '{line}' in {s} s nach {tries} Versuchen",
                    "pas de réponse à '{line}' en {s} s après {tries} essais",
                    "nessuna risposta a '{line}' in {s} s dopo {tries} tentativi",
                    "「{line}」への応答が {s} 秒以内にありません（{tries} 回試行）",
                    "“{line}”在 {s} 秒内没有应答（已尝试 {tries} 次）",
                    "sem resposta a '{line}' em {s} s após {tries} tentativas",
                ],
                vec![
                    ("line", line.clone()),
                    ("s", num(*seconds)),
                    ("tries", tries.to_string()),
                ],
            ),
            Why::Refused { line, said } => (
                [
                    "'{line}' was refused: {said}",
                    "'{line}' fue rechazada: {said}",
                    "'{line}' wurde abgelehnt: {said}",
                    "'{line}' a été refusée : {said}",
                    "'{line}' è stata rifiutata: {said}",
                    "「{line}」は拒否されました：{said}",
                    "“{line}”被拒绝：{said}",
                    "'{line}' foi recusada: {said}",
                ],
                vec![("line", line.clone()), ("said", said.clone())],
            ),
            Why::NoOk { line, tries } => (
                [
                    "'{line}' got no ok after {tries} tries",
                    "'{line}' no recibió ok tras {tries} intentos",
                    "'{line}' bekam nach {tries} Versuchen kein ok",
                    "'{line}' n'a pas reçu de ok après {tries} essais",
                    "'{line}' non ha ricevuto ok dopo {tries} tentativi",
                    "「{line}」は {tries} 回試しても ok がありません",
                    "“{line}”尝试 {tries} 次后仍没有 ok",
                    "'{line}' não recebeu ok após {tries} tentativas",
                ],
                vec![("line", line.clone()), ("tries", tries.to_string())],
            ),
            Why::Unreadable { line, said } => (
                [
                    "'{line}' got a line that is not known: {said}",
                    "'{line}' recibió una línea desconocida: {said}",
                    "'{line}' bekam eine unbekannte Zeile: {said}",
                    "'{line}' a reçu une ligne inconnue : {said}",
                    "'{line}' ha ricevuto una riga sconosciuta: {said}",
                    "「{line}」に未知の行が返りました：{said}",
                    "“{line}”收到无法识别的行：{said}",
                    "'{line}' recebeu uma linha desconhecida: {said}",
                ],
                vec![("line", line.clone()), ("said", said.clone())],
            ),
            Why::NoPosition => (
                [
                    "the answer to the question about the position has none",
                    "la respuesta a la pregunta sobre la posición no la trae",
                    "die Antwort auf die Frage nach der Position enthält keine",
                    "la réponse à la question sur la position n'en contient pas",
                    "la risposta alla domanda sulla posizione non ne contiene",
                    "位置の問い合わせへの応答に位置がありません",
                    "对位置查询的应答中没有位置",
                    "a resposta à pergunta sobre a posição não a traz",
                ],
                vec![],
            ),
            Why::NotAtTarget => (
                [
                    "the move did not end in time and the head is not at the target",
                    "el movimiento no terminó a tiempo y el cabezal no está en el destino",
                    "die Bewegung endete nicht rechtzeitig, und der Kopf ist nicht am Ziel",
                    "le déplacement n'a pas fini à temps et la tête n'est pas à la cible",
                    "il movimento non è finito in tempo e la testa non è al bersaglio",
                    "移動が時間内に終わらず、ヘッドは目標にありません",
                    "移动未按时结束，且机头不在目标位置",
                    "o movimento não terminou a tempo e a cabeça não está no destino",
                ],
                vec![],
            ),
            Why::NeverDone => (
                [
                    "the move did not end in time: the head reports the target and the wait never said ok",
                    "el movimiento no terminó a tiempo: el cabezal informa del destino y la espera nunca dijo ok",
                    "die Bewegung endete nicht rechtzeitig: der Kopf meldet das Ziel, und das Warten sagte nie ok",
                    "le déplacement n'a pas fini à temps : la tête annonce la cible et l'attente n'a jamais dit ok",
                    "il movimento non è finito in tempo: la testa riporta il bersaglio e l'attesa non ha mai detto ok",
                    "移動が時間内に終わりません：ヘッドは目標を報告しますが、待機は ok を返しませんでした",
                    "移动未按时结束：机头报告已在目标位置，但等待命令始终没有返回 ok",
                    "o movimento não terminou a tempo: a cabeça indica o destino e a espera nunca disse ok",
                ],
                vec![],
            ),
            Why::Mismatch { x, y, z } => (
                [
                    "the head reports ({x}, {y}, {z}) and not the target",
                    "el cabezal informa ({x}, {y}, {z}) y no el destino",
                    "der Kopf meldet ({x}, {y}, {z}) und nicht das Ziel",
                    "la tête annonce ({x}, {y}, {z}) et non la cible",
                    "la testa riporta ({x}, {y}, {z}) e non il bersaglio",
                    "ヘッドは目標ではなく ({x}, {y}, {z}) を報告しています",
                    "机头报告的是 ({x}, {y}, {z})，而不是目标位置",
                    "a cabeça indica ({x}, {y}, {z}) e não o destino",
                ],
                vec![
                    ("x", format!("{x:.3}")),
                    ("y", format!("{y:.3}")),
                    ("z", format!("{z:.3}")),
                ],
            ),
            Why::Restarted { line } => (
                [
                    "the controller restarted while '{line}' was waited for",
                    "el controlador se reinició mientras se esperaba '{line}'",
                    "die Steuerung ist neu gestartet, während auf '{line}' gewartet wurde",
                    "le contrôleur a redémarré pendant l'attente de '{line}'",
                    "il controllore si è riavviato mentre si attendeva '{line}'",
                    "「{line}」を待つ間にコントローラが再起動しました",
                    "等待“{line}”时控制器重新启动了",
                    "o controlador reiniciou enquanto se esperava por '{line}'",
                ],
                vec![("line", line.clone())],
            ),
        };
        say(lang, &row, &pairs)
    }
}

impl Kind {
    /// The word of the kind of failure in a language.
    pub fn localized(self, lang: Lang) -> &'static str {
        let row: [&'static str; 8] = match self {
            Kind::Timeout => [
                "Timeout",
                "Sin respuesta",
                "Zeitüberschreitung",
                "Délai dépassé",
                "Tempo scaduto",
                "タイムアウト",
                "超时",
                "Sem resposta",
            ],
            Kind::Rejected => [
                "Rejected",
                "Rechazado",
                "Abgelehnt",
                "Refusé",
                "Rifiutato",
                "拒否",
                "被拒绝",
                "Recusado",
            ],
            Kind::Halted => [
                "Halted",
                "Parada",
                "Angehalten",
                "Arrêtée",
                "Fermata",
                "停止中",
                "已停止",
                "Parada",
            ],
            Kind::Held => [
                "Held", "Retenida", "Gesperrt", "Retenue", "Bloccata", "保留", "保持", "Retida",
            ],
            Kind::Refused => [
                "Refused",
                "No permitido",
                "Verweigert",
                "Non permis",
                "Non consentito",
                "不可",
                "不允许",
                "Não permitido",
            ],
            Kind::Mismatch => [
                "Mismatch",
                "Posición distinta",
                "Abweichung",
                "Écart de position",
                "Posizione diversa",
                "位置不一致",
                "位置不符",
                "Posição diferente",
            ],
            Kind::Link => [
                "Link",
                "Enlace",
                "Verbindung",
                "Liaison",
                "Collegamento",
                "接続",
                "连接",
                "Ligação",
            ],
            Kind::Interlock => [
                "Interlock",
                "Bloqueo",
                "Verriegelung",
                "Verrouillage",
                "Interblocco",
                "インターロック",
                "互锁",
                "Bloqueio",
            ],
        };
        crate::msg8::pick(lang, &row)
    }
}

/// A failure of a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub kind: Kind,
    /// The reason in English, as the log of a halt keeps it.
    pub detail: String,
    pub why: Why,
}

impl Failure {
    fn new(kind: Kind, why: Why) -> Failure {
        Failure {
            kind,
            detail: why.localized(Lang::En),
            why,
        }
    }

    /// The reason in a language.
    pub fn localized(&self, lang: Lang) -> String {
        self.why.localized(lang)
    }
}

/// A line sent and what came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at_s: f64,
    pub line: String,
    pub result: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    Ready,
    Halted,
}

/// What the wait for a move came to when it did not end well.
enum Waited {
    Timeout,
    /// An answer without `ok` (the controller only says it is busy): the moves are not over.
    NoOk,
    Failed(Failure),
}

/// What a run of placements gave.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub placed: usize,
    pub failed: Option<(String, Failure)>,
}

/// A session with a controller.
pub struct Session<L: Link> {
    pub profile: DriverProfile,
    pub link: L,
    pub governor: Governor,
    pub log: Vec<Entry>,
    /// Whether every move is followed by a question about where the head is.
    pub verify_moves: bool,
    /// The signals the interlocks of the profile look at: the actuators (what was last switched) and what a sensor was reported to be.
    pub signals: Signals,
    /// The lines of `safe_stop` that the controller did not confirm with `ok` when the session was stopped: a person has to look at what they switch off.
    pub unconfirmed: Vec<String>,
    state: State,
}

/// The most signals a session keeps.
pub const MAX_SIGNALS: usize = 64;

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

impl<L: Link> Session<L> {
    pub fn new(profile: &DriverProfile, link: L) -> Session<L> {
        Session {
            profile: profile.clone(),
            link,
            governor: Governor::new(Rules::from_profile(profile)),
            log: Vec::new(),
            verify_moves: true,
            signals: profile
                .actuators
                .iter()
                .map(|a| (a.name.clone(), false))
                .collect(),
            unconfirmed: Vec::new(),
            state: State::New,
        }
    }

    /// Tells the session what a sensor says (a door closed, the nozzles lifted). A name that is not a name (letters, digits and `_`, at most `MAX_SIGNAL`) and a new name when `MAX_SIGNALS`
    /// are kept are refused, and the answer is false.
    pub fn set_signal(&mut self, name: &str, on: bool) -> bool {
        let valid = !name.is_empty()
            && name.len() <= MAX_SIGNAL
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid || (self.signals.len() >= MAX_SIGNALS && !self.signals.contains_key(name)) {
            return false;
        }
        self.signals.insert(name.to_string(), on);
        true
    }

    /// Whether the session was stopped by a failure.
    pub fn is_halted(&self) -> bool {
        self.state == State::Halted
    }

    fn note(&mut self, line: &str, result: &str) {
        let at_s = self.link.now();
        self.log.push(Entry {
            at_s,
            line: line.to_string(),
            result: result.to_string(),
        });
    }

    /// Takes an event into the rules (the vacuum that was lost, for one, is known to the caller and not to the controller).
    pub fn event(&mut self, e: Event) -> Level {
        self.governor.observe(e)
    }

    /// Stops the session after sending the lines that leave the machine safe. A line that gets no `ok` is sent again up to `retries` times (they only switch things off); one that is
    /// still not confirmed, or is refused, is kept in `unconfirmed`, and the others are still sent.
    fn halt(&mut self, why: &str) {
        if self.state == State::Halted {
            return;
        }
        self.state = State::Halted;
        self.note("(halt)", why);
        let lines = self.profile.safe_stop.clone();
        let timeout = self.profile.command_timeout_s;
        for l in lines {
            let mut tries = 0u32;
            loop {
                let (result, confirmed, again) = match self.link.exchange(&l, timeout) {
                    Ok(answer) => {
                        let replies: Vec<Reply> =
                            answer.iter().map(|a| self.profile.classify(a)).collect();
                        if let Some(Reply::Error(m)) =
                            replies.iter().find(|r| matches!(r, Reply::Error(_)))
                        {
                            (format!("refused: {m}"), false, false)
                        } else if replies.contains(&Reply::Ok) {
                            ("sent".to_string(), true, false)
                        } else {
                            ("no ok".to_string(), false, true)
                        }
                    }
                    Err(LinkError::Timeout) => ("no answer".to_string(), false, true),
                    Err(e) => (format!("not sent: {e:?}"), false, false),
                };
                self.note(&l, &result);
                if confirmed {
                    break;
                }
                if again && tries < self.profile.retries {
                    tries += 1;
                    self.link.pause(self.profile.retry_wait_s);
                    continue;
                }
                self.unconfirmed.push(l.clone());
                break;
            }
        }
    }

    /// Marlin writes `start` when it boots. After the session is connected that means the controller restarted: it no longer knows where the head is or what was switched on.
    fn restarted(&self, answer: &[String]) -> bool {
        self.state == State::Ready
            && self.profile.dialect == Dialect::Marlin
            && answer.iter().any(|l| l.trim() == "start")
    }

    fn fail(&mut self, kind: Kind, why: Why, event: Event) -> Failure {
        self.governor.observe(event);
        let failure = Failure::new(kind, why);
        self.halt(&failure.detail);
        failure
    }

    /// Sends a line. With `again` a missing answer is asked for again up to `retries` times, with the pause of the profile between them.
    fn send(&mut self, line: &str, timeout_s: f64, again: bool) -> Result<Vec<Reply>, Failure> {
        if self.state == State::Halted {
            return Err(Failure::new(Kind::Halted, Why::Stopped));
        }
        let mut tries = 0u32;
        loop {
            let answer = self.link.exchange(line, timeout_s);
            let lines = match answer {
                Ok(l) => l,
                Err(LinkError::Closed(why)) => {
                    self.note(line, "link closed");
                    return Err(self.fail(Kind::Link, Why::LinkClosed(why), Event::Timeout));
                }
                Err(LinkError::Timeout) => {
                    self.note(line, "timeout");
                    self.governor.observe(Event::Timeout);
                    if again && tries < self.profile.retries {
                        tries += 1;
                        self.link.pause(self.profile.retry_wait_s);
                        self.governor.observe(Event::Retry);
                        continue;
                    }
                    return Err(self.fail(
                        Kind::Timeout,
                        Why::NoAnswer {
                            line: line.to_string(),
                            seconds: timeout_s,
                            tries: tries + 1,
                        },
                        Event::Timeout,
                    ));
                }
            };
            if self.restarted(&lines) {
                self.note(line, "controller restarted");
                return Err(self.fail(
                    Kind::Link,
                    Why::Restarted {
                        line: line.to_string(),
                    },
                    Event::ErrorReply,
                ));
            }
            let replies: Vec<Reply> = lines.iter().map(|l| self.profile.classify(l)).collect();
            if let Some(Reply::Error(m)) = replies.iter().find(|r| matches!(r, Reply::Error(_))) {
                let m = m.clone();
                self.note(line, &format!("error: {m}"));
                return Err(self.fail(
                    Kind::Rejected,
                    Why::Refused {
                        line: line.to_string(),
                        said: m,
                    },
                    Event::ErrorReply,
                ));
            }
            if !replies.contains(&Reply::Ok) {
                let busy = replies.contains(&Reply::Busy);
                self.note(line, if busy { "busy" } else { "no ok" });
                self.governor.observe(Event::Timeout);
                if again && tries < self.profile.retries {
                    tries += 1;
                    self.link.pause(self.profile.retry_wait_s);
                    self.governor.observe(Event::Retry);
                    continue;
                }
                return Err(self.fail(
                    Kind::Timeout,
                    Why::NoOk {
                        line: line.to_string(),
                        tries: tries + 1,
                    },
                    Event::Timeout,
                ));
            }
            if !self.profile.tolerate_unknown_echo {
                if let Some(Reply::Other(t)) = replies.iter().find(|r| matches!(r, Reply::Other(_)))
                {
                    let t = t.clone();
                    self.note(line, &format!("unreadable: {t}"));
                    return Err(self.fail(
                        Kind::Link,
                        Why::Unreadable {
                            line: line.to_string(),
                            said: t,
                        },
                        Event::ErrorReply,
                    ));
                }
            }
            self.note(line, "ok");
            self.governor.observe(Event::Success);
            return Ok(replies);
        }
    }

    /// Opens the session: the lines of `connect`, then the absolute mode.
    pub fn connect(&mut self) -> Result<(), Failure> {
        if self.state == State::Halted {
            return Err(Failure::new(Kind::Halted, Why::Stopped));
        }
        let t = self.profile.connect_timeout_s;
        for l in self.profile.connect.clone() {
            self.send(&l, t, true)?;
        }
        let abs = self.profile.absolute_mode.clone();
        self.send(&abs, t, true)?;
        self.state = State::Ready;
        Ok(())
    }

    fn ready(&self) -> Result<(), Failure> {
        match self.state {
            State::Ready => Ok(()),
            State::Halted => Err(Failure::new(Kind::Halted, Why::Stopped)),
            State::New => Err(Failure::new(Kind::Refused, Why::NotConnected)),
        }
    }

    /// Homes the machine. Homing moves X, Y and Z, so the interlocks of those axes and the hold of the rules of safety apply to it as to any move.
    pub fn home(&mut self) -> Result<(), Failure> {
        self.ready()?;
        let all = Target {
            x: Some(0.0),
            y: Some(0.0),
            z: Some(0.0),
            rot: None,
        };
        if let Err(violation) = interlock::check(&self.profile.interlocks, &self.signals, &all) {
            return Err(self.fail(
                Kind::Interlock,
                Why::Interlock(violation),
                Event::LimitRefused,
            ));
        }
        if !self.governor.limits().moves {
            return Err(Failure::new(Kind::Held, Why::Held(self.governor.level())));
        }
        let t = self.profile.move_timeout_s;
        for l in self.profile.home_lines() {
            self.send(&l, t, false)?;
        }
        Ok(())
    }

    /// Asks where the head is.
    pub fn position(&mut self) -> Result<Pos, Failure> {
        self.ready()?;
        let line = self.profile.position_request.clone();
        let t = self.profile.command_timeout_s;
        let replies = self.send(&line, t, true)?;
        replies
            .into_iter()
            .find_map(|r| match r {
                Reply::Position(p) => Some(p),
                _ => None,
            })
            .ok_or_else(|| {
                let f = Failure::new(Kind::Link, Why::NoPosition);
                self.halt(&f.detail);
                f
            })
    }

    fn at_target(&self, p: &Pos, t: &Target) -> bool {
        let tol = self.profile.position_tol_mm;
        t.x.is_none_or(|v| near(p.x, v, tol))
            && t.y.is_none_or(|v| near(p.y, v, tol))
            && t.z.is_none_or(|v| near(p.z, v, tol))
            && match (t.rot, p.rot) {
                (Some(want), Some(got)) => {
                    let d = (want - got).rem_euclid(360.0);
                    d.min(360.0 - d) <= tol.max(0.01)
                }
                _ => true,
            }
    }

    /// Moves the head. The feed is cut to the share the rules allow, the move is refused when they hold the machine, and the head is asked for its place afterward (`verify_moves`).
    pub fn move_to(&mut self, target: &Target, feed: f64) -> Result<(), Failure> {
        self.ready()?;
        if let Err(violation) = interlock::check(&self.profile.interlocks, &self.signals, target) {
            return Err(self.fail(
                Kind::Interlock,
                Why::Interlock(violation),
                Event::LimitRefused,
            ));
        }
        let limits = self.governor.limits();
        if !limits.moves {
            return Err(Failure::new(Kind::Held, Why::Held(self.governor.level())));
        }
        let used = (feed * limits.speed_pct / 100.0).max(1.0);
        let line = match self.profile.move_line_checked(target, used) {
            Ok(l) => l,
            Err(e) => {
                return Err(self.fail(Kind::Refused, Why::Profile(e), Event::LimitRefused));
            }
        };
        let tc = self.profile.command_timeout_s;
        self.send(&line, tc, false)?;
        let wait = self.profile.wait.clone();
        let tm = self.profile.move_timeout_s;
        let mut tries = 0u32;
        // whether the head said it is at the target while the wait had not yet said that the moves are over
        let mut doubted = false;
        loop {
            match self.send_wait(&wait, tm) {
                Ok(()) => {
                    if doubted {
                        self.note(
                            "(recovered)",
                            "the head is at the target and the wait said ok",
                        );
                    }
                    break;
                }
                Err(Waited::Timeout) => {
                    // the move may be done and the answer lost: ask where the head is, and then wait again. A controller reports the place it was told to go to while the
                    // head is still on its way, so the position alone does not end a move
                    let pos = self.position()?;
                    let there = self.at_target(&pos, target);
                    let more = if there {
                        self.profile.retries.max(1)
                    } else {
                        self.profile.retries
                    };
                    if tries >= more {
                        let why = if there {
                            Why::NeverDone
                        } else {
                            Why::NotAtTarget
                        };
                        return Err(self.fail(Kind::Timeout, why, Event::Timeout));
                    }
                    doubted = there;
                    tries += 1;
                    self.governor.observe(Event::Retry);
                    self.link.pause(self.profile.retry_wait_s);
                }
                Err(Waited::NoOk) => {
                    if tries >= self.profile.retries {
                        let why = Why::NoOk {
                            line: wait.clone(),
                            tries: tries + 1,
                        };
                        return Err(self.fail(Kind::Timeout, why, Event::Timeout));
                    }
                    tries += 1;
                    self.governor.observe(Event::Retry);
                    self.link.pause(self.profile.retry_wait_s);
                }
                Err(Waited::Failed(f)) => return Err(f),
            }
        }
        if self.verify_moves {
            let pos = self.position()?;
            if !self.at_target(&pos, target) {
                return Err(self.fail(
                    Kind::Mismatch,
                    Why::Mismatch {
                        x: pos.x,
                        y: pos.y,
                        z: pos.z,
                    },
                    Event::PositionMismatch,
                ));
            }
        }
        Ok(())
    }

    /// The line that waits for a move: one try. A timeout is given back to the caller to settle, and is not yet a failure.
    fn send_wait(&mut self, line: &str, timeout_s: f64) -> Result<(), Waited> {
        if self.state == State::Halted {
            return Err(Waited::Failed(Failure::new(Kind::Halted, Why::Stopped)));
        }
        match self.link.exchange(line, timeout_s) {
            Err(LinkError::Timeout) => {
                self.note(line, "timeout");
                Err(Waited::Timeout)
            }
            Err(LinkError::Closed(why)) => {
                self.note(line, "link closed");
                Err(Waited::Failed(self.fail(
                    Kind::Link,
                    Why::LinkClosed(why),
                    Event::Timeout,
                )))
            }
            Ok(lines) => {
                if self.restarted(&lines) {
                    self.note(line, "controller restarted");
                    return Err(Waited::Failed(self.fail(
                        Kind::Link,
                        Why::Restarted {
                            line: line.to_string(),
                        },
                        Event::ErrorReply,
                    )));
                }
                let replies: Vec<Reply> = lines.iter().map(|l| self.profile.classify(l)).collect();
                if let Some(Reply::Error(m)) = replies.iter().find(|r| matches!(r, Reply::Error(_)))
                {
                    let m = m.clone();
                    self.note(line, &format!("error: {m}"));
                    return Err(Waited::Failed(self.fail(
                        Kind::Rejected,
                        Why::Refused {
                            line: line.to_string(),
                            said: m,
                        },
                        Event::ErrorReply,
                    )));
                }
                if !replies.contains(&Reply::Ok) {
                    let busy = replies.contains(&Reply::Busy);
                    self.note(line, if busy { "busy" } else { "no ok" });
                    self.governor.observe(Event::Timeout);
                    return Err(Waited::NoOk);
                }
                self.note(line, "ok");
                self.governor.observe(Event::Success);
                Ok(())
            }
        }
    }

    /// Switches an actuator and waits for the controller to say it is done.
    pub fn set_actuator(&mut self, name: &str, on: bool) -> Result<(), Failure> {
        self.ready()?;
        let line = match self.profile.actuator_line_checked(name, on) {
            Ok(l) => l,
            Err(e) => {
                return Err(self.fail(Kind::Refused, Why::Profile(e), Event::LimitRefused));
            }
        };
        let t = self.profile.command_timeout_s;
        self.send(&line, t, true)?;
        let wait = self.profile.wait.clone();
        self.send(&wait, t, true)?;
        self.signals.insert(name.to_string(), on);
        Ok(())
    }

    /// Does one step.
    pub fn run_op(&mut self, op: &Op) -> Result<(), Failure> {
        match op {
            Op::Move(t, feed) => self.move_to(t, *feed),
            Op::Actuator(n, on) => self.set_actuator(n, *on),
            Op::Dwell(ms) => {
                self.ready()?;
                let line = self.profile.dwell_line(*ms);
                let t = self.profile.command_timeout_s + f64::from(*ms) / 1000.0;
                self.send(&line, t, false).map(|_| ())
            }
            Op::Wait => {
                self.ready()?;
                let w = self.profile.wait.clone();
                let t = self.profile.move_timeout_s;
                self.send(&w, t, true).map(|_| ())
            }
            Op::Query => self.position().map(|_| ()),
        }
    }

    /// Places the parts one after the other. The first failure ends the run and says which part it was. A run that ends because the rules hold the machine also sends the lines of
    /// `safe_stop`: nothing of it is left switched on.
    pub fn run_placements(&mut self, placements: &[Placement]) -> RunReport {
        let mut placed = 0usize;
        for p in placements {
            let ops = match self.profile.placement_ops_checked(p) {
                Ok(o) => o,
                Err(e) => {
                    let f = self.fail(Kind::Refused, Why::Profile(e), Event::LimitRefused);
                    return RunReport {
                        placed,
                        failed: Some((p.reference.clone(), f)),
                    };
                }
            };
            for op in &ops {
                if let Err(f) = self.run_op(op) {
                    // a hold does not stop the session by itself, and the pump of a part half placed would be left running
                    if self.state == State::Ready {
                        self.halt(&f.detail);
                    }
                    return RunReport {
                        placed,
                        failed: Some((p.reference.clone(), f)),
                    };
                }
            }
            placed += 1;
        }
        RunReport {
            placed,
            failed: None,
        }
    }

    /// Ends the session the proper way: the motors are released.
    pub fn close(&mut self) -> Result<(), Failure> {
        if self.state == State::Halted {
            return Ok(());
        }
        let t = self.profile.command_timeout_s;
        for l in self.profile.disable.clone() {
            self.send(&l, t, true)?;
        }
        self.state = State::New;
        Ok(())
    }
}
