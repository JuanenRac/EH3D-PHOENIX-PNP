// EH3D-PHOENIX-PNP - the lines of a job and their fingerprint.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The lines that a driver profile writes for a plan, from connecting to switching the motors off, can be looked at before they are run, and the fingerprint of those lines
//! (SHA-256 of the lines, each one ended by a line break) says whether what is run is what was seen. The same plan and the same profile always give the same lines and the same
//! fingerprint; a line that changes, is added or goes, or a place that moves by a thousandth of a millimetre, gives another one. This is a local check of the lines of this program and nothing
//! authorises with it: the server makes its own hash of the request it holds, and that one (see `server`) is what authorises a job there. Nothing is sent anywhere; the lines are text.

use crate::config::MachineConfig;
use crate::error::Lang;
use crate::feeders::{assign_side, Feeder};
use crate::feeding::{self, FeederKind};
use crate::gcodedriver::DriverProfile;
use crate::marks;
use crate::msg8::say;
use crate::parts::{self, Side};
use crate::plan::{plan, Placement, Plan, Settings};
use crate::sha256::sha256_hex;

/// How a job of the driver is planned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JobOptions {
    pub side: Side,
    /// For the bottom side: the width of the board.
    pub board_width_mm: Option<f64>,
    /// For the bottom side: where the left edge of the board is on the X axis.
    pub origin_x_mm: f64,
    /// Place what can be placed when the feeders do not cover the board.
    pub partial: bool,
}

/// The plan of a job of the driver.
#[derive(Debug, Clone, PartialEq)]
pub struct JobPlan {
    pub plan: Plan,
    /// The references of the parts that are marks of the board and are not placed.
    pub marks: Vec<String>,
}

/// Plans the job that the driver sends, from the texts of the table of parts and of the table of feeders. As the plan of `schedule` does, the marks of the board are left out, the
/// rotation rules of the configuration (`rotation`) are applied, and the part is taken at the angle it lies at in its feeder (the column `rotation` of the feeders). The table of
/// feeders may have every column of the table of `schedule`; a feeder that is switched off is not used and a part of a tray or of a strip is refused, because this plan picks every
/// part of a feeder at one place.
pub fn plan_job(
    parts_text: &str,
    feeders_text: &str,
    profile: &DriverProfile,
    cfg: &MachineConfig,
    options: JobOptions,
    lang: Lang,
) -> Result<JobPlan, String> {
    let mut board = parts::parse(parts_text)?;
    let taken = marks::take_fiducial_parts(&mut board, &cfg.fiducial_words, &cfg.fiducial_refs);
    cfg.rotation_table()
        .map_err(|e| e.localized(lang))?
        .apply(&mut board);
    let mut specs: Vec<feeding::FeederSpec> = feeding::from_csv_with(feeders_text, cfg.tray_fill)
        .map_err(|e| e.localized(lang))?
        .into_iter()
        .filter(|f| f.enabled)
        .collect();
    // the higher priority first, and the order of the table among equals
    specs.sort_by_key(|f| std::cmp::Reverse(f.priority));
    let feeders: Vec<Feeder> = specs.iter().map(|f| f.base.clone()).collect();
    let assigned = assign_side(&board, &feeders, Some(options.side));
    if let Some(spec) = assigned
        .assignments
        .iter()
        .filter_map(|a| specs.get(a.feeder))
        .find(|f| {
            !matches!(
                f.kind,
                FeederKind::Tape | FeederKind::Tube | FeederKind::Bulk
            )
        })
    {
        return Err(say(
            lang,
            &[
                "the feeder {id} is a {kind}: the driver picks every part of a feeder at one place, so it takes tape, tube and bulk feeders only",
                "el alimentador {id} es de tipo {kind}: el controlador toma todas las piezas de un alimentador en un mismo punto, así que solo admite cinta, tubo y granel",
                "der Feeder {id} ist vom Typ {kind}: der Treiber nimmt jedes Teil eines Feeders an derselben Stelle auf, daher nur Gurt, Stange und Schüttgut",
                "le chargeur {id} est de type {kind} : le pilote prend chaque composant d'un chargeur au même endroit, il n'accepte donc que bande, tube et vrac",
                "l'alimentatore {id} è di tipo {kind}: il driver preleva ogni componente di un alimentatore nello stesso punto, quindi accetta solo nastro, tubo e sfuso",
                "フィーダ {id} は {kind} 型です。ドライバはフィーダの部品をすべて同じ位置で取るため、テープ・スティック・バラのみ扱えます",
                "供料器 {id} 为 {kind} 类型：驱动程序在同一位置拾取供料器的所有元件，因此只支持编带、管装和散料",
                "o alimentador {id} é do tipo {kind}: o controlador apanha todas as peças de um alimentador no mesmo ponto, por isso só aceita fita, tubo e granel",
            ],
            &[
                ("id", spec.base.id.clone()),
                ("kind", spec.kind.name().to_string()),
            ],
        ));
    }
    if (!assigned.without_feeder.is_empty() || !assigned.short.is_empty()) && !options.partial {
        return Err(say(
            lang,
            &[
                "the feeders do not cover the board (use --partial to place only what can be placed)",
                "los alimentadores no cubren la placa (use --partial para colocar solo lo que se pueda)",
                "die Feeder decken die Platine nicht ab (mit --partial wird nur bestückt, was möglich ist)",
                "les chargeurs ne couvrent pas la carte (--partial pour ne poser que ce qui peut l'être)",
                "gli alimentatori non coprono la scheda (usare --partial per posare solo il possibile)",
                "フィーダが基板の部品を満たしていません（可能なものだけ実装するには --partial）",
                "供料器无法覆盖整块板（用 --partial 只贴装能贴装的部分）",
                "os alimentadores não cobrem a placa (use --partial para colocar só o que for possível)",
            ],
            &[],
        ));
    }
    let settings = Settings {
        nozzles: profile.nozzles,
        side: options.side,
        board_width_mm: options.board_width_mm,
        origin_x_mm: options.origin_x_mm,
        ..Settings::default()
    };
    let mut planned = plan(&board, &feeders, &assigned, &settings)?;
    for p in &mut planned.placements {
        if let Some(spec) = specs.iter().find(|f| f.base.id == p.feeder) {
            p.nozzle_pick_deg = spec.tape_rotation;
        }
    }
    Ok(JobPlan {
        plan: planned,
        marks: taken.into_iter().map(|p| p.reference).collect(),
    })
}

/// The lines of the whole job in order: connect, absolute mode, homing, each placement, switch off. A step the profile cannot write is an error and stops it here.
pub fn script(profile: &DriverProfile, placements: &[Placement]) -> Result<Vec<String>, String> {
    script_in(profile, placements, Lang::En)
}

/// `script`, with the reason of a step that cannot be written said in `lang`.
pub fn script_in(
    profile: &DriverProfile,
    placements: &[Placement],
    lang: Lang,
) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(profile.connect.clone());
    out.push(profile.absolute_mode.clone());
    out.extend(profile.home_lines());
    for p in placements {
        let ops = profile
            .placement_ops_checked(p)
            .map_err(|e| e.localized(lang))?;
        for op in ops {
            out.extend(profile.lines(&op)?);
        }
    }
    out.extend(profile.disable.clone());
    Ok(out)
}

/// The lines as one text, each ended by a line break.
pub fn text(lines: &[String]) -> String {
    let mut t = lines.join("\n");
    t.push('\n');
    t
}

/// The fingerprint of the lines: 64 letters of lower case hexadecimal.
pub fn fingerprint(lines: &[String]) -> String {
    sha256_hex(text(lines).as_bytes())
}

/// Whether a text the person gives is a fingerprint: 64 hexadecimal digits, in either case.
pub fn is_fingerprint(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether the fingerprint that was given (in either case) is the one of the lines.
pub fn matches(lines: &[String], given: &str) -> bool {
    is_fingerprint(given.trim())
        && crate::sha256::same(&fingerprint(lines), &given.trim().to_ascii_lowercase())
}
