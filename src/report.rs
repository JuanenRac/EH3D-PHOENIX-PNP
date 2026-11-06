//! The report of a job after it was simulated: how many parts were placed, put aside, left out or not to be mounted, where the time went, how the picks went at the first attempt, how
//! far the camera found the parts from the nozzle, the same by package, feeder, nozzle, nozzle tip and board of a panel, the errors that were raised with their codes, and what the person
//! decided when an error stopped the job. It is made from the finished (or stopped) job, as a text in a language, as sections of the CSV of the statistics, as JSON and as a page of HTML
//! that holds everything it shows (no script, no picture and nothing from outside).

use crate::error::{Code, Lang, PnpError};
use crate::job::{Action, Job, Phase, Status};
use crate::model::{Machine, Offset};
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;

/// The bins of every histogram of the correction of the camera.
pub const HIST_BINS: usize = 8;

/// A finding of the checks before the job, as it is shown in the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportCheck {
    pub severity: String,
    pub source: String,
    pub message: String,
}

/// What the report needs besides the job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportInput {
    /// The references of the parts that are not to be mounted (they are not in the plan).
    pub dnp: Vec<String>,
    /// The findings of the checks before the job.
    pub checks: Vec<ReportCheck>,
}

/// The parts of one group (a nozzle, a tip, a board) and what became of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub planned: usize,
    pub placed: usize,
    pub deferred: usize,
    pub skipped: usize,
}

impl Group {
    /// The parts of the group that are not placed (put aside, left out or still pending).
    pub fn not_placed(&self) -> usize {
        self.planned - self.placed
    }
}

/// A histogram of one quantity of the correction of the camera: `HIST_BINS` bins of the same width from `lo` to `hi`; what is outside goes to the bin at the end.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub name: &'static str,
    pub unit: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub counts: Vec<u32>,
    pub mean: f64,
}

impl Histogram {
    /// The edges of bin `i`.
    pub fn bin(&self, i: usize) -> (f64, f64) {
        let width = (self.hi - self.lo) / self.counts.len().max(1) as f64;
        (self.lo + width * i as f64, self.lo + width * (i + 1) as f64)
    }
}

/// How many of `values` fall in each of `bins` bins of the same width between `lo` and `hi`. A value below `lo` goes to the first bin and one at `hi` or above to the last.
pub fn histogram(values: &[f64], lo: f64, hi: f64, bins: usize) -> Vec<u32> {
    let mut counts = vec![0u32; bins];
    if bins == 0 || hi <= lo || !lo.is_finite() || !hi.is_finite() {
        return counts;
    }
    let width = (hi - lo) / bins as f64;
    for v in values.iter().copied().filter(|v| v.is_finite()) {
        let at = ((v - lo) / width).floor();
        let i = if at < 0.0 {
            0
        } else if at >= bins as f64 {
            bins - 1
        } else {
            at as usize
        };
        counts[i] += 1;
    }
    counts
}

/// The numbers of the report of a job.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkReport {
    pub phase: Phase,
    pub planned: usize,
    pub placed: usize,
    pub deferred: usize,
    pub skipped: usize,
    pub pending: usize,
    pub dnp: Vec<String>,
    pub checks: Vec<ReportCheck>,
    pub seconds_total: f64,
    pub seconds_pick: f64,
    pub seconds_align: f64,
    pub seconds_place: f64,
    pub seconds_travel: f64,
    pub seconds_tip: f64,
    pub parts_per_hour: f64,
    pub seconds_per_part: f64,
    pub first_try: u32,
    pub retried: u32,
    pub lost: u32,
    pub align_on: bool,
    pub corrected: u32,
    pub over_tolerance: u32,
    pub no_reading: u32,
    pub mean_mm: f64,
    pub max_mm: f64,
    pub max_deg: f64,
    pub packages: Vec<crate::job::KindStats>,
    pub feeders: Vec<crate::job::FeederStats>,
    pub nozzles: Vec<Group>,
    pub tips: Vec<Group>,
    pub boards: Vec<Group>,
    pub histograms: Vec<Histogram>,
    pub codes: Vec<(Code, u32)>,
    pub actions: [u32; 4],
    pub put_aside: Vec<(String, String)>,
    pub left_out: Vec<(String, String)>,
}

/// The board of a part of a panel: what comes before the last `/` of its reference; `-` for a part of a single board.
fn board_of(reference: &str) -> String {
    match reference.rfind('/') {
        Some(at) => reference[..at].to_string(),
        None => "-".to_string(),
    }
}

fn add_to(groups: &mut Vec<Group>, name: &str, status: Status) {
    let at = match groups.iter().position(|g| g.name == name) {
        Some(at) => at,
        None => {
            groups.push(Group {
                name: name.to_string(),
                planned: 0,
                placed: 0,
                deferred: 0,
                skipped: 0,
            });
            groups.len() - 1
        }
    };
    let g = &mut groups[at];
    g.planned += 1;
    match status {
        Status::Placed => g.placed += 1,
        Status::Deferred => g.deferred += 1,
        Status::Skipped => g.skipped += 1,
        Status::Pending => {}
    }
}

fn us_to_s(us: u64) -> f64 {
    us as f64 / 1e6
}

/// The report of a job.
pub fn build(job: &Job, input: &ReportInput) -> WorkReport {
    let s = job.stats();
    let t = job.trace();
    let cfg = job.config();
    let plan = &job.plan().plan;
    let mut nozzles: Vec<Group> = Vec::new();
    let mut tips: Vec<Group> = Vec::new();
    let mut boards: Vec<Group> = Vec::new();
    let mut put_aside = Vec::new();
    let mut left_out = Vec::new();
    let mut pending = 0usize;
    for (i, p) in plan.placements.iter().enumerate() {
        let status = job.status()[i];
        add_to(&mut nozzles, &(p.nozzle + 1).to_string(), status);
        let tip = job.plan().details.get(i).map_or("", |d| d.tip.as_str());
        add_to(&mut tips, if tip.is_empty() { "-" } else { tip }, status);
        add_to(&mut boards, &board_of(&p.reference), status);
        match status {
            Status::Pending => pending += 1,
            Status::Deferred => put_aside.push((p.reference.clone(), job.reason(i).to_string())),
            Status::Skipped => left_out.push((p.reference.clone(), job.reason(i).to_string())),
            Status::Placed => {}
        }
    }
    nozzles.sort_by_key(|g| g.name.parse::<usize>().unwrap_or(usize::MAX));
    let seconds_total = us_to_s(job.machine().now_us());
    let placed = s.placed;
    let mut histograms = Vec::new();
    if cfg.align {
        let tol = cfg.pick_tolerance_mm;
        let read = |pick: fn(&Offset) -> f64| -> Vec<f64> { t.readings.iter().map(pick).collect() };
        let dx = read(|o| o.dx_mm);
        let dy = read(|o| o.dy_mm);
        let turn = read(|o| o.rotation_deg);
        let mean = |v: &[f64]| {
            if v.is_empty() {
                0.0
            } else {
                v.iter().sum::<f64>() / v.len() as f64
            }
        };
        let limit = turn.iter().fold(1.0f64, |m, d| m.max(d.abs().ceil()));
        for (name, unit, values, lo, hi) in [
            ("dx", "mm", &dx, -tol, tol),
            ("dy", "mm", &dy, -tol, tol),
            ("turn", "deg", &turn, -limit, limit),
        ] {
            histograms.push(Histogram {
                name,
                unit,
                lo,
                hi,
                counts: histogram(values, lo, hi, HIST_BINS),
                mean: mean(values),
            });
        }
    }
    let codes: Vec<(Code, u32)> = t
        .codes
        .iter()
        .filter_map(|(n, times)| {
            Code::ALL
                .iter()
                .copied()
                .find(|c| c.number() == *n)
                .map(|c| (c, *times))
        })
        .collect();
    WorkReport {
        phase: job.phase(),
        planned: plan.placements.len(),
        placed,
        deferred: s.deferred,
        skipped: s.skipped,
        pending,
        dnp: input.dnp.clone(),
        checks: input.checks.clone(),
        seconds_total,
        seconds_pick: us_to_s(t.pick_us),
        seconds_align: us_to_s(t.align_us),
        seconds_place: us_to_s(t.place_us),
        seconds_travel: us_to_s(t.travel_us),
        seconds_tip: us_to_s(t.tip_us),
        parts_per_hour: job.parts_per_hour(),
        seconds_per_part: if placed > 0 {
            seconds_total / placed as f64
        } else {
            0.0
        },
        first_try: t.first_try,
        retried: t.retried,
        lost: t.lost,
        align_on: cfg.align,
        corrected: s.align.corrected,
        over_tolerance: s.align.rejected,
        no_reading: s.align.camera_failures,
        mean_mm: s.align.mean_mm(),
        max_mm: s.align.max_mm,
        max_deg: s.align.max_deg,
        packages: s.kinds.clone(),
        feeders: s.feeders.clone(),
        nozzles,
        tips,
        boards,
        histograms,
        codes,
        actions: t.actions,
        put_aside,
        left_out,
    }
}

impl WorkReport {
    /// The share of the parts that were picked at the first attempt among all that were tried: 0 to 1, 0 when none was tried.
    pub fn first_try_ratio(&self) -> f64 {
        let tried = self.first_try + self.retried + self.lost;
        if tried > 0 {
            f64::from(self.first_try) / f64::from(tried)
        } else {
            0.0
        }
    }

    /// The seconds of the five phases, in the order pick, align, place, travel, tip change.
    pub fn phases(&self) -> [f64; 5] {
        [
            self.seconds_pick,
            self.seconds_align,
            self.seconds_place,
            self.seconds_travel,
            self.seconds_tip,
        ]
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------
// The words of the text and of the page, in the eight languages.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum K {
    Title,
    State,
    PhaseReady,
    PhasePaused,
    PhaseStopped,
    PhaseFinished,
    Parts,
    InPlan,
    Placed,
    Deferred,
    Skipped,
    Pending,
    Dnp,
    Time,
    Total,
    PerPart,
    PerHour,
    Phase,
    Seconds,
    Share,
    PhasePick,
    PhaseAlign,
    PhasePlace,
    PhaseTravel,
    PhaseTip,
    FirstTry,
    AfterRetries,
    Lost,
    Camera,
    Corrected,
    OverTolerance,
    NoReading,
    MeanDistance,
    LargestDistance,
    LargestTurn,
    ByPackage,
    ByFeeder,
    ByNozzle,
    ByTip,
    ByBoard,
    ColName,
    ColPlanned,
    ColPicked,
    ColPickFailed,
    ColPlaceFailed,
    ColFaults,
    ColSwitchedOff,
    ColNotPlaced,
    Hist,
    HistDx,
    HistDy,
    HistTurn,
    Mean,
    Errors,
    Findings,
    PutAside,
    LeftOut,
    DnpList,
    Operator,
    ActRetry,
    ActSkip,
    ActDefer,
    ActSwitchOff,
    NoneWord,
    Simulated,
    Range,
    Count,
}

#[rustfmt::skip]
fn word(k: K, lang: Lang) -> &'static str {
    let row: [&'static str; 8] = match k {
        K::Title => ["Work report", "Informe de trabajo", "Arbeitsbericht", "Rapport de travail", "Rapporto di lavoro", "作業レポート", "作业报告", "Relatório de trabalho"],
        K::State => ["State", "Estado", "Zustand", "État", "Stato", "状態", "状态", "Estado"],
        K::PhaseReady => ["ready", "listo", "bereit", "prêt", "pronto", "準備完了", "就绪", "pronto"],
        K::PhasePaused => ["paused", "en pausa", "pausiert", "en pause", "in pausa", "一時停止", "已暂停", "em pausa"],
        K::PhaseStopped => ["stopped by an error", "detenido por un error", "durch einen Fehler gestoppt", "arrêté par une erreur", "fermato da un errore", "エラーで停止", "因错误停止", "parado por um erro"],
        K::PhaseFinished => ["finished", "terminado", "beendet", "terminé", "terminato", "完了", "已完成", "concluído"],
        K::Parts => ["Parts", "Piezas", "Teile", "Pièces", "Parti", "部品", "零件", "Peças"],
        K::InPlan => ["In the plan", "En el plan", "Im Plan", "Dans le plan", "Nel piano", "計画内", "计划中", "No plano"],
        K::Placed => ["Placed", "Colocadas", "Bestückt", "Placées", "Posizionate", "搭載済み", "已贴装", "Colocadas"],
        K::Deferred => ["Put aside", "Aplazadas", "Zurückgestellt", "Mises de côté", "Messe da parte", "保留", "已搁置", "Adiadas"],
        K::Skipped => ["Left out by the person", "Saltadas por la persona", "Von der Person ausgelassen", "Ignorées par la personne", "Saltate dalla persona", "オペレーターがスキップ", "操作员跳过", "Saltadas pela pessoa"],
        K::Pending => ["Still pending", "Aún pendientes", "Noch offen", "Encore en attente", "Ancora in attesa", "未処理", "尚未处理", "Ainda pendentes"],
        K::Dnp => ["Not to be mounted (DNP)", "No montar (DNP)", "Nicht zu bestücken (DNP)", "À ne pas monter (DNP)", "Da non montare (DNP)", "実装しない (DNP)", "不贴装 (DNP)", "Não montar (DNP)"],
        K::Time => ["Time", "Tiempo", "Zeit", "Temps", "Tempo", "時間", "时间", "Tempo"],
        K::Total => ["Machine time", "Tiempo de máquina", "Maschinenzeit", "Temps machine", "Tempo macchina", "マシン時間", "机器时间", "Tempo de máquina"],
        K::PerPart => ["per placed part", "por pieza colocada", "pro bestücktes Teil", "par pièce placée", "per parte posizionata", "搭載1点あたり", "每个已贴装零件", "por peça colocada"],
        K::PerHour => ["parts per hour (simulated)", "piezas por hora (simulado)", "Teile pro Stunde (simuliert)", "pièces par heure (simulé)", "parti all'ora (simulato)", "部品/時間（シミュレーション）", "零件/小时（模拟）", "peças por hora (simulado)"],
        K::Phase => ["Phase", "Fase", "Phase", "Phase", "Fase", "工程", "阶段", "Fase"],
        K::Seconds => ["Seconds", "Segundos", "Sekunden", "Secondes", "Secondi", "秒", "秒", "Segundos"],
        K::Share => ["Share", "Parte", "Anteil", "Part", "Quota", "割合", "占比", "Parte"],
        K::PhasePick => ["Feed and pick", "Alimentar y recoger", "Zuführen und Aufnehmen", "Alimenter et prélever", "Alimentare e prelevare", "供給とピック", "送料与拾取", "Alimentar e recolher"],
        K::PhaseAlign => ["Align (camera)", "Alinear (cámara)", "Ausrichten (Kamera)", "Aligner (caméra)", "Allineare (telecamera)", "アライメント（カメラ）", "对位（相机）", "Alinhar (câmara)"],
        K::PhasePlace => ["Place", "Colocar", "Bestücken", "Placer", "Posizionare", "搭載", "贴装", "Colocar"],
        K::PhaseTravel => ["Moves of the head", "Desplazamientos del cabezal", "Kopfbewegungen", "Déplacements de la tête", "Spostamenti della testa", "ヘッドの移動", "吸头移动", "Deslocações da cabeça"],
        K::PhaseTip => ["Change of tip", "Cambio de punta", "Düsenwechsel", "Changement de buse", "Cambio di punta", "ノズル交換", "更换吸嘴", "Troca de ponta"],
        K::FirstTry => ["Picks at the first attempt", "Recogidas al primer intento", "Aufnahmen beim ersten Versuch", "Prélèvements au premier essai", "Prelievi al primo tentativo", "初回のピック成功", "首次拾取成功", "Recolhas à primeira tentativa"],
        K::AfterRetries => ["after retries", "tras reintentos", "nach Wiederholungen", "après nouveaux essais", "dopo nuovi tentativi", "再試行後", "重试后", "após novas tentativas"],
        K::Lost => ["never picked", "nunca recogidas", "nie aufgenommen", "jamais prélevées", "mai prelevate", "未ピック", "从未拾取", "nunca recolhidas"],
        K::Camera => ["Camera", "Cámara", "Kamera", "Caméra", "Telecamera", "カメラ", "相机", "Câmara"],
        K::Corrected => ["Parts corrected", "Piezas corregidas", "Korrigierte Teile", "Pièces corrigées", "Parti corrette", "補正した部品", "已修正零件", "Peças corrigidas"],
        K::OverTolerance => ["Looks over the tolerance (part thrown away)", "Miradas fuera de tolerancia (pieza descartada)", "Blicke außerhalb der Toleranz (Teil verworfen)", "Regards hors tolérance (pièce rejetée)", "Osservazioni fuori tolleranza (parte scartata)", "許容差超過の確認（部品を破棄）", "超出公差的查看（零件已丢弃）", "Olhares fora da tolerância (peça descartada)"],
        K::NoReading => ["Looks without a reading (camera or vision)", "Miradas sin lectura (cámara o visión)", "Blicke ohne Messwert (Kamera oder Bildverarbeitung)", "Regards sans mesure (caméra ou vision)", "Osservazioni senza misura (telecamera o visione)", "測定値なしの確認（カメラまたは画像処理）", "无读数的查看（相机或视觉）", "Olhares sem leitura (câmara ou visão)"],
        K::MeanDistance => ["mean distance (mm)", "distancia media (mm)", "mittlerer Abstand (mm)", "distance moyenne (mm)", "distanza media (mm)", "平均距離 (mm)", "平均距离 (mm)", "distância média (mm)"],
        K::LargestDistance => ["largest distance (mm)", "distancia mayor (mm)", "größter Abstand (mm)", "plus grande distance (mm)", "distanza maggiore (mm)", "最大距離 (mm)", "最大距离 (mm)", "maior distância (mm)"],
        K::LargestTurn => ["largest turn (degrees)", "giro mayor (grados)", "größte Drehung (Grad)", "plus grande rotation (degrés)", "rotazione maggiore (gradi)", "最大回転（度）", "最大旋转（度）", "maior rotação (graus)"],
        K::ByPackage => ["By package", "Por paquete", "Nach Gehäuse", "Par boîtier", "Per package", "パッケージ別", "按封装", "Por encapsulamento"],
        K::ByFeeder => ["By feeder", "Por alimentador", "Nach Zuführung", "Par alimentateur", "Per alimentatore", "フィーダー別", "按供料器", "Por alimentador"],
        K::ByNozzle => ["By nozzle (head)", "Por boquilla (cabeza)", "Nach Düse (Kopf)", "Par buse (tête)", "Per ugello (testa)", "ノズル（ヘッド）別", "按吸嘴（吸头）", "Por bico (cabeça)"],
        K::ByTip => ["By nozzle tip", "Por punta de boquilla", "Nach Düsenspitze", "Par pointe de buse", "Per punta dell'ugello", "ノズル先端別", "按吸嘴头", "Por ponta do bico"],
        K::ByBoard => ["By board of the panel", "Por placa del panel", "Nach Platine des Nutzens", "Par carte du panneau", "Per scheda del pannello", "パネルの基板別", "按拼板中的电路板", "Por placa do painel"],
        K::ColName => ["Name", "Nombre", "Name", "Nom", "Nome", "名前", "名称", "Nome"],
        K::ColPlanned => ["Planned", "Previstas", "Geplant", "Prévues", "Previste", "計画", "计划", "Previstas"],
        K::ColPicked => ["Picked", "Recogidas", "Aufgenommen", "Prélevées", "Prelevate", "ピック済み", "已拾取", "Recolhidas"],
        K::ColPickFailed => ["Pick failures", "Fallos de recogida", "Aufnahmefehler", "Échecs de prélèvement", "Prelievi falliti", "ピック失敗", "拾取失败", "Falhas de recolha"],
        K::ColPlaceFailed => ["Place failures", "Fallos de colocación", "Bestückfehler", "Échecs de placement", "Posizionamenti falliti", "搭載失敗", "贴装失败", "Falhas de colocação"],
        K::ColFaults => ["Faults", "Fallos", "Störungen", "Pannes", "Guasti", "障害", "故障", "Falhas"],
        K::ColSwitchedOff => ["Switched off", "Apagado", "Abgeschaltet", "Éteint", "Spento", "停止中", "已关闭", "Desligado"],
        K::ColNotPlaced => ["Not placed", "Sin colocar", "Nicht bestückt", "Non placées", "Non posizionate", "未搭載", "未贴装", "Por colocar"],
        K::Hist => ["Correction of the camera (first reading of each part)", "Corrección de la cámara (primera lectura de cada pieza)", "Korrektur der Kamera (erste Messung jedes Teils)", "Correction de la caméra (première mesure de chaque pièce)", "Correzione della telecamera (prima lettura di ogni parte)", "カメラの補正（各部品の最初の測定）", "相机修正（每个零件的首次读数）", "Correção da câmara (primeira leitura de cada peça)"],
        K::HistDx => ["X offset (mm)", "Desvío X (mm)", "Abweichung X (mm)", "Écart X (mm)", "Scostamento X (mm)", "Xずれ (mm)", "X 偏差 (mm)", "Desvio X (mm)"],
        K::HistDy => ["Y offset (mm)", "Desvío Y (mm)", "Abweichung Y (mm)", "Écart Y (mm)", "Scostamento Y (mm)", "Yずれ (mm)", "Y 偏差 (mm)", "Desvio Y (mm)"],
        K::HistTurn => ["Turn (degrees)", "Giro (grados)", "Drehung (Grad)", "Rotation (degrés)", "Rotazione (gradi)", "回転（度）", "旋转（度）", "Rotação (graus)"],
        K::Mean => ["mean", "media", "Mittel", "moyenne", "media", "平均", "平均", "média"],
        K::Errors => ["Errors raised (code, times)", "Errores producidos (código, veces)", "Aufgetretene Fehler (Code, Anzahl)", "Erreurs survenues (code, fois)", "Errori avvenuti (codice, volte)", "発生したエラー（コード、回数）", "出现的错误（代码、次数）", "Erros ocorridos (código, vezes)"],
        K::Findings => ["Findings before the job", "Avisos antes del trabajo", "Hinweise vor dem Auftrag", "Avis avant le travail", "Avvisi prima del lavoro", "ジョブ前の指摘", "作业前的提示", "Avisos antes do trabalho"],
        K::PutAside => ["Parts put aside", "Piezas aplazadas", "Zurückgestellte Teile", "Pièces mises de côté", "Parti messe da parte", "保留した部品", "已搁置的零件", "Peças adiadas"],
        K::LeftOut => ["Parts left out by the person", "Piezas saltadas por la persona", "Von der Person ausgelassene Teile", "Pièces ignorées par la personne", "Parti saltate dalla persona", "オペレーターがスキップした部品", "操作员跳过的零件", "Peças saltadas pela pessoa"],
        K::DnpList => ["Parts not to be mounted", "Piezas que no se montan", "Nicht zu bestückende Teile", "Pièces à ne pas monter", "Parti da non montare", "実装しない部品", "不贴装的零件", "Peças que não se montam"],
        K::Operator => ["Actions of the person", "Acciones de la persona", "Aktionen der Person", "Actions de la personne", "Azioni della persona", "オペレーターの操作", "操作员的操作", "Ações da pessoa"],
        K::ActRetry => ["retry", "reintentar", "wiederholen", "réessayer", "riprovare", "再試行", "重试", "tentar de novo"],
        K::ActSkip => ["skip", "saltar", "auslassen", "ignorer", "saltare", "スキップ", "跳过", "saltar"],
        K::ActDefer => ["defer", "aplazar", "zurückstellen", "mettre de côté", "mettere da parte", "保留", "搁置", "adiar"],
        K::ActSwitchOff => ["switch off the feeder", "apagar el alimentador", "Zuführung abschalten", "éteindre l'alimentateur", "spegnere l'alimentatore", "フィーダーを停止", "关闭供料器", "desligar o alimentador"],
        K::NoneWord => ["none", "ninguno", "keine", "aucun", "nessuno", "なし", "无", "nenhum"],
        K::Simulated => ["Simulated: nothing here was measured on a machine.", "Simulado: nada de esto se midió en una máquina.", "Simuliert: nichts davon wurde an einer Maschine gemessen.", "Simulé : rien de ceci n'a été mesuré sur une machine.", "Simulato: nulla di questo è stato misurato su una macchina.", "シミュレーション: 実機での測定値ではありません。", "模拟结果：这些数据均非在真实机器上测得。", "Simulado: nada disto foi medido numa máquina."],
        K::Range => ["Range", "Rango", "Bereich", "Plage", "Intervallo", "範囲", "范围", "Intervalo"],
        K::Count => ["Count", "Cantidad", "Anzahl", "Nombre", "Numero", "件数", "数量", "Quantidade"],
    };
    row[lang as usize]
}

/// The name of the state of a job in a language.
fn phase_word(phase: Phase, lang: Lang) -> &'static str {
    word(
        match phase {
            Phase::Ready => K::PhaseReady,
            Phase::Paused => K::PhasePaused,
            Phase::Stopped => K::PhaseStopped,
            Phase::Finished => K::PhaseFinished,
        },
        lang,
    )
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------
// The sections, drawn as text and as HTML from the same description.

struct Table {
    head: Vec<String>,
    rows: Vec<Vec<String>>,
    /// A bar for each row, from 0 to 1.
    bars: Vec<f64>,
}

struct Section {
    title: String,
    pairs: Vec<(String, String)>,
    tables: Vec<(String, Table)>,
    lines: Vec<String>,
}

fn section(title: &str) -> Section {
    Section {
        title: title.to_string(),
        pairs: Vec::new(),
        tables: Vec::new(),
        lines: Vec::new(),
    }
}

fn plain(head: &[&str], rows: Vec<Vec<String>>) -> Table {
    Table {
        head: head.iter().map(|h| h.to_string()).collect(),
        rows,
        bars: Vec::new(),
    }
}

fn group_table(groups: &[Group], lang: Lang) -> Table {
    plain(
        &[
            word(K::ColName, lang),
            word(K::ColPlanned, lang),
            word(K::Placed, lang),
            word(K::Deferred, lang),
            word(K::Skipped, lang),
            word(K::ColNotPlaced, lang),
        ],
        groups
            .iter()
            .map(|g| {
                vec![
                    g.name.clone(),
                    g.planned.to_string(),
                    g.placed.to_string(),
                    g.deferred.to_string(),
                    g.skipped.to_string(),
                    g.not_placed().to_string(),
                ]
            })
            .collect(),
    )
}

fn percent(part: f64, whole: f64) -> String {
    if whole > 0.0 {
        format!("{:.1} %", part * 100.0 / whole)
    } else {
        "0.0 %".to_string()
    }
}

fn hist_title(name: &str, lang: Lang) -> &'static str {
    match name {
        "dx" => word(K::HistDx, lang),
        "dy" => word(K::HistDy, lang),
        _ => word(K::HistTurn, lang),
    }
}

impl WorkReport {
    fn sections(&self, lang: Lang) -> Vec<Section> {
        let mut out = Vec::new();
        let mut head = section(word(K::Title, lang));
        head.pairs.push((
            word(K::State, lang).to_string(),
            phase_word(self.phase, lang).to_string(),
        ));
        out.push(head);

        let mut parts = section(word(K::Parts, lang));
        for (k, n) in [
            (K::InPlan, self.planned),
            (K::Placed, self.placed),
            (K::Deferred, self.deferred),
            (K::Skipped, self.skipped),
            (K::Pending, self.pending),
            (K::Dnp, self.dnp.len()),
        ] {
            parts.pairs.push((word(k, lang).to_string(), n.to_string()));
        }
        out.push(parts);

        let mut time = section(word(K::Time, lang));
        time.pairs.push((
            word(K::Total, lang).to_string(),
            format!("{:.1} s", self.seconds_total),
        ));
        time.pairs.push((
            word(K::PerPart, lang).to_string(),
            format!("{:.2} s", self.seconds_per_part),
        ));
        time.pairs.push((
            word(K::PerHour, lang).to_string(),
            format!("{:.0}", self.parts_per_hour),
        ));
        let sum: f64 = self.phases().iter().sum();
        let names = [
            K::PhasePick,
            K::PhaseAlign,
            K::PhasePlace,
            K::PhaseTravel,
            K::PhaseTip,
        ];
        let mut rows = Vec::new();
        let mut bars = Vec::new();
        for (k, s) in names.iter().zip(self.phases()) {
            rows.push(vec![
                word(*k, lang).to_string(),
                format!("{s:.1}"),
                percent(s, sum),
            ]);
            bars.push(if sum > 0.0 { s / sum } else { 0.0 });
        }
        let mut phases = plain(
            &[
                word(K::Phase, lang),
                word(K::Seconds, lang),
                word(K::Share, lang),
            ],
            rows,
        );
        phases.bars = bars;
        time.tables.push((String::new(), phases));
        out.push(time);

        let mut picks = section(word(K::ColPicked, lang));
        let tried = self.first_try + self.retried + self.lost;
        picks.pairs.push((
            word(K::FirstTry, lang).to_string(),
            format!(
                "{} / {} ({:.1} %)",
                self.first_try,
                tried,
                self.first_try_ratio() * 100.0
            ),
        ));
        picks.pairs.push((
            word(K::AfterRetries, lang).to_string(),
            self.retried.to_string(),
        ));
        picks
            .pairs
            .push((word(K::Lost, lang).to_string(), self.lost.to_string()));
        out.push(picks);

        if self.align_on {
            let mut cam = section(word(K::Camera, lang));
            for (k, v) in [
                (K::Corrected, self.corrected.to_string()),
                (K::OverTolerance, self.over_tolerance.to_string()),
                (K::NoReading, self.no_reading.to_string()),
                (K::MeanDistance, format!("{:.3}", self.mean_mm)),
                (K::LargestDistance, format!("{:.3}", self.max_mm)),
                (K::LargestTurn, format!("{:.3}", self.max_deg)),
            ] {
                cam.pairs.push((word(k, lang).to_string(), v));
            }
            out.push(cam);
            let mut hist = section(word(K::Hist, lang));
            for h in &self.histograms {
                let biggest = h.counts.iter().copied().max().unwrap_or(0);
                let mut rows = Vec::new();
                let mut bars = Vec::new();
                for (i, n) in h.counts.iter().enumerate() {
                    let (a, b) = h.bin(i);
                    rows.push(vec![format!("{a:.3} .. {b:.3}"), n.to_string()]);
                    bars.push(if biggest > 0 {
                        f64::from(*n) / f64::from(biggest)
                    } else {
                        0.0
                    });
                }
                let mut table = plain(&[word(K::Range, lang), word(K::Count, lang)], rows);
                table.bars = bars;
                hist.tables.push((
                    format!(
                        "{} - {}: {:.3}",
                        hist_title(h.name, lang),
                        word(K::Mean, lang),
                        h.mean
                    ),
                    table,
                ));
            }
            out.push(hist);
        }

        let mut by_package = section(word(K::ByPackage, lang));
        by_package.tables.push((
            String::new(),
            plain(
                &[
                    word(K::ColName, lang),
                    word(K::ColPicked, lang),
                    word(K::ColPickFailed, lang),
                    word(K::Placed, lang),
                    word(K::ColPlaceFailed, lang),
                ],
                self.packages
                    .iter()
                    .map(|k| {
                        vec![
                            k.package.clone(),
                            k.picked.to_string(),
                            k.pick_failures.to_string(),
                            k.placed.to_string(),
                            k.place_failures.to_string(),
                        ]
                    })
                    .collect(),
            ),
        ));
        out.push(by_package);

        let mut by_feeder = section(word(K::ByFeeder, lang));
        by_feeder.tables.push((
            String::new(),
            plain(
                &[
                    word(K::ColName, lang),
                    word(K::ColPicked, lang),
                    word(K::ColPickFailed, lang),
                    word(K::ColFaults, lang),
                    word(K::ColSwitchedOff, lang),
                ],
                self.feeders
                    .iter()
                    .map(|f| {
                        vec![
                            f.id.clone(),
                            f.picked.to_string(),
                            f.pick_failures.to_string(),
                            f.faults.to_string(),
                            if f.disabled { "x" } else { "-" }.to_string(),
                        ]
                    })
                    .collect(),
            ),
        ));
        out.push(by_feeder);

        for (k, groups) in [
            (K::ByNozzle, &self.nozzles),
            (K::ByTip, &self.tips),
            (K::ByBoard, &self.boards),
        ] {
            let mut s = section(word(k, lang));
            s.tables.push((String::new(), group_table(groups, lang)));
            out.push(s);
        }

        let mut errors = section(word(K::Errors, lang));
        if self.codes.is_empty() {
            errors.lines.push(word(K::NoneWord, lang).to_string());
        }
        for (code, times) in &self.codes {
            errors.lines.push(format!(
                "{}  {}  x {times}",
                code.label(),
                code.message(lang)
            ));
        }
        out.push(errors);

        let mut findings = section(word(K::Findings, lang));
        if self.checks.is_empty() {
            findings.lines.push(word(K::NoneWord, lang).to_string());
        }
        for c in &self.checks {
            findings.lines.push(format!(
                "{} {}: {}",
                c.severity.to_uppercase(),
                c.source,
                c.message
            ));
        }
        out.push(findings);

        for (k, list) in [(K::PutAside, &self.put_aside), (K::LeftOut, &self.left_out)] {
            if !list.is_empty() {
                let mut s = section(word(k, lang));
                for (r, why) in list {
                    s.lines.push(format!("{r}: {why}"));
                }
                out.push(s);
            }
        }
        if !self.dnp.is_empty() {
            let mut s = section(word(K::DnpList, lang));
            s.lines.push(self.dnp.join(", "));
            out.push(s);
        }
        if self.actions.iter().any(|n| *n > 0) {
            let mut s = section(word(K::Operator, lang));
            for (k, n) in [
                (K::ActRetry, self.actions[0]),
                (K::ActSkip, self.actions[1]),
                (K::ActDefer, self.actions[2]),
                (K::ActSwitchOff, self.actions[3]),
            ] {
                s.pairs.push((word(k, lang).to_string(), n.to_string()));
            }
            out.push(s);
        }
        let mut note = section("");
        note.lines.push(word(K::Simulated, lang).to_string());
        out.push(note);
        out
    }

    /// The report as a text that can be copied: sections with their lines, and the tables with their columns aligned.
    pub fn text(&self, lang: Lang) -> String {
        let mut out = String::new();
        for (n, s) in self.sections(lang).iter().enumerate() {
            if !s.title.is_empty() {
                if n > 0 {
                    out.push('\n');
                }
                out.push_str(&s.title);
                out.push('\n');
                out.push_str(&"-".repeat(s.title.chars().count().min(60)));
                out.push('\n');
            }
            for (k, v) in &s.pairs {
                out.push_str(&format!("{k}: {v}\n"));
            }
            for line in &s.lines {
                out.push_str(&format!("{line}\n"));
            }
            for (title, table) in &s.tables {
                if !title.is_empty() {
                    out.push_str(&format!("{title}\n"));
                }
                out.push_str(&text_table(table));
            }
        }
        out
    }

    /// The report as a page of HTML with everything in it: the style is inside, there is no script, no picture and no link, and every text is escaped.
    pub fn html(&self, lang: Lang) -> String {
        let title = word(K::Title, lang);
        let mut out = String::new();
        out.push_str("<!DOCTYPE html>\n");
        out.push_str(&format!("<html lang=\"{}\">\n<head>\n", lang.code()));
        out.push_str("<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        out.push_str(&format!("<title>{}</title>\n", escape(title)));
        out.push_str(STYLE);
        out.push_str("</head>\n<body>\n");
        for (n, s) in self.sections(lang).iter().enumerate() {
            if n == 0 {
                out.push_str(&format!("<h1>{}</h1>\n", escape(&s.title)));
            } else if !s.title.is_empty() {
                out.push_str(&format!("<h2>{}</h2>\n", escape(&s.title)));
            }
            if !s.pairs.is_empty() {
                out.push_str("<table class=\"pairs\">\n");
                for (k, v) in &s.pairs {
                    out.push_str(&format!(
                        "<tr><th scope=\"row\">{}</th><td>{}</td></tr>\n",
                        escape(k),
                        escape(v)
                    ));
                }
                out.push_str("</table>\n");
            }
            if !s.lines.is_empty() {
                out.push_str("<ul>\n");
                for line in &s.lines {
                    out.push_str(&format!("<li>{}</li>\n", escape(line)));
                }
                out.push_str("</ul>\n");
            }
            for (caption, table) in &s.tables {
                if !caption.is_empty() {
                    out.push_str(&format!("<h3>{}</h3>\n", escape(caption)));
                }
                out.push_str(&html_table(table));
            }
        }
        out.push_str("</body>\n</html>\n");
        out
    }

    /// The report as JSON, with English names: it is added to the statistics of the job as `work`.
    pub fn json(&self) -> Value {
        let group = |g: &Group| json!({"name": g.name, "planned": g.planned, "placed": g.placed, "deferred": g.deferred, "skipped": g.skipped, "not_placed": g.not_placed()});
        let mut doc = json!({
            "state": match self.phase {
                Phase::Ready => "ready",
                Phase::Paused => "paused",
                Phase::Stopped => "stopped",
                Phase::Finished => "finished",
            },
            "parts": {
                "planned": self.planned, "placed": self.placed, "deferred": self.deferred,
                "skipped": self.skipped, "pending": self.pending, "dnp": self.dnp.len()
            },
            "seconds": {
                "total": self.seconds_total, "pick": self.seconds_pick, "camera": self.seconds_align,
                "place": self.seconds_place, "travel": self.seconds_travel, "tip_change": self.seconds_tip
            },
            "first_try": {
                "picked_first": self.first_try, "after_retries": self.retried,
                "never_picked": self.lost, "ratio": self.first_try_ratio()
            },
            "nozzles": self.nozzles.iter().map(group).collect::<Vec<_>>(),
            "tips": self.tips.iter().map(group).collect::<Vec<_>>(),
            "boards": self.boards.iter().map(group).collect::<Vec<_>>(),
            "errors": self.codes.iter().map(|(c, n)| json!({"code": c.label(), "times": n})).collect::<Vec<_>>(),
            "actions": {
                "retry": self.actions[0], "skip": self.actions[1],
                "defer": self.actions[2], "switch_off_feeder": self.actions[3]
            },
            "dnp": self.dnp,
            "findings": self.checks.iter().map(|c| json!({"severity": c.severity, "source": c.source, "message": c.message})).collect::<Vec<_>>(),
            "put_aside": self.put_aside.iter().map(|(r, why)| json!({"reference": r, "reason": why})).collect::<Vec<_>>(),
            "left_out": self.left_out.iter().map(|(r, why)| json!({"reference": r, "reason": why})).collect::<Vec<_>>()
        });
        if self.align_on {
            doc["camera"] = json!({
                "corrected": self.corrected, "over_tolerance": self.over_tolerance,
                "no_reading": self.no_reading, "mean_mm": self.mean_mm,
                "max_mm": self.max_mm, "max_deg": self.max_deg
            });
            let mut hist = serde_json::Map::new();
            for h in &self.histograms {
                hist.insert(
                    h.name.to_string(),
                    json!({"unit": h.unit, "lo": h.lo, "hi": h.hi, "mean": h.mean, "counts": h.counts}),
                );
            }
            doc["histograms"] = Value::Object(hist);
        }
        doc
    }

    /// The sections that come after the rows of `Job::stats_csv` in the CSV of the statistics. The header does not change: each row is `section,name,value` and the columns after
    /// the value (`picked`, `pick_failures`, `placed`, `place_failures`, `faults`, `disabled`) are used by the groups as the header of the file says: for `nozzle`, `tip` and `board`
    /// the value is the parts planned, `placed` the parts placed and `faults` the parts not placed.
    pub fn csv_sections(&self) -> String {
        let mut out = String::new();
        let mut row = |section: &str, name: &str, value: String| {
            out.push_str(&format!("{section},{},{value},,,,,,\n", csv_field(name)));
        };
        row(
            "work",
            "state",
            self.json()["state"].as_str().unwrap_or("").to_string(),
        );
        row("work", "planned", self.planned.to_string());
        row("work", "skipped", self.skipped.to_string());
        row("work", "pending", self.pending.to_string());
        row("work", "dnp", self.dnp.len().to_string());
        row("work", "first_try", self.first_try.to_string());
        row("work", "after_retries", self.retried.to_string());
        row("work", "never_picked", self.lost.to_string());
        row(
            "work",
            "first_try_ratio",
            format!("{:.3}", self.first_try_ratio()),
        );
        row("time", "total", format!("{:.3}", self.seconds_total));
        row("time", "pick", format!("{:.3}", self.seconds_pick));
        row("time", "camera", format!("{:.3}", self.seconds_align));
        row("time", "place", format!("{:.3}", self.seconds_place));
        row("time", "travel", format!("{:.3}", self.seconds_travel));
        row("time", "tip_change", format!("{:.3}", self.seconds_tip));
        for (k, n) in ["retry", "skip", "defer", "switch_off_feeder"]
            .iter()
            .zip(self.actions)
        {
            if n > 0 {
                row("action", k, n.to_string());
            }
        }
        for (code, times) in &self.codes {
            row("error", &code.label(), times.to_string());
        }
        for h in &self.histograms {
            for (i, n) in h.counts.iter().enumerate() {
                let (a, b) = h.bin(i);
                row(
                    "histogram",
                    &format!("{}_{}:{:.3}..{:.3}", h.name, h.unit, a, b),
                    n.to_string(),
                );
            }
        }
        for (section, groups) in [
            ("nozzle", &self.nozzles),
            ("tip", &self.tips),
            ("board", &self.boards),
        ] {
            for g in groups.iter() {
                out.push_str(&format!(
                    "{section},{},{},,,{},,{},\n",
                    csv_field(&g.name),
                    g.planned,
                    g.placed,
                    g.not_placed()
                ));
            }
        }
        for r in &self.dnp {
            out.push_str(&format!("dnp,{},,,,,,,\n", csv_field(r)));
        }
        for c in &self.checks {
            out.push_str(&format!(
                "finding,{},{},,,,,,\n",
                csv_field(&format!("{}:{}", c.severity, c.source)),
                csv_field(&c.message)
            ));
        }
        for (r, why) in &self.left_out {
            out.push_str(&format!(
                "skipped,{},{},,,,,,\n",
                csv_field(r),
                csv_field(why)
            ));
        }
        out
    }
}

fn csv_field(text: &str) -> String {
    crate::parts::csv_field(text)
}

/// The rows of `Job::stats_csv` and, after the rows that were always there, the sections of the report. The section of the align step stays the last one, as it always was.
pub fn csv(job: &Job, input: &ReportInput) -> String {
    let old = job.stats_csv();
    let sections = build(job, input).csv_sections();
    match old.find(
        "
align,",
    ) {
        Some(at) => format!("{}{}{}", &old[..=at], sections, &old[at + 1..]),
        None => format!("{old}{sections}"),
    }
}

/// The statistics of the job as JSON with the report added as `work`.
pub fn json(job: &Job, input: &ReportInput) -> String {
    let mut doc: Value = serde_json::from_str(&job.stats_json()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut doc {
        map.insert("work".to_string(), build(job, input).json());
    }
    serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
}

/// Text with the characters that mean something in HTML written as their entities.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "<style>\nbody{font-family:sans-serif;margin:2em auto;max-width:60em;padding:0 1em;color:#1b1b1b;background:#fff}\nh1{font-size:1.6em}h2{font-size:1.2em;margin-top:1.6em;border-bottom:1px solid #999}h3{font-size:1em}\ntable{border-collapse:collapse;margin:.4em 0}th,td{border:1px solid #bbb;padding:.2em .6em;text-align:left}\ntable.pairs th{font-weight:normal;background:#f2f2f2}\ntd.bar{min-width:10em;border-left:0}td.bar div{height:.8em;background:#c44a1c}\n@media (prefers-color-scheme: dark){body{color:#e8e8e8;background:#161616}th,td{border-color:#555}table.pairs th{background:#242424}}\n</style>\n";

fn html_table(t: &Table) -> String {
    let mut out = String::from("<table>\n<tr>");
    for h in &t.head {
        out.push_str(&format!("<th>{}</th>", escape(h)));
    }
    if !t.bars.is_empty() {
        out.push_str("<th></th>");
    }
    out.push_str("</tr>\n");
    for (i, r) in t.rows.iter().enumerate() {
        out.push_str("<tr>");
        for c in r {
            out.push_str(&format!("<td>{}</td>", escape(c)));
        }
        if let Some(b) = t.bars.get(i) {
            let width = (b.clamp(0.0, 1.0) * 100.0).round() as u32;
            out.push_str(&format!(
                "<td class=\"bar\"><div style=\"width:{width}%\"></div></td>"
            ));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

fn text_table(t: &Table) -> String {
    let cols = t.head.len();
    let mut widths: Vec<usize> = t.head.iter().map(|h| h.chars().count()).collect();
    for r in &t.rows {
        for (i, c) in r.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let line = |cells: &[String], bar: Option<f64>| {
        let mut l = String::from("  ");
        for (i, c) in cells.iter().enumerate().take(cols) {
            l.push_str(c);
            if i + 1 < cols || bar.is_some() {
                l.push_str(&" ".repeat(widths[i] - c.chars().count() + 2));
            }
        }
        if let Some(b) = bar {
            l.push_str(&"#".repeat((b.clamp(0.0, 1.0) * 24.0).round() as usize));
        }
        l.trim_end().to_string() + "\n"
    };
    let mut out = line(&t.head, None);
    for (i, r) in t.rows.iter().enumerate() {
        out.push_str(&line(r, t.bars.get(i).copied()));
    }
    out
}

/// Writes `text` to a new file: a file that exists is never written over (the error is `PHOENIX-PNP-040`).
pub fn write_new(path: &str, text: &str) -> Result<(), PnpError> {
    if Path::new(path).exists() {
        return Err(PnpError::new(Code::ReportExists, path));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                PnpError::new(Code::ReportExists, path)
            } else {
                PnpError::new(Code::DriverError, format!("{path}: {e}"))
            }
        })?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|e| PnpError::new(Code::DriverError, format!("{path}: {e}")))
}

/// The names of the actions of the person, for the interface.
pub fn action_names() -> [&'static str; 4] {
    [
        Action::Retry.name(),
        Action::Skip.name(),
        Action::Defer.name(),
        Action::SwitchOffFeeder.name(),
    ]
}

/// Whether every word of the report has a text in every language (for the tests).
pub fn words_complete() -> bool {
    use K::*;
    let all = [
        Title,
        State,
        PhaseReady,
        PhasePaused,
        PhaseStopped,
        PhaseFinished,
        Parts,
        InPlan,
        Placed,
        Deferred,
        Skipped,
        Pending,
        Dnp,
        Time,
        Total,
        PerPart,
        PerHour,
        Phase,
        Seconds,
        Share,
        PhasePick,
        PhaseAlign,
        PhasePlace,
        PhaseTravel,
        PhaseTip,
        FirstTry,
        AfterRetries,
        Lost,
        Camera,
        Corrected,
        OverTolerance,
        NoReading,
        MeanDistance,
        LargestDistance,
        LargestTurn,
        ByPackage,
        ByFeeder,
        ByNozzle,
        ByTip,
        ByBoard,
        ColName,
        ColPlanned,
        ColPicked,
        ColPickFailed,
        ColPlaceFailed,
        ColFaults,
        ColSwitchedOff,
        ColNotPlaced,
        Hist,
        HistDx,
        HistDy,
        HistTurn,
        Mean,
        Errors,
        Findings,
        PutAside,
        LeftOut,
        DnpList,
        Operator,
        ActRetry,
        ActSkip,
        ActDefer,
        ActSwitchOff,
        NoneWord,
        Simulated,
        Range,
        Count,
    ];
    all.iter()
        .all(|k| Lang::ALL.iter().all(|l| !word(*k, *l).is_empty()))
}
