// EH3D-PHOENIX-PNP - the commands that compare and count: parts-diff, config-diff, board-stats, queue-stock and feeder-deplete.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::config::MAX_CONFIG_BYTES;
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::msg8::say;
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_phoenix_pnp::revision;
use eh3d_phoenix_pnp::{
    boardstats, changeover, configdiff, deplete, exchange, fwdiff, jobdiff, machsheet, partsdiff,
    queuestock, refill, report,
};

/// `parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F]`: what changed between two tables of parts, part by part (added, removed, moved, turned, another value, another package,
/// the other side). The report goes to the screen, or the changes as a CSV to `--out` (never over a file that exists). The exit code is 2 when the tables differ.
pub fn parts_diff(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut settings = partsdiff::Settings {
        tol_mm: cfg.parts_diff_tol_mm,
        rot_tol_deg: cfg.parts_diff_rot_tol_deg,
    };
    if let Some(v) = flag(&mut args, "--tol")? {
        settings.tol_mm = num(&v, "--tol")?;
    }
    if let Some(v) = flag(&mut args, "--rot-tol")? {
        settings.rot_tol_deg = num(&v, "--rot-tol")?;
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("parts-diff needs two tables of parts".into()),
    };
    let l = |e: PnpError| e.localized(lang);
    let first = parse_all(&read_text(&a, cfg.max_file_bytes)?, &cfg.profile)?;
    let second = parse_all(&read_text(&b, cfg.max_file_bytes)?, &cfg.profile)?;
    let diff = partsdiff::diff(&first.parts, &second.parts, &settings).map_err(l)?;
    print!("{}", diff.text());
    if let Some(o) = &out {
        refuse_same_file(&[a, b], o)?;
        report::write_new(o, &diff.csv()).map_err(l)?;
        println!("written to {o}");
    }
    if diff.is_same() {
        Ok(())
    } else {
        fail(format!("{} parts differ", diff.parts_changed()))
    }
}

/// `config-diff FIRST SECOND [--lang CODE]`: the settings that two configuration files give differently, in what they mean (`yes` and `on`, `5` and `5.0`, and a line that gives the
/// default are the same as they are). The exit code is 2 when the machines differ.
pub fn config_diff(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("config-diff needs two configuration files".into()),
    };
    let first = read_text(&a, MAX_CONFIG_BYTES as u64)?;
    let second = read_text(&b, MAX_CONFIG_BYTES as u64)?;
    let diff = configdiff::diff(&first, &second).map_err(|(which, e)| {
        let path = if which == "first" { &a } else { &b };
        format!("{path}: {}", e.localized(lang))
    })?;
    print!("{}", diff.text());
    if diff.is_same() {
        Ok(())
    } else {
        fail(format!("{} settings differ", diff.changes.len()))
    }
}

/// `fw-config-diff FIRST SECOND [--lang CODE]`: the values that two dumps of the configuration of a firmware (the answer to `M503`) give differently, and a warning when the firmware is not the
/// same in both (the EEPROM is to be reset). The exit code is 2 when the values differ.
pub fn fw_config_diff(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("fw-config-diff needs two dumps of a firmware".into()),
    };
    let load = |path: &str| -> Result<fwdiff::Dump, String> {
        fwdiff::parse(&read_text(path, fwdiff::MAX_DUMP_BYTES as u64)?)
            .map_err(|e| format!("{path}: {e}"))
    };
    let diff = fwdiff::diff(&load(&a)?, &load(&b)?);
    for line in diff.lines() {
        println!("{line}");
    }
    let (added, removed, changed) = diff.counts();
    let sum: [&str; 8] = [
        "{n} values differ ({c} changed, {a} only in the second, {r} only in the first); {s} are written differently and mean the same",
        "{n} valores difieren ({c} cambiados, {a} solo en el segundo, {r} solo en el primero); {s} se escriben distinto y significan lo mismo",
        "{n} Werte weichen ab ({c} geändert, {a} nur im zweiten, {r} nur im ersten); {s} sind anders geschrieben und bedeuten dasselbe",
        "{n} valeurs diffèrent ({c} modifiées, {a} seulement dans le second, {r} seulement dans le premier) ; {s} sont écrites autrement et veulent dire la même chose",
        "{n} valori differiscono ({c} cambiati, {a} solo nel secondo, {r} solo nel primo); {s} sono scritti diversamente e significano lo stesso",
        "{n} 個の値が異なります（変更 {c}、2つ目のみ {a}、1つ目のみ {r}）。表記が違うだけで同じ意味のものが {s} 個あります",
        "{n} 个值不同（更改 {c}，仅第二个有 {a}，仅第一个有 {r}）；{s} 个写法不同但含义相同",
        "{n} valores diferem ({c} alterados, {a} só no segundo, {r} só no primeiro); {s} escrevem-se de outra forma e querem dizer o mesmo",
    ];
    println!(
        "{}",
        say(
            lang,
            &sum,
            &[
                ("n", diff.changes.len().to_string()),
                ("c", changed.to_string()),
                ("a", added.to_string()),
                ("r", removed.to_string()),
                ("s", diff.same_meaning.to_string()),
            ]
        )
    );
    if let Some((old, new)) = diff.firmware_changed() {
        let warn: [&str; 8] = [
            "warning: the firmware changed from {old} to {new}; after flashing, reset the EEPROM (M502, then M500) and read the dump again, or the values stored by the old firmware stay in force",
            "aviso: el firmware cambió de {old} a {new}; tras flashear, restablece la EEPROM (M502 y luego M500) y vuelve a leer el volcado, o seguirán en vigor los valores guardados por el firmware anterior",
            "Warnung: Die Firmware wechselte von {old} zu {new}; nach dem Flashen den EEPROM zurücksetzen (M502, dann M500) und den Auszug neu lesen, sonst gelten die vom alten Firmware gespeicherten Werte weiter",
            "avertissement : le firmware est passé de {old} à {new} ; après le flashage, réinitialisez l'EEPROM (M502, puis M500) et relisez le relevé, sinon les valeurs enregistrées par l'ancien firmware restent en vigueur",
            "avviso: il firmware è passato da {old} a {new}; dopo il flash, reimposta l'EEPROM (M502, poi M500) e rileggi il dump, altrimenti restano in vigore i valori salvati dal firmware precedente",
            "警告：ファームウェアが {old} から {new} に変わりました。書き込み後は EEPROM をリセット（M502 のあと M500）してダンプを読み直してください。そうしないと旧ファームウェアが保存した値が有効のままです",
            "警告：固件已从 {old} 变为 {new}；刷写后请重置 EEPROM（先 M502 再 M500）并重新读取转储，否则旧固件保存的值仍然有效",
            "aviso: o firmware mudou de {old} para {new}; depois de gravar, reponha a EEPROM (M502 e depois M500) e volte a ler o despejo, ou os valores guardados pelo firmware anterior continuam em vigor",
        ];
        println!(
            "{}",
            say(
                lang,
                &warn,
                &[("old", old.to_string()), ("new", new.to_string())]
            )
        );
    }
    if diff.is_same() {
        Ok(())
    } else {
        fail(format!(
            "{} values of the firmware differ",
            diff.changes.len()
        ))
    }
}

/// `machine-sheet [--config F] [--lang CODE]`: the specification sheet of the machine as the settings give it; what nobody measured says so.
pub fn machine_sheet(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 0)?;
    print!("{}", machsheet::from_config(&cfg).text(lang));
    Ok(())
}

/// `board-stats PARTS [--near MM] [--limit N]`: what the table holds (the parts on each side, the kinds, a line for each package, the box of the centres) and, with `--near`, the pairs of
/// parts of one side whose centres are closer than that distance. The exit code is 2 when `--near` finds a pair.
pub fn board_stats(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let near = match flag(&mut args, "--near")? {
        Some(v) => Some(num(&v, "--near")?),
        None => Some(cfg.board_stats_near_mm).filter(|d| *d > 0.0),
    };
    let limit = match flag(&mut args, "--limit")? {
        Some(v) => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("--limit '{v}' is not a whole number"))?,
        None => cfg.board_stats_limit as usize,
    };
    finish(&args, 1)?;
    let path = args.first().ok_or("board-stats needs a table of parts")?;
    let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
    print!(
        "{}",
        boardstats::stats(&parsed.parts, parsed.dnp.len()).text()
    );
    let Some(within) = near else {
        return Ok(());
    };
    let pairs = boardstats::near(&parsed.parts, within).map_err(|e| e.localized(lang))?;
    print!("{}", boardstats::near_text(&pairs, within, limit));
    if pairs.is_empty() {
        Ok(())
    } else {
        fail(format!("{} pairs of parts are too near", pairs.len()))
    }
}

/// `queue-stock QUEUE [--out F] [--config F]`: whether the feeders hold enough parts for all the jobs of the queue that are still to be made, one after the other, and the job in which
/// each reel that falls short runs out. Jobs that share a table of feeders share its reels; a panel is not counted (the line says so). The exit code is 2 when a reel falls short.
pub fn queue_stock(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("queue-stock needs a queue file")?
        .clone();
    let found = queuestock::from_queue(&path, &cfg, lang, &read_text)?;
    print!("{}", found.text);
    if let (Some(o), Some(csv)) = (&out, &found.csv) {
        refuse_same_file(&found.inputs, o)?;
        report::write_new(o, csv).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    if found.short == 0 {
        Ok(())
    } else {
        fail(format!(
            "{} kinds of part fall short for the queue",
            found.short
        ))
    }
}

/// `feeder-deplete FEEDERS --state STATE [--out F]`: the table of feeders as it stands after the job that the saved state (`simulate --state`) is of: the count of each feeder is what is left
/// and the next place of a strip moves along. Without `--out` it only says what each feeder gave; with it the table is written (never over a file that exists, and never over the table
/// that was read). The exit code is 2 when a feeder is low or empty.
pub fn feeder_deplete(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let state = flag(&mut args, "--state")?.ok_or("feeder-deplete needs --state FILE")?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let feeders = args
        .first()
        .ok_or("feeder-deplete needs a table of feeders")?
        .clone();
    let l = |e: PnpError| e.localized(lang);
    let result = deplete::deplete(
        &read_text(&feeders, cfg.max_file_bytes)?,
        &read_text(&state, cfg.max_file_bytes)?,
    )
    .map_err(l)?;
    print!("{}", result.text());
    if let Some(o) = &out {
        refuse_same_file(&[feeders, state], o)?;
        report::write_new(o, &result.csv).map_err(l)?;
        println!("written to {o}");
    }
    if result.low() == 0 {
        Ok(())
    } else {
        fail(format!("{} feeders are low or empty", result.low()))
    }
}

/// `feeder-refill FEEDERS REFILLS [--out F]`: the table of feeders after reels were changed. `REFILLS` is a table `id,count[,x,y]` with a row for each feeder that got a new reel: the
/// count of the feeder is the parts of the new reel and a strip starts again at `x,y`. The report goes to the screen, or the table to `--out` (never over a file that exists, and never over a
/// file that was read). A feeder that is not in the table or a refill that does not fit it is refused.
pub fn feeder_refill(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    let held_to = flag(&mut args, "--revision")?;
    finish(&args, 2)?;
    let (feeders, refills) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("feeder-refill needs a table of feeders and a table of refills".into()),
    };
    let l = |e: PnpError| e.localized(lang);
    let rows = refill::parse_refills(&read_text(&refills, cfg.max_file_bytes)?).map_err(l)?;
    let result = refill::refill(&read_text(&feeders, cfg.max_file_bytes)?, &rows).map_err(l)?;
    print!("{}", result.text());
    match (&out, &held_to) {
        (Some(o), Some(token)) => {
            // with a revision the file named may be replaced, the table that was read included, but only if it is still as it was when the revision was taken
            refuse_same_file(std::slice::from_ref(&refills), o)?;
            let new = revision::replace(o, token, &result.csv, cfg.max_file_bytes).map_err(l)?;
            println!("written to {o} (revision {new})");
        }
        (Some(o), None) => {
            refuse_same_file(&[feeders, refills], o)?;
            report::write_new(o, &result.csv).map_err(l)?;
            println!("written to {o}");
        }
        (None, Some(_)) => {
            return Err("--revision needs --out: it holds the file that is replaced".into())
        }
        (None, None) => {}
    }
    Ok(())
}

/// `job-diff FIRST SECOND [--out F]`: what differs between two files of `job-export`, part by part and in the plan as a whole. The report goes to the screen, or the differences as a table to
/// `--out` (never over a file that exists, and never over a file that was read). The exit code is 2 when the jobs differ.
pub fn job_diff(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("job-diff needs two files of job-export".into()),
    };
    let read = |path: &str| -> Result<exchange::Exchange, String> {
        exchange::parse(&read_text(path, cfg.max_file_bytes)?)
            .map_err(|e| format!("{path}: {}", e.localized(lang)))
    };
    let (first, second) = (read(&a)?, read(&b)?);
    let found = jobdiff::diff(&first, &second);
    print!("{}", found.text());
    if let Some(o) = &out {
        refuse_same_file(&[a, b], o)?;
        report::write_new(o, &found.csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    if found.is_same() {
        Ok(())
    } else {
        fail(format!("{} parts differ", found.parts_changed()))
    }
}

/// `queue-changeover QUEUE [--out F] [--config F]`: what has to be changed on the feeders between one job of the queue and the next, for the jobs that are still to be made: the feeders that
/// hold another kind of part (a reel to change), the ones that are switched on for the next job only (a reel to put in), the ones it does not use (a reel that may stay) and how many stay as
/// they are. The report goes to the screen, or the table to `--out` (never over a file that exists, and never over a file that was read). The exit code is 2 when a reel has to be changed or put in.
pub fn queue_changeover(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("queue-changeover needs a queue file")?
        .clone();
    let found = changeover::from_queue(&path, &cfg, lang, &read_text)?;
    print!("{}", found.text);
    if let Some(o) = &out {
        refuse_same_file(&found.inputs, o)?;
        report::write_new(o, &found.csv).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    if found.work == 0 {
        Ok(())
    } else {
        fail(format!(
            "{} reels have to be changed or put in between the jobs",
            found.work
        ))
    }
}
