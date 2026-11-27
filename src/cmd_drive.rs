// EH3D-PHOENIX-PNP - the commands of the G-code driver: driver-check, driver-test, driver-run, safety-replay, homing-repeat and bringup.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints. The only controller they talk to is the simulated one: no port is opened and nothing is moved. A file that exists is never written
// over.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_flow::{deliver, whole};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::driver::{Failure, Session};
use eh3d_phoenix_pnp::drivertest;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gcodedriver::{DriverProfile, HomingMode, Op, Transport, MAX_PROFILE_BYTES};
use eh3d_phoenix_pnp::gcodesim::{Faults, GcodeSim, Link, MoonrakerLink, SimLink, SimMoonraker};
use eh3d_phoenix_pnp::msg8::{num as show, pick, say};
use eh3d_phoenix_pnp::parts::Side;
use eh3d_phoenix_pnp::plan::Placement;
use eh3d_phoenix_pnp::safety::{replay, replay_text, Level, Rules};
use eh3d_phoenix_pnp::toolpath::{self, JobOptions};

pub fn load_profile(path: &str, lang: Lang) -> Result<DriverProfile, String> {
    let text = read_text(path, MAX_PROFILE_BYTES as u64)?;
    DriverProfile::parse(&text).map_err(|e| format!("{path}: {}", e.localized(lang)))
}

/// The faults of the options `--fault`, each `error@N[:TEXT]`, `silent@N`, `halt@N`, `busy@N`, `offset@MM`, `slow@F`, `jitter@MM` or `strict`.
fn faults_of(args: &mut Vec<String>) -> Result<Faults, String> {
    let mut f = Faults::default();
    while let Some(v) = flag(args, "--fault")? {
        let (name, rest) = v.split_once('@').unwrap_or((v.as_str(), ""));
        let count = |what: &str| -> Result<usize, String> {
            whole(rest.split(':').next().unwrap_or(""), what, 1.0, 1e9).map(|n| n as usize)
        };
        match name {
            "error" => {
                let text = rest.split_once(':').map_or("simulated fault", |(_, t)| t);
                f.error_at = Some((count("--fault error@")?, text.to_string()));
            }
            "silent" => f.silent_at = Some(count("--fault silent@")?),
            "halt" => f.halt_at = Some(count("--fault halt@")?),
            "busy" => f.busy_lines = whole(rest, "--fault busy@", 0.0, 50.0)? as u32,
            "offset" => f.report_offset = num(rest, "--fault offset@")?,
            "slow" => f.slow_factor = num(rest, "--fault slow@")?,
            "jitter" => f.homing_jitter = num(rest, "--fault jitter@")?,
            "strict" => f.strict_unknown = true,
            other => {
                return Err(format!(
                    "--fault {other}: it is error@N, silent@N, halt@N, busy@N, offset@MM, slow@F, jitter@MM or strict"
                ))
            }
        }
    }
    if f.slow_factor != 0.0 && !(0.01..=100.0).contains(&f.slow_factor) {
        return Err("--fault slow@ must be from 0.01 to 100".into());
    }
    if !(0.0..=100.0).contains(&f.homing_jitter) || !(-100.0..=100.0).contains(&f.report_offset) {
        return Err("--fault jitter@ and offset@ are in mm and small".into());
    }
    Ok(f)
}

fn seed_of(args: &mut Vec<String>) -> Result<u64, String> {
    match flag(args, "--seed")? {
        Some(v) => Ok(whole(&v, "--seed", 0.0, 1e12)?),
        None => Ok(1),
    }
}

/// `wiring-sheet PROFILE [--out F] [--svg F]`: what is to be connected to which pin of the controller (the axes, the pump, the blow-off valve and any other valve), read from a driver
/// profile, and a drawing of the pneumatics. `--out` writes the table as a CSV and `--svg` the drawing, never over a file that exists. The exit code is 2 when a pin cannot be read from the
/// lines of an actuator, when the profile names no pump or no blow-off valve, or when two actuators share a pin.
pub fn wiring_sheet(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    let svg = flag(&mut args, "--svg")?;
    finish(&args, 1)?;
    for o in [&out, &svg].into_iter().flatten() {
        if std::path::Path::new(o).exists() {
            return Err(format!("{o}: exists and is never written over"));
        }
    }
    let path = args.first().ok_or("wiring-sheet needs a profile")?;
    let profile = load_profile(path, lang)?;
    let sheet = eh3d_phoenix_pnp::wiring::build(&profile);
    print!("{}", sheet.text());
    if let Some(o) = &out {
        eh3d_phoenix_pnp::report::write_new(o, &sheet.csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    if let Some(o) = &svg {
        eh3d_phoenix_pnp::report::write_new(o, &sheet.svg()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    match sheet.problems.len() {
        0 => Ok(()),
        n => fail(format!("{n} problems in the wiring of the profile")),
    }
}

/// `driver-check PROFILE`: reads a driver profile and says what it holds.
pub fn driver_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("driver-check needs a profile")?;
    let text = read_text(path, MAX_PROFILE_BYTES as u64)?;
    let p = DriverProfile::parse(&text)
        .map_err(|e| crate::cmd_nodes::not_passed(format!("{path}: {}", e.localized(lang))))?;
    let rows: [&str; 8] = [
        "profile {path}: {dialect} over {transport}, axes {x}{y}{z} and {rot}, {n} actuators (pump: {pump})",
        "perfil {path}: {dialect} por {transport}, ejes {x}{y}{z} y {rot}, {n} actuadores (bomba: {pump})",
        "Profil {path}: {dialect} über {transport}, Achsen {x}{y}{z} und {rot}, {n} Aktoren (Pumpe: {pump})",
        "profil {path} : {dialect} par {transport}, axes {x}{y}{z} et {rot}, {n} actionneurs (pompe : {pump})",
        "profilo {path}: {dialect} su {transport}, assi {x}{y}{z} e {rot}, {n} attuatori (pompa: {pump})",
        "プロファイル {path}：{transport} 経由の {dialect}、軸 {x}{y}{z} と {rot}、アクチュエータ {n} 個（ポンプ：{pump}）",
        "配置档 {path}：{dialect}，经 {transport}，轴 {x}{y}{z} 和 {rot}，{n} 个执行器（泵：{pump}）",
        "perfil {path}: {dialect} por {transport}, eixos {x}{y}{z} e {rot}, {n} atuadores (bomba: {pump})",
    ];
    println!(
        "{}",
        say(
            lang,
            &rows,
            &[
                ("path", path.clone()),
                ("dialect", p.dialect.name().into()),
                ("transport", p.transport.name().into()),
                ("x", p.axis_x.clone()),
                ("y", p.axis_y.clone()),
                ("z", p.axis_z.clone()),
                (
                    "rot",
                    if p.axis_rot.is_empty() {
                        "-".into()
                    } else {
                        p.axis_rot.clone()
                    }
                ),
                ("n", p.actuators.len().to_string()),
                ("pump", p.pump.clone()),
            ]
        )
    );
    let r = Rules::from_profile(&p);
    r.check()?;
    let limits: [&str; 8] = [
        "tries: {retries}; careful after {c} troubles at {cs} per cent, slow after {s} at {ss} per cent, held after {h}",
        "reintentos: {retries}; prudente tras {c} problemas al {cs} %, lento tras {s} al {ss} %, retenida tras {h}",
        "Wiederholungen: {retries}; vorsichtig nach {c} Störungen mit {cs} %, langsam nach {s} mit {ss} %, angehalten nach {h}",
        "essais : {retries} ; prudent après {c} incidents à {cs} %, lent après {s} à {ss} %, retenue après {h}",
        "tentativi: {retries}; prudente dopo {c} problemi al {cs} %, lento dopo {s} al {ss} %, fermo dopo {h}",
        "再試行：{retries}；{c} 回の不具合で慎重（{cs} %）、{s} 回で低速（{ss} %）、{h} 回で保留",
        "重试：{retries}；{c} 次故障后谨慎（{cs} %），{s} 次后慢速（{ss} %），{h} 次后保持",
        "tentativas: {retries}; prudente após {c} problemas a {cs} %, lento após {s} a {ss} %, retida após {h}",
    ];
    println!(
        "{}",
        say(
            lang,
            &limits,
            &[
                ("retries", p.retries.to_string()),
                ("c", p.careful_events.to_string()),
                ("cs", show(p.careful_speed_pct)),
                ("s", p.slow_events.to_string()),
                ("ss", show(p.slow_speed_pct)),
                ("h", p.hold_events.to_string()),
            ]
        )
    );
    let sim: [&str; 8] = [
        "evidence=simulator: the profile has been tried only against a simulated controller; no port was opened",
        "evidence=simulator: el perfil solo se ha probado contra un controlador simulado; no se abrió ningún puerto",
        "evidence=simulator: das Profil wurde nur gegen eine simulierte Steuerung geprüft; kein Port wurde geöffnet",
        "evidence=simulator : le profil n'a été essayé que sur un contrôleur simulé ; aucun port n'a été ouvert",
        "evidence=simulator: il profilo è stato provato solo con un controllore simulato; nessuna porta è stata aperta",
        "evidence=simulator：プロファイルは模擬コントローラでのみ試験されており、ポートは開いていません",
        "evidence=simulator：该配置档只在模拟控制器上试过，没有打开任何端口",
        "evidence=simulator: o perfil só foi experimentado com um controlador simulado; nenhuma porta foi aberta",
    ];
    println!("{}", pick(lang, &sim));
    Ok(())
}

/// `driver-test PROFILE TRANSCRIPT`: plays recorded answers against the patterns of a profile and names the lines it reads differently.
pub fn driver_test(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 2)?;
    let (pp, tp) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("driver-test needs a profile and a transcript".into()),
    };
    let profile = load_profile(&pp, lang)?;
    let (rows, bad) = drivertest::check(&profile, &read_text(&tp, cfg.max_file_bytes)?);
    for m in &bad {
        println!("{}", drivertest::message(m, lang));
    }
    let done: [&str; 8] = [
        "{rows} rows read, {bad} read differently",
        "{rows} filas leídas, {bad} leídas de otra manera",
        "{rows} Zeilen gelesen, {bad} anders gelesen",
        "{rows} lignes lues, {bad} lues autrement",
        "{rows} righe lette, {bad} lette in modo diverso",
        "{rows} 行を読み、{bad} 行が異なる読み方でした",
        "读取 {rows} 行，{bad} 行读法不同",
        "{rows} linhas lidas, {bad} lidas de outra maneira",
    ];
    let line = say(
        lang,
        &done,
        &[("rows", rows.to_string()), ("bad", bad.len().to_string())],
    );
    if bad.is_empty() {
        println!("{line}");
        Ok(())
    } else {
        fail(line)
    }
}

fn session_text(failure: &Failure, lang: Lang) -> String {
    format!(
        "{}: {}",
        failure.kind.localized(lang),
        failure.localized(lang)
    )
}

/// The options that say which job of a board the driver is given: `--side`, `--board-width`, `--origin-x` and `--partial`.
pub fn job_options(args: &mut Vec<String>) -> Result<JobOptions, String> {
    let partial = switch(args, "--partial");
    let side = match flag(args, "--side")?.as_deref() {
        None | Some("top") => Side::Top,
        Some("bottom") => Side::Bottom,
        Some(o) => return Err(format!("--side '{o}' is not top or bottom")),
    };
    let board_width_mm = match flag(args, "--board-width")? {
        Some(v) => Some(num(&v, "--board-width")?),
        None => None,
    };
    let origin_x_mm = match flag(args, "--origin-x")? {
        Some(v) => num(&v, "--origin-x")?,
        None => 0.0,
    };
    Ok(JobOptions {
        side,
        board_width_mm,
        origin_x_mm,
        partial,
    })
}

/// The placements of the job of the files `args[0]` (parts) and `args[1]` (feeders); the marks of the board that are left out are named.
pub fn job_of(
    args: &[String],
    profile: &DriverProfile,
    cfg: &MachineConfig,
    options: JobOptions,
    lang: Lang,
) -> Result<eh3d_phoenix_pnp::plan::Plan, String> {
    let parts = read_text(&args[0], cfg.max_file_bytes)?;
    let feeders = read_text(&args[1], cfg.max_file_bytes)?;
    let job = toolpath::plan_job(&parts, &feeders, profile, cfg, options, lang)?;
    if !job.marks.is_empty() {
        let row: [&str; 8] = [
            "{n} parts are marks of the board and are not placed ({names})",
            "{n} piezas son marcas de la placa y no se colocan ({names})",
            "{n} Teile sind Marken der Platine und werden nicht bestückt ({names})",
            "{n} composants sont des repères de la carte et ne sont pas posés ({names})",
            "{n} componenti sono riferimenti della scheda e non vengono posati ({names})",
            "{n} 個の部品は基板のマークのため実装しません（{names}）",
            "{n} 个元件是电路板标记点，不进行贴装（{names}）",
            "{n} componentes são marcas da placa e não são colocados ({names})",
        ];
        println!(
            "{}",
            say(
                lang,
                &row,
                &[
                    ("n", job.marks.len().to_string()),
                    ("names", job.marks.join(", "))
                ]
            )
        );
    }
    Ok(job.plan)
}

/// Says which lines of the safe stop the controller did not confirm, so that a person looks at what they switch off.
fn warn_unconfirmed<L: Link>(s: &Session<L>, lang: Lang) {
    if s.unconfirmed.is_empty() {
        return;
    }
    let row: [&str; 8] = [
        "WARNING: the controller did not confirm these lines that leave the machine safe: {lines}. Check the pump, the valves and the motors by hand",
        "AVISO: el controlador no confirmó estas líneas que dejan la máquina segura: {lines}. Compruebe a mano la bomba, las válvulas y los motores",
        "WARNUNG: die Steuerung hat diese Zeilen, die die Maschine sichern, nicht bestätigt: {lines}. Pumpe, Ventile und Motoren von Hand prüfen",
        "ATTENTION : le contrôleur n'a pas confirmé ces lignes de mise en sécurité : {lines}. Vérifiez à la main la pompe, les vannes et les moteurs",
        "ATTENZIONE: il controllore non ha confermato queste righe che mettono in sicurezza la macchina: {lines}. Controllare a mano pompa, valvole e motori",
        "警告：機械を安全にする次の行がコントローラに確認されていません：{lines}。ポンプ・バルブ・モーターを手で確認してください",
        "警告：控制器未确认以下使机器进入安全状态的行：{lines}。请手动检查气泵、电磁阀和电机",
        "AVISO: o controlador não confirmou estas linhas que deixam a máquina segura: {lines}. Verifique à mão a bomba, as válvulas e os motores",
    ];
    eprintln!(
        "{}",
        say(lang, &row, &[("lines", s.unconfirmed.join(" | "))])
    );
}

/// Runs the plan on a session over any link.
fn run_on<L: Link>(
    s: &mut Session<L>,
    placements: &[Placement],
    lang: Lang,
) -> Result<usize, String> {
    s.connect().map_err(|f| session_text(&f, lang))?;
    s.home().map_err(|f| session_text(&f, lang))?;
    let report = s.run_placements(placements);
    let _ = s.close();
    match report.failed {
        None => Ok(report.placed),
        Some((reference, f)) => {
            let row: [&str; 8] = [
                "stopped at {reference} after {placed} parts: {why}",
                "parado en {reference} tras {placed} piezas: {why}",
                "bei {reference} nach {placed} Teilen angehalten: {why}",
                "arrêté à {reference} après {placed} composants : {why}",
                "fermato a {reference} dopo {placed} componenti: {why}",
                "{placed} 個の後、{reference} で停止しました：{why}",
                "贴装 {placed} 个后在 {reference} 处停止：{why}",
                "parado em {reference} após {placed} componentes: {why}",
            ];
            fail(say(
                lang,
                &row,
                &[
                    ("reference", reference),
                    ("placed", report.placed.to_string()),
                    ("why", session_text(&f, lang)),
                ],
            ))
        }
    }
}

/// `driver-run PARTS FEEDERS PROFILE [--side top|bottom] [--board-width MM] [--origin-x MM] [--fault ...] [--seed S] [--script-out F] [--log-out F] [--partial]`: plans the board, turns the plan into the lines of
/// the profile and sends them to a SIMULATED controller of the dialect of the profile, with its timeouts, retries and stop.
pub fn driver_run(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let faults = faults_of(&mut args)?;
    let seed = seed_of(&mut args)?;
    let script_out = flag(&mut args, "--script-out")?;
    let log_out = flag(&mut args, "--log-out")?;
    let options = job_options(&mut args)?;
    finish(&args, 3)?;
    if args.len() < 3 {
        return Err("driver-run needs the parts, the feeders and a profile".into());
    }
    let profile = load_profile(&args[2], lang)?;
    let planned = job_of(&args, &profile, &cfg, options, lang)?;
    // the lines of the whole job, before anything is sent: a step the profile cannot write stops the command here
    let script = toolpath::script_in(&profile, &planned.placements, lang)?;
    let inputs = vec![args[0].clone(), args[1].clone(), args[2].clone()];
    let result = match profile.transport {
        Transport::Serial => {
            let mut s = Session::new(
                &profile,
                SimLink::new(GcodeSim::new(&profile, faults, seed)),
            );
            let r = run_on(&mut s, &planned.placements, lang);
            warn_unconfirmed(&s, lang);
            finish_run(&s.log, &log_out, &inputs, lang)?;
            r
        }
        Transport::Moonraker => {
            let link = MoonrakerLink::new(&profile, SimMoonraker::new(&profile, faults, seed));
            let mut s = Session::new(&profile, link);
            let r = run_on(&mut s, &planned.placements, lang);
            warn_unconfirmed(&s, lang);
            finish_run(&s.log, &log_out, &inputs, lang)?;
            r
        }
    };
    if let Some(o) = &script_out {
        let mut text = script.join("\n");
        text.push('\n');
        deliver(&text, Some(o), &inputs, lang)?;
    }
    let sim: [&str; 8] = [
        "evidence=simulator: sent to a simulated {dialect} controller, nothing was moved",
        "evidence=simulator: enviado a un controlador {dialect} simulado, no se movió nada",
        "evidence=simulator: an eine simulierte {dialect}-Steuerung gesendet, nichts wurde bewegt",
        "evidence=simulator : envoyé à un contrôleur {dialect} simulé, rien n'a bougé",
        "evidence=simulator: inviato a un controllore {dialect} simulato, nulla si è mosso",
        "evidence=simulator：模擬 {dialect} コントローラへ送信しました。何も動いていません",
        "evidence=simulator：已发送到模拟的 {dialect} 控制器，没有任何东西移动",
        "evidence=simulator: enviado para um controlador {dialect} simulado, nada se moveu",
    ];
    println!(
        "{}",
        say(
            lang,
            &sim,
            &[("dialect", profile.dialect.name().to_string())]
        )
    );
    match result {
        Ok(n) => {
            let row: [&str; 8] = [
                "{n} parts placed, {lines} lines in the script",
                "{n} piezas colocadas, {lines} líneas en el guion",
                "{n} Teile bestückt, {lines} Zeilen im Skript",
                "{n} composants posés, {lines} lignes dans le script",
                "{n} componenti posati, {lines} righe nello script",
                "{n} 個を実装しました。スクリプトは {lines} 行です",
                "已贴装 {n} 个元件，脚本共 {lines} 行",
                "{n} componentes colocados, {lines} linhas no guião",
            ];
            println!(
                "{}",
                say(
                    lang,
                    &row,
                    &[("n", n.to_string()), ("lines", script.len().to_string())]
                )
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn finish_run(
    log: &[eh3d_phoenix_pnp::driver::Entry],
    out: &Option<String>,
    inputs: &[String],
    lang: Lang,
) -> Result<(), String> {
    if let Some(o) = out {
        let mut text = String::from("at_s,line,result\n");
        for e in log {
            text.push_str(&format!(
                "{:.3},{},{}\n",
                e.at_s,
                eh3d_phoenix_pnp::parts::csv_field(&e.line),
                eh3d_phoenix_pnp::parts::csv_field(&e.result)
            ));
        }
        deliver(&text, Some(o), inputs, lang)?;
    }
    Ok(())
}

/// `safety-replay EVENTS [--profile F]`: replays a file of events against the rules of safety and shows the level and the limits after each; the exit code is 2 when the machine ends held or
/// stopped.
pub fn safety_replay(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let profile_path = flag(&mut args, "--profile")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("safety-replay needs a file of events")?;
    let rules = match &profile_path {
        Some(p) => Rules::from_profile(&load_profile(p, lang)?),
        None => Rules::from_profile(&DriverProfile::default()),
    };
    rules.check()?;
    let r = replay(&rules, &read_text(path, cfg.max_file_bytes)?);
    print!("{}", replay_text(&r, lang));
    match r.rows.last() {
        Some((_, level, _, _)) if *level >= Level::Hold => {
            fail(format!("the machine ends {}", level.name()))
        }
        _ => Ok(()),
    }
}

/// `homing-repeat PROFILE [--runs N] [--seed S] [--jitter MM] [--max-spread MM]`: homes a simulated machine N times and says how far apart the places it lands at are.
pub fn homing_repeat(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _) = language_and_config(&mut args)?;
    let runs = match flag(&mut args, "--runs")? {
        Some(v) => whole(&v, "--runs", 2.0, 1000.0)? as usize,
        None => 10,
    };
    let seed = seed_of(&mut args)?;
    let jitter = match flag(&mut args, "--jitter")? {
        Some(v) => num(&v, "--jitter")?,
        None => 0.0,
    };
    let max_spread_option = match flag(&mut args, "--max-spread")? {
        Some(v) => Some(num(&v, "--max-spread")?),
        None => None,
    };
    finish(&args, 1)?;
    let path = args.first().ok_or("homing-repeat needs a profile")?;
    let profile = load_profile(path, lang)?;
    let limit = max_spread_option.unwrap_or(profile.homing_spread_mm);
    if !(0.0..=100.0).contains(&jitter) || !(0.0..=100.0).contains(&limit) {
        return Err("--jitter and --max-spread are in mm and from 0 to 100".into());
    }
    let faults = Faults {
        homing_jitter: jitter,
        ..Faults::default()
    };
    let mut s = Session::new(
        &profile,
        SimLink::new(GcodeSim::new(&profile, faults, seed)),
    );
    s.connect().map_err(|f| session_text(&f, lang))?;
    let mut places: Vec<(f64, f64)> = Vec::new();
    for _ in 0..runs {
        s.home().map_err(|f| session_text(&f, lang))?;
        places.push(s.link.sim.true_position());
    }
    let spread = |f: fn(&(f64, f64)) -> f64| {
        let lo = places.iter().map(f).fold(f64::INFINITY, f64::min);
        let hi = places.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        hi - lo
    };
    let (sx, sy) = (spread(|p| p.0), spread(|p| p.1));
    let worst = sx.max(sy);
    let row: [&str; 8] = [
        "{runs} homings: spread {sx} mm in X and {sy} mm in Y (limit {limit} mm)",
        "{runs} homings: dispersión de {sx} mm en X y {sy} mm en Y (límite {limit} mm)",
        "{runs} Referenzfahrten: Streuung {sx} mm in X und {sy} mm in Y (Grenze {limit} mm)",
        "{runs} prises d'origine : dispersion de {sx} mm en X et {sy} mm en Y (limite {limit} mm)",
        "{runs} homing: dispersione di {sx} mm in X e {sy} mm in Y (limite {limit} mm)",
        "原点復帰 {runs} 回：X のばらつき {sx} mm、Y のばらつき {sy} mm（上限 {limit} mm）",
        "{runs} 次回零：X 方向离散 {sx} mm，Y 方向离散 {sy} mm（限值 {limit} mm）",
        "{runs} homings: dispersão de {sx} mm em X e {sy} mm em Y (limite {limit} mm)",
    ];
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("runs", runs.to_string()),
                ("sx", show((sx * 1000.0).round() / 1000.0)),
                ("sy", show((sy * 1000.0).round() / 1000.0)),
                ("limit", show(limit)),
            ]
        )
    );
    if profile.homing_mode == HomingMode::Sensorless {
        let rows: [&str; 8] = [
            "homing without switches: sensitivity {sx} in X and {sy} in Y",
            "referenciado sin finales de carrera: sensibilidad {sx} en X y {sy} en Y",
            "Referenzfahrt ohne Endschalter: Empfindlichkeit {sx} in X und {sy} in Y",
            "prise d'origine sans fins de course : sensibilité {sx} en X et {sy} en Y",
            "homing senza finecorsa: sensibilità {sx} in X e {sy} in Y",
            "リミットスイッチなしの原点復帰：感度 X {sx}、Y {sy}",
            "无限位开关回零：X 灵敏度 {sx}，Y 灵敏度 {sy}",
            "homing sem fins de curso: sensibilidade {sx} em X e {sy} em Y",
        ];
        let at =
            |i: usize| profile.homing_sensitivity[i].map_or("-".to_string(), |v| v.to_string());
        println!("{}", say(lang, &rows, &[("sx", at(0)), ("sy", at(1))]));
    }
    println!("evidence=simulator");
    if worst > limit {
        let row: [&str; 8] = [
            "the spread of {worst} mm is over {limit} mm",
            "la dispersión de {worst} mm supera {limit} mm",
            "die Streuung von {worst} mm liegt über {limit} mm",
            "la dispersion de {worst} mm dépasse {limit} mm",
            "la dispersione di {worst} mm supera {limit} mm",
            "ばらつき {worst} mm は {limit} mm を超えています",
            "离散 {worst} mm 超过 {limit} mm",
            "a dispersão de {worst} mm ultrapassa {limit} mm",
        ];
        fail(say(
            lang,
            &row,
            &[("worst", show(worst)), ("limit", show(limit))],
        ))
    } else {
        Ok(())
    }
}

/// `bringup PROFILE [--fault ...] [--seed S]`: the steps of a first start of a machine on a simulated controller: connect, home, ask the position, switch each actuator, move round a square.
pub fn bringup(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _) = language_and_config(&mut args)?;
    let faults = faults_of(&mut args)?;
    let seed = seed_of(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("bringup needs a profile")?;
    let profile = load_profile(path, lang)?;
    let mut s = Session::new(
        &profile,
        SimLink::new(GcodeSim::new(&profile, faults, seed)),
    );
    let mut results: Vec<(String, Result<(), Failure>)> = Vec::new();
    let mut step = |name: &str, r: Result<(), Failure>| results.push((name.to_string(), r));
    step("connect", s.connect());
    step("home", s.home());
    step("position", s.position().map(|_| ()));
    let names: Vec<String> = profile.actuators.iter().map(|a| a.name.clone()).collect();
    for n in &names {
        let r = s
            .set_actuator(n, true)
            .and_then(|()| s.set_actuator(n, false));
        step(&format!("actuator {n}"), r);
    }
    for (i, t) in profile.probe_moves().into_iter().enumerate() {
        let (name, feed) = match i {
            0 => ("lift".to_string(), profile.feed_z),
            n => (format!("move {n}"), profile.feed_travel),
        };
        let r = s.run_op(&Op::Move(t, feed));
        step(&name, r);
    }
    step("close", s.close());
    let mut failed = 0usize;
    for (name, r) in &results {
        match r {
            Ok(()) => println!("PASS  {name}"),
            Err(f) => {
                failed += 1;
                println!("FAIL  {name}: {}", session_text(f, lang));
            }
        }
    }
    warn_unconfirmed(&s, lang);
    println!("evidence=simulator");
    if failed == 0 {
        Ok(())
    } else {
        let row: [&str; 8] = [
            "{failed} of {all} steps failed",
            "{failed} de {all} pasos fallaron",
            "{failed} von {all} Schritten sind fehlgeschlagen",
            "{failed} étapes sur {all} ont échoué",
            "{failed} passi su {all} non riusciti",
            "{all} 手順のうち {failed} 件が失敗しました",
            "{all} 个步骤中有 {failed} 个失败",
            "{failed} de {all} passos falharam",
        ];
        fail(say(
            lang,
            &row,
            &[
                ("failed", failed.to_string()),
                ("all", results.len().to_string()),
            ],
        ))
    }
}
