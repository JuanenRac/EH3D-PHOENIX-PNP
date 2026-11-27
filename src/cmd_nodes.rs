// EH3D-PHOENIX-PNP - the commands of the banks of feeders, of the composition of a panel and of the link to the nodes: slot-bank, panel-compose and node-sim.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine, and a file named by --out that exists is never written over. They end with the exit code 2 when a
// check of theirs is not passed (a bank that is not ready, a panel that does not fit, a link that was held), and with 1 when the arguments or the files are wrong.

use crate::cli_args::{finish, flag, num};
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::msg8::{pick, say};
use eh3d_phoenix_pnp::nodelink::{
    beat_period_ms, one_lost_question_is_enough, Feeders, Head, Hold, ScNodeLink,
};
use eh3d_phoenix_pnp::nodesim::{Injection, SimFeeders, SimHead};
use eh3d_phoenix_pnp::panelcomp::Composition;
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::revision;
use eh3d_phoenix_pnp::slotbank::{self, Bank, Geometry, Layout, State};
use std::sync::atomic::{AtomicBool, Ordering};

static CHECK_FAILED: AtomicBool = AtomicBool::new(false);

/// The error of a check that was not passed: the message, and the exit code of the program becomes 2.
pub fn not_passed(message: impl Into<String>) -> String {
    CHECK_FAILED.store(true, Ordering::SeqCst);
    message.into()
}

/// The result of a command whose check was not passed: an error with the message, and the exit code of the program becomes 2.
pub fn fail<T>(message: String) -> Result<T, String> {
    Err(not_passed(message))
}

/// The exit code for an error: 2 when a check was not passed, else 1.
pub fn exit_code() -> u8 {
    if CHECK_FAILED.load(Ordering::SeqCst) {
        2
    } else {
        1
    }
}

fn bounded(text: &str, what: &str, lo: f64, hi: f64) -> Result<f64, String> {
    let v = num(text, what)?;
    if !(lo..=hi).contains(&v) {
        return Err(format!("{what} {text} is not from {lo} to {hi}"));
    }
    Ok(v)
}

fn refuse_existing(out: Option<&str>) -> Result<(), String> {
    match out {
        Some(o) if std::path::Path::new(o).exists() => {
            Err(format!("{o}: exists and is never written over"))
        }
        _ => Ok(()),
    }
}

/// `table-revision FILE`: the revision of a table, to hold a later write to it (`feeder-refill --out FILE --revision TOKEN`).
pub fn table_revision(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("table-revision needs a table")?;
    let (_, token) = revision::read(path, cfg.max_file_bytes).map_err(|e| e.localized(lang))?;
    println!("{token}");
    Ok(())
}

/// `slot-bank TABLE [--tape MM] [--out F]`: the banks of feeders as the table `id,bank,first_slot,tape_mm,state,chain` puts them: a picture of each bank, the slots each feeder takes, and
/// where a tape of `--tape` millimetres would fit. The sizes of the slots are the `slot_*` settings. The exit code is 2 when a feeder is depleted or jammed.
pub fn slot_bank(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let tape = flag(&mut args, "--tape")?
        .map(|v| bounded(&v, "--tape", 2.0, 104.0))
        .transpose()?;
    let out = flag(&mut args, "--out")?;
    let held_to = flag(&mut args, "--revision")?;
    let mut events: Vec<(String, slotbank::Event)> = Vec::new();
    while let Some(v) = flag(&mut args, "--event")? {
        let (id, name) = v
            .split_once('=')
            .ok_or_else(|| format!("--event {v} is not ID=EVENT"))?;
        let event = slotbank::Event::ALL
            .into_iter()
            .find(|e| e.name() == name)
            .ok_or_else(|| {
                format!("--event {v}: '{name}' is not load, exhaust, jam, clear, reload or remove")
            })?;
        events.push((id.to_string(), event));
    }
    finish(&args, 1)?;
    if held_to.is_some() && out.is_none() {
        return Err("--revision needs --out: it holds the file that is replaced".into());
    }
    if held_to.is_none() {
        refuse_existing(out.as_deref())?;
    }
    let path = args.first().ok_or("slot-bank needs a table of the banks")?;
    let geo = Geometry::from_config(&cfg);
    let mut layout = Layout::from_csv(&read_text(path, cfg.max_file_bytes)?, geo)
        .map_err(|e| e.localized(lang))?;
    for (id, event) in &events {
        let to = layout.apply(id, *event).map_err(|e| e.localized(lang))?;
        println!("{id}: {} -> {}", event.name(), to.name());
    }
    print!("{}", layout.text());
    if let Some(mm) = tape {
        let width = geo.slots_for(mm).map_err(|e| e.localized(lang))?;
        println!("a tape of {mm} mm takes {width} slots");
        for bank in Bank::ALL {
            match layout.first_fit(bank, mm) {
                Some(first) => println!(
                    "{} bank: it fits from slot {first} to {}",
                    bank.name(),
                    first + width - 1
                ),
                None => println!("{} bank: it does not fit", bank.name()),
            }
        }
    }
    if let Some(o) = &out {
        match &held_to {
            // with a revision the file named may be replaced, the table that was read included, but only if it is still as it was when the revision was taken
            Some(token) => {
                let new = revision::replace(o, token, &layout.to_csv(), cfg.max_file_bytes)
                    .map_err(|e| e.localized(lang))?;
                println!("written to {o} (revision {new})");
            }
            None => {
                report::write_new(o, &layout.to_csv()).map_err(|e| e.localized(lang))?;
                println!("written to {o}");
            }
        }
    }
    let stopped = layout
        .entries()
        .iter()
        .filter(|e| matches!(e.state, State::Depleted | State::Jammed))
        .count();
    match stopped {
        0 => Ok(()),
        n => Err(not_passed(format!("{n} feeders are depleted or jammed"))),
    }
}

/// `panel-compose SPEC [--clearance MM] [--out F]`: checks the composition of a panel (the sheet, its rails, the outlines of the boards and where each unit goes and how it is turned) and
/// writes the panel file that the other commands read to `--out`. Nothing is written when a unit goes out of the area between the rails, two units overlap or two are closer than
/// `--clearance` (`panel_clearance_mm`). The exit code is 2 then.
pub fn panel_compose(mut args: Vec<String>) -> Result<(), String> {
    let (lang, mut cfg) = language_and_config(&mut args)?;
    if let Some(v) = flag(&mut args, "--clearance")? {
        cfg.panel_clearance_mm = bounded(&v, "--clearance", 0.0, 50.0)?;
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    refuse_existing(out.as_deref())?;
    let path = args
        .first()
        .ok_or("panel-compose needs a file of the composition")?;
    let comp =
        Composition::parse(&read_text(path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    print!("{}", comp.text(cfg.panel_clearance_mm));
    let findings = comp.check(cfg.panel_clearance_mm);
    if !findings.is_empty() {
        return Err(not_passed(format!(
            "{} problems: no panel file is written",
            findings.len()
        )));
    }
    match &out {
        Some(o) => {
            report::write_new(o, &comp.to_panel_json()).map_err(|e| e.localized(lang))?;
            println!("written to {o}");
        }
        None => print!("{}", comp.to_panel_json()),
    }
    Ok(())
}

/// `node-sim [--steps N] [--seed S] [--silence PM] [--unauthorized PM] [--interlock PM] [--pause-ms N]`: a job on a head node and a feeder node that are made up, with faults drawn from the
/// seed (per thousand requests). `--pause-ms` is the time that passes between two steps: longer than the watch of a node, the node is safe when it is asked again. The last line says the
/// watches the nodes gave. Each step asks the head whether the machine may move, asks for the permission to move, advances a feeder, picks and places with a nozzle, and lets the permission go. It says
/// what the link saw. The same seed gives the same text. The exit code is 2 when the link was held.
pub fn node_sim(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let steps = flag(&mut args, "--steps")?
        .map(|v| crate::cmd_flow::whole(&v, "--steps", 1.0, 10_000.0))
        .transpose()?
        .unwrap_or(20);
    let seed = flag(&mut args, "--seed")?
        .map(|v| crate::cmd_flow::whole(&v, "--seed", 1.0, 1e15))
        .transpose()?
        .unwrap_or(1);
    let mut rate = |name: &str| -> Result<u32, String> {
        Ok(flag(&mut args, name)?
            .map(|v| crate::cmd_flow::whole(&v, name, 0.0, 1000.0))
            .transpose()?
            .unwrap_or(0) as u32)
    };
    let (silence, unauthorized, interlock) = (
        rate("--silence")?,
        rate("--unauthorized")?,
        rate("--interlock")?,
    );
    let pause = flag(&mut args, "--pause-ms")?
        .map(|v| crate::cmd_flow::whole(&v, "--pause-ms", 0.0, 600_000.0))
        .transpose()?
        .unwrap_or(0) as u32;
    finish(&args, 0)?;
    let head_sim = SimHead::new(2, "t");
    let feed_sim = SimFeeders::new(4, Some(1000), "t");
    head_sim.set_injection(Injection::seeded(seed, silence, unauthorized, interlock));
    feed_sim.set_injection(Injection::seeded(seed ^ 0x9E37, silence, unauthorized, 0));
    let retries = 0;
    let link = |s: Box<dyn eh3d_serverclient::Transport>| {
        ScNodeLink::new("sim:1", "t", cfg.node_timeout_s, retries, s).without_waiting()
    };
    let mut head = Head::new(link(Box::new(head_sim.clone())), cfg.node_silence_limit);
    let mut feeders = Feeders::new(link(Box::new(feed_sim.clone())), cfg.node_silence_limit);
    let (mut moved, mut refused, mut skipped) = (0u32, 0u32, 0u32);
    for step in 1..=steps {
        if step > 1 {
            head_sim.elapse(pause);
            feed_sim.elapse(pause);
        }
        // the state of the chain is read at every step, which also keeps the feeder node hearing the program
        let _ = feeders.refresh();
        if !head.xy_clear() {
            skipped += 1;
            let row: [&str; 8] = [
                "step {step}: the head is not clear or cannot be heard: the machine does not move",
                "paso {step}: el cabezal no está libre o no se le oye: la máquina no se mueve",
                "Schritt {step}: der Kopf ist nicht frei oder nicht erreichbar: die Maschine bewegt sich nicht",
                "étape {step} : la tête n'est pas dégagée ou ne répond pas : la machine ne bouge pas",
                "passo {step}: la testa non è libera o non risponde: la macchina non si muove",
                "手順 {step}：ヘッドが退避していないか応答がないため、機械は動きません",
                "第 {step} 步：机头未让开或无法联系，机器不移动",
                "passo {step}: a cabeça não está livre ou não se ouve: a máquina não se move",
            ];
            println!("{}", say(lang, &row, &[("step", step.to_string())]));
            continue;
        }
        match head.set_xy_moving(true) {
            Ok(()) => {
                moved += 1;
                let advanced = feeders.advance(1, 1);
                let _ = head.set_xy_moving(false);
                // a part is picked only when the feeder said its tape moved
                if let Err(e) = advanced {
                    let row: [&str; 8] = [
                        "step {step}: moved; the feeder did not advance and nothing is picked: {why}",
                        "paso {step}: movido; el alimentador no avanzó y no se toma nada: {why}",
                        "Schritt {step}: bewegt; der Feeder hat nicht vorgeschoben, nichts wird aufgenommen: {why}",
                        "étape {step} : déplacé ; le chargeur n'a pas avancé, rien n'est pris : {why}",
                        "passo {step}: mosso; l'alimentatore non è avanzato e non si preleva nulla: {why}",
                        "手順 {step}：移動しました。フィーダが送られなかったため、何も取りません：{why}",
                        "第 {step} 步：已移动；供料器未进给，不拾取任何元件：{why}",
                        "passo {step}: movido; o alimentador não avançou e nada é apanhado: {why}",
                    ];
                    println!(
                        "{}",
                        say(
                            lang,
                            &row,
                            &[("step", step.to_string()), ("why", e.localized(lang))]
                        )
                    );
                    continue;
                }
                let picked = head.pick(1);
                let placed = head.place(1);
                let row: [&str; 8] = [
                    "step {step}: moved; pick {pick}, place {place}",
                    "paso {step}: movido; toma {pick}, colocación {place}",
                    "Schritt {step}: bewegt; Aufnahme {pick}, Absetzen {place}",
                    "étape {step} : déplacé ; prise {pick}, pose {place}",
                    "passo {step}: mosso; presa {pick}, posa {place}",
                    "手順 {step}：移動しました。ピック {pick}、プレース {place}",
                    "第 {step} 步：已移动；拾取 {pick}，贴放 {place}",
                    "passo {step}: movido; recolha {pick}, colocação {place}",
                ];
                println!(
                    "{}",
                    say(
                        lang,
                        &row,
                        &[
                            ("step", step.to_string()),
                            ("pick", picked.is_ok().to_string()),
                            ("place", placed.is_ok().to_string())
                        ]
                    )
                );
            }
            Err(e) => {
                refused += 1;
                let row: [&str; 8] = [
                    "step {step}: the permission was refused: {why}",
                    "paso {step}: se denegó el permiso: {why}",
                    "Schritt {step}: die Erlaubnis wurde verweigert: {why}",
                    "étape {step} : l'autorisation a été refusée : {why}",
                    "passo {step}: il permesso è stato negato: {why}",
                    "手順 {step}：許可が拒否されました：{why}",
                    "第 {step} 步：许可被拒绝：{why}",
                    "passo {step}: a autorização foi recusada: {why}",
                ];
                println!(
                    "{}",
                    say(
                        lang,
                        &row,
                        &[("step", step.to_string()), ("why", e.localized(lang))]
                    )
                );
            }
        }
    }
    let row: [&str; 8] = [
        "{steps} steps: {moved} moved, {refused} refused, {skipped} not clear; the head answered {head_n} requests, the feeders {feed_n}; head {head}, feeders {feed}",
        "{steps} pasos: {moved} movidos, {refused} denegados, {skipped} no libres; el cabezal respondió {head_n} peticiones, los alimentadores {feed_n}; cabezal {head}, alimentadores {feed}",
        "{steps} Schritte: {moved} bewegt, {refused} verweigert, {skipped} nicht frei; der Kopf beantwortete {head_n} Anfragen, die Feeder {feed_n}; Kopf {head}, Feeder {feed}",
        "{steps} étapes : {moved} déplacées, {refused} refusées, {skipped} non dégagées ; la tête a répondu à {head_n} requêtes, les chargeurs à {feed_n} ; tête {head}, chargeurs {feed}",
        "{steps} passi: {moved} mossi, {refused} negati, {skipped} non liberi; la testa ha risposto a {head_n} richieste, gli alimentatori a {feed_n}; testa {head}, alimentatori {feed}",
        "{steps} 手順：移動 {moved}、拒否 {refused}、未退避 {skipped}。ヘッドは {head_n} 件、フィーダは {feed_n} 件に応答。ヘッド {head}、フィーダ {feed}",
        "{steps} 步：移动 {moved}，拒绝 {refused}，未让开 {skipped}；机头应答 {head_n} 次请求，供料器 {feed_n} 次；机头 {head}，供料器 {feed}",
        "{steps} passos: {moved} movidos, {refused} recusados, {skipped} não livres; a cabeça respondeu a {head_n} pedidos, os alimentadores a {feed_n}; cabeça {head}, alimentadores {feed}",
    ];
    let hold_word = |hold: Option<Hold>| match hold {
        Some(h) => h.localized(lang),
        None => pick(
            lang,
            &[
                "not held",
                "sin retener",
                "nicht gesperrt",
                "non retenu",
                "non bloccato",
                "保留なし",
                "未保持",
                "sem retenção",
            ],
        ),
    };
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("steps", steps.to_string()),
                ("moved", moved.to_string()),
                ("refused", refused.to_string()),
                ("skipped", skipped.to_string()),
                ("head_n", head_sim.requests().to_string()),
                ("feed_n", feed_sim.requests().to_string()),
                ("head", hold_word(head.guard().held()).to_string()),
                ("feed", hold_word(feeders.guard().held()).to_string()),
            ]
        )
    );
    for (name, watch) in [
        (
            [
                "head",
                "cabezal",
                "Kopf",
                "tête",
                "testa",
                "ヘッド",
                "机头",
                "cabeça",
            ],
            head.watch_ms(),
        ),
        (
            [
                "feeders",
                "alimentadores",
                "Feeder",
                "chargeurs",
                "alimentatori",
                "フィーダ",
                "供料器",
                "alimentadores",
            ],
            feeders.watch_ms(),
        ),
    ] {
        let Some(watch) = watch else {
            continue;
        };
        let row: [&str; 8] = [
            "{name}: safe after {watch} ms without a question with the token; asked every {period} ms while an order is waited for",
            "{name}: pasa a seguro tras {watch} ms sin una pregunta con el token; se le pregunta cada {period} ms mientras se espera una orden",
            "{name}: sicher nach {watch} ms ohne Anfrage mit dem Token; während eines Auftrags alle {period} ms gefragt",
            "{name} : en état sûr après {watch} ms sans requête avec le jeton ; interrogé toutes les {period} ms pendant l'attente d'un ordre",
            "{name}: in stato sicuro dopo {watch} ms senza una richiesta con il token; interrogato ogni {period} ms mentre si attende un ordine",
            "{name}：トークン付きの問い合わせが {watch} ms ないと安全状態になります。指令の待機中は {period} ms ごとに問い合わせます",
            "{name}：{watch} ms 内没有带令牌的请求即进入安全状态；等待指令期间每 {period} ms 询问一次",
            "{name}: fica em estado seguro após {watch} ms sem uma pergunta com o token; perguntado a cada {period} ms enquanto se espera uma ordem",
        ];
        let short: [&str; 8] = [
            "; one question without an answer is enough for it (node_timeout_s)",
            "; basta una pregunta sin respuesta para ello (node_timeout_s)",
            "; eine Anfrage ohne Antwort genügt dafür (node_timeout_s)",
            " ; une seule requête sans réponse suffit (node_timeout_s)",
            "; basta una richiesta senza risposta (node_timeout_s)",
            "。応答のない問い合わせ 1 回でそうなります（node_timeout_s）",
            "；一次无应答的询问就足以触发（node_timeout_s）",
            "; basta uma pergunta sem resposta para isso (node_timeout_s)",
        ];
        println!(
            "{}{}",
            say(
                lang,
                &row,
                &[
                    ("name", pick(lang, &name).to_string()),
                    ("watch", watch.to_string()),
                    ("period", beat_period_ms(watch).to_string())
                ]
            ),
            if one_lost_question_is_enough(watch, cfg.node_timeout_s) {
                pick(lang, &short)
            } else {
                ""
            }
        );
    }
    if head.guard().held().is_some() || feeders.guard().held().is_some() {
        return Err(not_passed(pick(
            lang,
            &[
                "the link to a node was held",
                "se retuvo el enlace con un nodo",
                "die Verbindung zu einem Knoten wurde gesperrt",
                "la liaison avec un nœud a été retenue",
                "il collegamento a un nodo è stato bloccato",
                "ノードとの接続が保留されました",
                "与某个节点的连接已被保持",
                "a ligação a um nó ficou retida",
            ],
        )));
    }
    Ok(())
}
