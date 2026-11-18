// EH3D-PHOENIX-PNP - the sheet for the person who sets the machine up before a job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The setup sheet: everything a person needs to know before a run of boards, on one page. Which feeder goes in which slot and what it holds, which parts are missing for the run, which
//! nozzle tips the run uses and how worn they are, which calibrations the job depends on and whether they are set, and what does not look right (a bulk feeder that holds a part with a
//! direction, a feeder that is switched off but holds a part the board needs). The words are in the eight languages; the table of the feeders is also given as a CSV with a header that
//! does not depend on the language. Plain text and arithmetic: nothing is read or written here and nothing moves a machine.

use crate::catalog::Catalog;
use crate::config::{MachineConfig, VisionMode};
use crate::error::{Lang, PnpError};
use crate::feeding::{FeederKind, FeederSpec};
use crate::loadsheet::{self, Need};
use crate::nozzlelife::NozzleLife;
use crate::parts::{csv_field, Part, Side};
use crate::polarity;
use crate::stock::stock_plan;
use std::collections::BTreeMap;

/// What the sheet is made from.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub parts: &'a [Part],
    pub feeders: &'a [FeederSpec],
    pub catalog: &'a Catalog,
    pub cfg: &'a MachineConfig,
    /// The side of the board (both sides for `None`).
    pub side: Option<Side>,
    /// How many times the parts that were given are mounted in the run (boards, or panels when the parts are those of a panel).
    pub boards: usize,
    /// What is known of the panel, when the parts are those of a panel.
    pub panel: Option<PanelInfo>,
    /// The use of the nozzle tips so far, when a record of it is kept.
    pub life: Option<&'a NozzleLife>,
}

/// What the sheet needs to know of a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelInfo {
    /// The measured places of the fiducials were given.
    pub fiducials_measured: bool,
}

/// A feeder to load.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederLine {
    /// The slot of the table of feeders, empty when it names none.
    pub slot: String,
    pub id: String,
    pub kind: FeederKind,
    pub value: String,
    pub package: String,
    pub tape_mm: f64,
    pub tip: String,
    /// The parts it holds.
    pub count: u32,
    pub x: f64,
    pub y: f64,
}

/// A kind of part that the feeders that are on do not hold enough of.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingLine {
    pub value: String,
    pub package: String,
    pub needed: u64,
    pub held: u64,
    pub short: u64,
}

/// How a tip stands against `nozzle_life_warn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wear {
    /// Below the level, even after this run, or no level to compare with.
    Fine,
    /// The run takes it to the level.
    Reaches,
    /// It is at the level or over it already.
    Over,
}

/// A nozzle tip that the run uses.
#[derive(Debug, Clone, PartialEq)]
pub struct TipLine {
    pub tip: String,
    /// The parts the run places with it.
    pub parts: u64,
    /// The kinds of part that use it, as `value / package`.
    pub kinds: Vec<String>,
    /// The parts it has placed so far, when a record is kept.
    pub used: Option<u64>,
    pub wear: Wear,
}

/// The calibrations and settings that a job depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calibration {
    Camera,
    NozzleRunout,
    Backlash,
    TipChanger,
    Fiducials,
}

impl Calibration {
    /// The word for it on the command line and in the files.
    pub fn key(self) -> &'static str {
        match self {
            Calibration::Camera => "camera",
            Calibration::NozzleRunout => "runout",
            Calibration::Backlash => "backlash",
            Calibration::TipChanger => "tip_changer",
            Calibration::Fiducials => "fiducials",
        }
    }

    /// The command or the settings that set it.
    pub fn how(self) -> &'static str {
        match self {
            Calibration::Camera => "camera-calibrate",
            Calibration::NozzleRunout => "nozzle-calibrate",
            Calibration::Backlash => "backlash_x_mm, backlash_y_mm",
            Calibration::TipChanger => "tip_changer_x_mm, tip_changer_y_mm",
            Calibration::Fiducials => "--fiducials",
        }
    }
}

/// A calibration the job depends on and whether it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalLine {
    pub item: Calibration,
    pub set: bool,
}

/// Something that does not look right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A feeder that is switched off holds a part that the board needs.
    FeederOff(String),
    /// A bulk feeder holds a part that has a direction (the references listed).
    BulkDirection(String, Vec<String>),
}

/// The sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupSheet {
    pub boards: usize,
    pub side: Option<Side>,
    pub parts_per_board: usize,
    pub kinds: usize,
    pub feeders: Vec<FeederLine>,
    pub missing: Vec<MissingLine>,
    pub tips: Vec<TipLine>,
    pub calibrations: Vec<CalLine>,
    pub warnings: Vec<Warning>,
}

fn key_of(n: &Need) -> String {
    format!(
        "{}|{}",
        n.value.trim().to_ascii_lowercase(),
        n.package.trim().to_ascii_lowercase()
    )
}

/// Makes the sheet. The number of boards must be from 1 to 1000000.
pub fn build(i: &Inputs) -> Result<SetupSheet, PnpError> {
    let stock = stock_plan(i.parts, i.feeders, i.side, i.boards)?;
    let (needs, _) = loadsheet::needs(i.parts, i.side, i.catalog, i.cfg);
    let by_key: BTreeMap<String, &Need> = needs.iter().map(|n| (key_of(n), n)).collect();

    let mut feeders: Vec<FeederLine> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();
    for f in i.feeders {
        let Some(need) = by_key.get(&f.base.key()) else {
            continue;
        };
        if !f.enabled {
            warnings.push(Warning::FeederOff(f.base.id.clone()));
            continue;
        }
        feeders.push(FeederLine {
            slot: f.slot.clone().unwrap_or_default(),
            id: f.base.id.clone(),
            kind: f.kind,
            value: f.base.value.clone(),
            package: f.base.package.clone(),
            tape_mm: need.tape_mm,
            tip: need.tip.clone(),
            count: f.base.count,
            x: f.base.x,
            y: f.base.y,
        });
    }
    feeders.sort_by(|a, b| {
        a.slot
            .is_empty()
            .cmp(&b.slot.is_empty())
            .then_with(|| a.slot.cmp(&b.slot))
            .then_with(|| a.id.cmp(&b.id))
    });

    let missing: Vec<MissingLine> = stock
        .lines
        .iter()
        .filter(|l| l.short > 0)
        .map(|l| MissingLine {
            value: l.value.clone(),
            package: l.package.clone(),
            needed: l.needed,
            held: l.available,
            short: l.short,
        })
        .collect();

    // the tips of the run, with the parts each places and how worn it is
    let mut tips: BTreeMap<String, TipLine> = BTreeMap::new();
    for n in &needs {
        let tip = n.tip.trim();
        if tip.is_empty() {
            continue;
        }
        let line = tips.entry(tip.to_string()).or_insert_with(|| TipLine {
            tip: tip.to_string(),
            parts: 0,
            kinds: Vec::new(),
            used: None,
            wear: Wear::Fine,
        });
        line.parts += n.count as u64 * i.boards as u64;
        line.kinds.push(format!("{} / {}", n.value, n.package));
    }
    let warn_at = i.cfg.nozzle_life_warn;
    for line in tips.values_mut() {
        line.kinds.sort();
        if let Some(life) = i.life {
            let used = life.get(&line.tip).map_or(0, |t| t.uses);
            line.used = Some(used);
            if warn_at > 0 {
                line.wear = if used >= warn_at {
                    Wear::Over
                } else if used.saturating_add(line.parts) >= warn_at {
                    Wear::Reaches
                } else {
                    Wear::Fine
                };
            }
        }
    }
    let tips: Vec<TipLine> = tips.into_values().collect();

    // a bulk feeder gives the parts no fixed direction
    let directed: Vec<Part> = polarity::sheet(
        &i.parts
            .iter()
            .filter(|p| i.side.is_none_or(|s| s == p.side))
            .cloned()
            .collect::<Vec<_>>(),
        &i.cfg.polarity_refs,
        &i.cfg.polarity_packages,
    );
    for f in i
        .feeders
        .iter()
        .filter(|f| f.enabled && !f.kind.keeps_direction())
    {
        let refs: Vec<String> = directed
            .iter()
            .filter(|p| p.key() == f.base.key())
            .map(|p| p.reference.clone())
            .collect();
        if !refs.is_empty() {
            warnings.push(Warning::BulkDirection(f.base.id.clone(), refs));
        }
    }

    let mut calibrations: Vec<CalLine> = Vec::new();
    if i.cfg.align || i.cfg.vision != VisionMode::Off {
        let c = &i.cfg.camera;
        calibrations.push(CalLine {
            item: Calibration::Camera,
            set: c.units_per_px > 0.0 || c.offset_x_mm != 0.0 || c.offset_y_mm != 0.0,
        });
    }
    calibrations.push(CalLine {
        item: Calibration::NozzleRunout,
        set: i.cfg.nozzle_runout_mm > 0.0,
    });
    calibrations.push(CalLine {
        item: Calibration::Backlash,
        set: i.cfg.backlash_x_mm > 0.0 || i.cfg.backlash_y_mm > 0.0,
    });
    if tips.len() > 1 {
        calibrations.push(CalLine {
            item: Calibration::TipChanger,
            set: i.cfg.tip_changer.is_some(),
        });
    }
    if let Some(panel) = i.panel {
        calibrations.push(CalLine {
            item: Calibration::Fiducials,
            set: panel.fiducials_measured,
        });
    }

    Ok(SetupSheet {
        boards: i.boards,
        side: i.side,
        parts_per_board: stock.parts_per_board,
        kinds: stock.lines.len(),
        feeders,
        missing,
        tips,
        calibrations,
        warnings,
    })
}

#[derive(Debug, Clone, Copy)]
enum W {
    Title,
    Boards,
    Side,
    SideTop,
    SideBottom,
    SideBoth,
    PartsPerBoard,
    PartsInRun,
    Kinds,
    SecFeeders,
    SecMissing,
    SecTips,
    SecCalibrations,
    SecWarnings,
    ColSlot,
    ColFeeder,
    ColKind,
    ColPart,
    ColTape,
    ColHeld,
    ColPlace,
    ColTip,
    ColParts,
    ColKinds,
    ColUsed,
    ColNeeded,
    ColShort,
    KindTape,
    KindTube,
    KindBulk,
    KindTray,
    KindStrip,
    None,
    Set,
    NotSet,
    CalCamera,
    CalRunout,
    CalBacklash,
    CalTipChanger,
    CalFiducials,
    LookAtTip,
    ReachesLevel,
    WarnOff,
    WarnBulk,
    Ready,
    NotReady,
}

fn w(word: W, lang: Lang) -> &'static str {
    let row: [&str; 8] = match word {
        W::Title => ["Setup sheet", "Hoja de preparación", "Rüstliste", "Fiche de préparation", "Foglio di preparazione", "段取りシート", "准备清单", "Folha de preparação"],
        W::Boards => ["Boards", "Placas", "Platinen", "Cartes", "Schede", "基板数", "板数", "Placas"],
        W::Side => ["Side", "Cara", "Seite", "Face", "Lato", "面", "面", "Face"],
        W::SideTop => ["top", "superior", "oben", "dessus", "sopra", "表面", "顶面", "superior"],
        W::SideBottom => ["bottom", "inferior", "unten", "dessous", "sotto", "裏面", "底面", "inferior"],
        W::SideBoth => ["both", "ambas", "beide", "les deux", "entrambi", "両面", "两面", "ambas"],
        W::PartsPerBoard => ["Parts per board", "Piezas por placa", "Teile pro Platine", "Pièces par carte", "Componenti per scheda", "1 枚あたりの部品数", "每块板零件数", "Peças por placa"],
        W::PartsInRun => ["Parts for the run", "Piezas para la tirada", "Teile für den Lauf", "Pièces pour la série", "Componenti per la produzione", "ロットの部品数", "批量零件数", "Peças para a série"],
        W::Kinds => ["Kinds of part", "Tipos de pieza", "Teilesorten", "Types de pièce", "Tipi di componente", "部品の種類", "零件种类", "Tipos de peça"],
        W::SecFeeders => ["Feeders to load", "Alimentadores que cargar", "Zuführungen zu bestücken", "Alimentateurs à charger", "Alimentatori da caricare", "セットするフィーダー", "需装载的供料器", "Alimentadores a carregar"],
        W::SecMissing => ["Parts missing", "Piezas que faltan", "Fehlende Teile", "Pièces manquantes", "Componenti mancanti", "不足している部品", "缺少的零件", "Peças em falta"],
        W::SecTips => ["Nozzle tips", "Puntas de boquilla", "Düsenspitzen", "Pointes de buse", "Punte degli ugelli", "ノズルチップ", "吸嘴头", "Pontas dos bicos"],
        W::SecCalibrations => ["Calibrations", "Calibraciones", "Kalibrierungen", "Étalonnages", "Calibrazioni", "校正", "校准", "Calibrações"],
        W::SecWarnings => ["Warnings", "Avisos", "Warnungen", "Avertissements", "Avvisi", "警告", "警告", "Avisos"],
        W::ColSlot => ["Slot", "Ranura", "Platz", "Emplacement", "Alloggiamento", "スロット", "槽位", "Ranhura"],
        W::ColFeeder => ["Feeder", "Alimentador", "Zuführung", "Alimentateur", "Alimentatore", "フィーダー", "供料器", "Alimentador"],
        W::ColKind => ["Kind", "Tipo", "Art", "Type", "Tipo", "種類", "类型", "Tipo"],
        W::ColPart => ["Part", "Pieza", "Teil", "Pièce", "Componente", "部品", "零件", "Peça"],
        W::ColTape => ["Tape (mm)", "Cinta (mm)", "Gurt (mm)", "Bande (mm)", "Nastro (mm)", "テープ (mm)", "料带 (mm)", "Fita (mm)"],
        W::ColHeld => ["Held", "Tiene", "Enthält", "Contient", "Contiene", "保持数", "含量", "Tem"],
        W::ColPlace => ["Place (mm)", "Lugar (mm)", "Ort (mm)", "Endroit (mm)", "Posizione (mm)", "位置 (mm)", "位置 (mm)", "Lugar (mm)"],
        W::ColTip => ["Tip", "Punta", "Spitze", "Pointe", "Punta", "チップ", "吸嘴头", "Ponta"],
        W::ColParts => ["Parts", "Piezas", "Teile", "Pièces", "Componenti", "部品数", "零件数", "Peças"],
        W::ColKinds => ["Kinds", "Tipos", "Sorten", "Types", "Tipi", "種類", "种类", "Tipos"],
        W::ColUsed => ["Used so far", "Usada hasta ahora", "Bisher genutzt", "Utilisée jusqu'ici", "Usata finora", "これまでの使用数", "迄今使用", "Usada até agora"],
        W::ColNeeded => ["Needed", "Necesita", "Benötigt", "Nécessaire", "Necessari", "必要数", "需要", "Precisa"],
        W::ColShort => ["Short", "Faltan", "Fehlen", "Manque", "Mancano", "不足", "缺少", "Faltam"],
        W::KindTape => ["tape", "cinta", "Gurt", "bande", "nastro", "テープ", "料带", "fita"],
        W::KindTube => ["tube", "varilla", "Stange", "tube", "stecca", "チューブ", "料管", "tubo"],
        W::KindBulk => ["bulk", "a granel", "Schüttgut", "vrac", "alla rinfusa", "バルク", "散料", "a granel"],
        W::KindTray => ["tray", "bandeja", "Tray", "plateau", "vassoio", "トレイ", "托盘", "tabuleiro"],
        W::KindStrip => ["strip", "tira", "Streifen", "réglette", "striscia", "ストリップ", "条带", "tira"],
        W::None => ["none", "ninguna", "keine", "aucune", "nessuna", "なし", "无", "nenhuma"],
        W::Set => ["set", "puesta", "gesetzt", "réglée", "impostata", "設定済み", "已设置", "definida"],
        W::NotSet => ["NOT SET", "SIN PONER", "NICHT GESETZT", "NON RÉGLÉE", "NON IMPOSTATA", "未設定", "未设置", "NÃO DEFINIDA"],
        W::CalCamera => ["Camera calibration", "Calibración de la cámara", "Kamerakalibrierung", "Étalonnage de la caméra", "Calibrazione della telecamera", "カメラの校正", "相机校准", "Calibração da câmara"],
        W::CalRunout => ["Runout of the nozzle", "Excentricidad de la boquilla", "Rundlauf der Düse", "Faux-rond de la buse", "Eccentricità dell'ugello", "ノズルの振れ", "吸嘴偏心", "Excentricidade do bico"],
        W::CalBacklash => ["Slack of the axes", "Holgura de los ejes", "Spiel der Achsen", "Jeu des axes", "Gioco degli assi", "軸のバックラッシュ", "轴间隙", "Folga dos eixos"],
        W::CalTipChanger => ["Place of the tip changer", "Lugar del cambiador de puntas", "Ort des Spitzenwechslers", "Endroit du changeur de pointes", "Posizione del cambio punte", "チップチェンジャーの位置", "吸嘴头更换器位置", "Lugar do trocador de pontas"],
        W::CalFiducials => ["Fiducials of the panel measured", "Fiduciales del panel medidos", "Fiducials des Nutzens gemessen", "Repères du panneau mesurés", "Fiduciali del pannello misurati", "パネルのフィデューシャルの測定", "已测量拼板基准点", "Fiduciais do painel medidos"],
        W::LookAtTip => ["look at this tip", "mira esta punta", "diese Spitze prüfen", "regardez cette pointe", "controlla questa punta", "このチップを確認", "请检查该吸嘴头", "olhe para esta ponta"],
        W::ReachesLevel => ["the run takes it to the level of the warning", "la tirada la lleva al nivel de aviso", "der Lauf bringt sie an die Warnschwelle", "la série l'amène au seuil d'alerte", "la produzione la porta al livello di avviso", "このロットで警告レベルに達します", "本批次将使其达到警告阈值", "a série leva-a ao nível de aviso"],
        W::WarnOff => ["the feeder {a} is switched off and holds a part that the board needs: it is not counted", "el alimentador {a} está apagado y tiene una pieza que la placa necesita: no se cuenta", "die Zuführung {a} ist abgeschaltet und enthält ein Teil, das die Platine braucht: sie wird nicht gezählt", "l'alimentateur {a} est éteint et contient une pièce dont la carte a besoin : il n'est pas compté", "l'alimentatore {a} è spento e contiene un componente che serve alla scheda: non viene contato", "フィーダー {a} は停止中ですが、基板に必要な部品を保持しています（数に入れていません）", "供料器 {a} 已关闭，但装有电路板需要的零件，未计入", "o alimentador {a} está desligado e tem uma peça de que a placa precisa: não é contado"],
        W::WarnBulk => ["the bulk feeder {a} holds parts that have a direction ({b}): they come in no fixed direction", "el alimentador a granel {a} tiene piezas con dirección ({b}): no llegan en una dirección fija", "die Schüttgut-Zuführung {a} enthält Teile mit Richtung ({b}): sie kommen in keiner festen Richtung", "l'alimentateur en vrac {a} contient des pièces qui ont un sens ({b}) : elles n'arrivent dans aucune direction fixe", "l'alimentatore alla rinfusa {a} contiene componenti con una direzione ({b}): non arrivano in una direzione fissa", "バルクのフィーダー {a} に向きのある部品 ({b}) が入っています。決まった向きでは来ません", "散料供料器 {a} 装有有方向的零件 ({b})：它们不会以固定方向出料", "o alimentador a granel {a} tem peças com direção ({b}): não chegam numa direção fixa"],
        W::Ready => ["Ready to load: the feeders that are on hold enough for the run.", "Listo para cargar: los alimentadores encendidos tienen bastante para la tirada.", "Bereit zum Bestücken: die eingeschalteten Zuführungen halten genug für den Lauf.", "Prêt à charger : les alimentateurs allumés contiennent assez pour la série.", "Pronto da caricare: gli alimentatori accesi contengono abbastanza per la produzione.", "セット可能：有効なフィーダーでロットに足ります。", "可以装载：已开启的供料器足够本批次使用。", "Pronto para carregar: os alimentadores ligados têm o bastante para a série."],
        W::NotReady => ["NOT ready: parts are missing for the run.", "NO está listo: faltan piezas para la tirada.", "NICHT bereit: für den Lauf fehlen Teile.", "PAS prêt : il manque des pièces pour la série.", "NON pronto: mancano componenti per la produzione.", "未準備：ロットに部品が足りません。", "未就绪：本批次缺少零件。", "NÃO está pronto: faltam peças para a série."],
    };
    row[lang as usize]
}

/// The width of a text on a screen: the letters of the Far East take two columns.
fn width(text: &str) -> usize {
    text.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 2 } else { 1 })
        .sum()
}

fn table(head: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = head.iter().map(|h| width(h)).collect();
    for r in rows {
        for (i, cell) in r.iter().enumerate() {
            widths[i] = widths[i].max(width(cell));
        }
    }
    let line = |cells: Vec<&str>| -> String {
        let mut out = String::from("  ");
        for (i, c) in cells.iter().enumerate() {
            out.push_str(c);
            if i + 1 < cells.len() {
                out.push_str(&" ".repeat(widths[i] - width(c) + 2));
            }
        }
        out.push('\n');
        out
    };
    let mut out = line(head.to_vec());
    for r in rows {
        out.push_str(&line(r.iter().map(String::as_str).collect()));
    }
    out
}

fn kind_word(kind: FeederKind, lang: Lang) -> &'static str {
    w(
        match kind {
            FeederKind::Tape => W::KindTape,
            FeederKind::Tube => W::KindTube,
            FeederKind::Bulk => W::KindBulk,
            FeederKind::Tray { .. } => W::KindTray,
            FeederKind::Strip { .. } => W::KindStrip,
        },
        lang,
    )
}

fn number(v: f64) -> String {
    format!("{v:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

impl SetupSheet {
    /// Whether the feeders that are on hold enough for the run.
    pub fn is_ready(&self) -> bool {
        self.missing.is_empty()
    }

    /// The parts that are missing for the run, all kinds together.
    pub fn short_parts(&self) -> u64 {
        self.missing.iter().map(|m| m.short).sum()
    }

    /// The calibrations the job depends on that are not set.
    pub fn calibrations_not_set(&self) -> usize {
        self.calibrations.iter().filter(|c| !c.set).count()
    }

    /// Two lines for a place where the whole sheet does not fit: whether the run can be loaded, the calibrations that are not set and the number of warnings.
    pub fn summary(&self, lang: Lang) -> String {
        format!(
            "{}\n{}: {} {} / {}   {}: {}",
            w(
                if self.is_ready() {
                    W::Ready
                } else {
                    W::NotReady
                },
                lang
            ),
            w(W::SecCalibrations, lang),
            self.calibrations_not_set(),
            w(W::NotSet, lang),
            self.calibrations.len(),
            w(W::SecWarnings, lang),
            self.warnings.len()
        )
    }

    /// The sheet as text in a language.
    pub fn text(&self, lang: Lang) -> String {
        let side = match self.side {
            Some(Side::Top) => w(W::SideTop, lang),
            Some(Side::Bottom) => w(W::SideBottom, lang),
            None => w(W::SideBoth, lang),
        };
        let mut out = format!("{}\n\n", w(W::Title, lang).to_uppercase());
        out.push_str(&format!(
            "{}: {}   {}: {}   {}: {}   {}: {}   {}: {}\n",
            w(W::Boards, lang),
            self.boards,
            w(W::Side, lang),
            side,
            w(W::Kinds, lang),
            self.kinds,
            w(W::PartsPerBoard, lang),
            self.parts_per_board,
            w(W::PartsInRun, lang),
            self.parts_per_board as u64 * self.boards as u64
        ));
        let none = w(W::None, lang);

        out.push_str(&format!("\n{}\n", w(W::SecFeeders, lang)));
        if self.feeders.is_empty() {
            out.push_str(&format!("  {none}\n"));
        } else {
            let head = [
                w(W::ColSlot, lang),
                w(W::ColFeeder, lang),
                w(W::ColKind, lang),
                w(W::ColPart, lang),
                w(W::ColTape, lang),
                w(W::ColTip, lang),
                w(W::ColHeld, lang),
                w(W::ColPlace, lang),
            ];
            let rows: Vec<Vec<String>> = self
                .feeders
                .iter()
                .map(|f| {
                    vec![
                        if f.slot.is_empty() {
                            "-".to_string()
                        } else {
                            f.slot.clone()
                        },
                        f.id.clone(),
                        kind_word(f.kind, lang).to_string(),
                        format!("{} / {}", f.value, f.package),
                        number(f.tape_mm),
                        if f.tip.is_empty() {
                            "-".to_string()
                        } else {
                            f.tip.clone()
                        },
                        f.count.to_string(),
                        format!("{}, {}", number(f.x), number(f.y)),
                    ]
                })
                .collect();
            out.push_str(&table(&head, &rows));
        }

        out.push_str(&format!("\n{}\n", w(W::SecMissing, lang)));
        if self.missing.is_empty() {
            out.push_str(&format!("  {none}\n"));
        } else {
            let head = [
                w(W::ColPart, lang),
                w(W::ColNeeded, lang),
                w(W::ColHeld, lang),
                w(W::ColShort, lang),
            ];
            let rows: Vec<Vec<String>> = self
                .missing
                .iter()
                .map(|m| {
                    vec![
                        format!("{} / {}", m.value, m.package),
                        m.needed.to_string(),
                        m.held.to_string(),
                        m.short.to_string(),
                    ]
                })
                .collect();
            out.push_str(&table(&head, &rows));
        }

        out.push_str(&format!("\n{}\n", w(W::SecTips, lang)));
        if self.tips.is_empty() {
            out.push_str(&format!("  {none}\n"));
        } else {
            let head = [
                w(W::ColTip, lang),
                w(W::ColParts, lang),
                w(W::ColUsed, lang),
                w(W::ColKinds, lang),
                "",
            ];
            let rows: Vec<Vec<String>> = self
                .tips
                .iter()
                .map(|t| {
                    vec![
                        t.tip.clone(),
                        t.parts.to_string(),
                        t.used.map_or_else(|| "-".to_string(), |u| u.to_string()),
                        t.kinds.join(", "),
                        match t.wear {
                            Wear::Fine => String::new(),
                            Wear::Reaches => format!(
                                "<- {} ({})",
                                w(W::LookAtTip, lang),
                                w(W::ReachesLevel, lang)
                            ),
                            Wear::Over => format!("<- {}", w(W::LookAtTip, lang)),
                        },
                    ]
                })
                .collect();
            out.push_str(&table(&head, &rows));
        }

        out.push_str(&format!("\n{}\n", w(W::SecCalibrations, lang)));
        for c in &self.calibrations {
            let name = w(
                match c.item {
                    Calibration::Camera => W::CalCamera,
                    Calibration::NozzleRunout => W::CalRunout,
                    Calibration::Backlash => W::CalBacklash,
                    Calibration::TipChanger => W::CalTipChanger,
                    Calibration::Fiducials => W::CalFiducials,
                },
                lang,
            );
            out.push_str(&format!(
                "  {name}: {} ({})\n",
                w(if c.set { W::Set } else { W::NotSet }, lang),
                c.item.how()
            ));
        }

        out.push_str(&format!("\n{}\n", w(W::SecWarnings, lang)));
        if self.warnings.is_empty() {
            out.push_str(&format!("  {none}\n"));
        }
        for warning in &self.warnings {
            let line = match warning {
                Warning::FeederOff(id) => w(W::WarnOff, lang).replace("{a}", id),
                Warning::BulkDirection(id, refs) => w(W::WarnBulk, lang)
                    .replace("{a}", id)
                    .replace("{b}", &refs.join(", ")),
            };
            out.push_str(&format!("  {line}\n"));
        }
        out.push_str(&format!(
            "\n{}\n",
            w(
                if self.is_ready() {
                    W::Ready
                } else {
                    W::NotReady
                },
                lang
            )
        ));
        out
    }

    /// The feeders to load as a CSV; its header does not depend on a language.
    pub fn csv(&self) -> String {
        let mut out = String::from("slot,feeder,kind,value,package,tape_mm,tip,parts,x,y\n");
        for f in &self.feeders {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&f.slot),
                csv_field(&f.id),
                f.kind.name(),
                csv_field(&f.value),
                csv_field(&f.package),
                f.tape_mm,
                csv_field(&f.tip),
                f.count,
                f.x,
                f.y
            ));
        }
        out
    }
}
