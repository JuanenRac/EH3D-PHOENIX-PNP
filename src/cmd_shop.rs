// EH3D-PHOENIX-PNP - the commands of the shop floor that need no machine: job-fingerprint, serial-sign, serial-verify, head-split, footprint-check, reject-report, retry-policy,
// clearance, bom-cost and lot-age.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.
// The key of a signature is read from a file named on the command line and is never printed.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_flow::{deliver, whole};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::bomcost;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::clearance;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::feeding::{self, FeederSpec, Picks};
use eh3d_phoenix_pnp::footcheck;
use eh3d_phoenix_pnp::headsplit;
use eh3d_phoenix_pnp::lotage;
use eh3d_phoenix_pnp::lottrace;
use eh3d_phoenix_pnp::parts::{parse_all, Part};
use eh3d_phoenix_pnp::rejects;
use eh3d_phoenix_pnp::retrypolicy;
use eh3d_phoenix_pnp::serialsig;

/// The most bytes of a key file.
const MAX_KEY_BYTES: u64 = 4096;

/// The parts and the feeders of a command, with their texts (for the fingerprints).
struct Loaded {
    parts: Vec<Part>,
    feeders: Vec<FeederSpec>,
    picks: Picks,
    parts_text: String,
    feeders_text: String,
}

fn load(cfg: &MachineConfig, lang: Lang, parts: &str, feeders: &str) -> Result<Loaded, String> {
    let parts_text = read_text(parts, cfg.max_file_bytes)?;
    let feeders_text = read_text(feeders, cfg.max_file_bytes)?;
    let parsed = parse_all(&parts_text, &cfg.profile)?;
    let feeder_list =
        feeding::from_csv_with(&feeders_text, cfg.tray_fill).map_err(|e| e.localized(lang))?;
    let picks = feeding::assign(&parsed.parts, &feeder_list, None);
    Ok(Loaded {
        parts: parsed.parts,
        feeders: feeder_list,
        picks,
        parts_text,
        feeders_text,
    })
}

/// The key in a file: its bytes without the line ending at the end. The file must exist, hold something and be small.
fn read_key(path: &str) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > MAX_KEY_BYTES {
        return Err(format!("{path}: a key is up to {MAX_KEY_BYTES} bytes"));
    }
    let mut key = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    while key.last().is_some_and(|b| matches!(b, b'\n' | b'\r')) {
        key.pop();
    }
    if key.is_empty() {
        return Err(format!("{path}: the key file is empty"));
    }
    Ok(key)
}

/// The signature given on the command line: 64 digits in lower case hexadecimal, or `-` for none.
fn signature_arg(v: &str, what: &str) -> Result<String, String> {
    if v == serialsig::NONE
        || (v.len() == 64 && v.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    {
        Ok(v.to_string())
    } else {
        Err(format!(
            "{what} {v} is not a signature (64 digits of hexadecimal, or -)"
        ))
    }
}

/// `job-fingerprint FILE...`: the SHA-256 of each file, read as text with the line endings and the spaces at the end of a line made alike (so that the same table saved on Windows and
/// on Linux has the same fingerprint), and, for several files, one for the set. Up to 16 files.
pub fn job_fingerprint(mut args: Vec<String>) -> Result<(), String> {
    let (_, cfg) = language_and_config(&mut args)?;
    finish(&args, 16)?;
    if args.is_empty() {
        return Err("job-fingerprint needs at least one file".into());
    }
    let mut lines = Vec::new();
    for path in &args {
        let print = serialsig::fingerprint(&read_text(path, cfg.max_file_bytes)?);
        println!("{print}  {path}");
        lines.push(print);
    }
    if args.len() > 1 {
        println!(
            "{}  (the set of {} files)",
            serialsig::fingerprint(&lines.join("\n")),
            args.len()
        );
    }
    Ok(())
}

/// `serial-sign PARTS FEEDERS --serial S [--lots F] [--prev SIG] [--key-file F] [--out F] [--partial]`: the record of one board: its serial, the fingerprints of the tables it was made from,
/// each placed reference with its feeder and its lot, and a signature of all that (a SHA-256, or an HMAC-SHA-256 with the key of `--key-file`). `--prev` is the signature of the record of
/// the board before, which chains the records of a run. The record goes to the screen, or to `--out` (never over a file that exists or one that was read). A part that no feeder holds
/// stops the record unless `--partial` says the board is not complete.
pub fn serial_sign(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let serial = flag(&mut args, "--serial")?.ok_or("serial-sign needs --serial")?;
    let lots_path = flag(&mut args, "--lots")?;
    let prev = match flag(&mut args, "--prev")? {
        Some(v) => signature_arg(&v, "--prev")?,
        None => serialsig::NONE.to_string(),
    };
    let key = flag(&mut args, "--key-file")?
        .map(|p| read_key(&p).map(|k| (p, k)))
        .transpose()?;
    let out = flag(&mut args, "--out")?;
    let partial = switch(&mut args, "--partial");
    finish(&args, 2)?;
    let [parts_path, feeders_path] = args.as_slice() else {
        return Err("serial-sign needs a table of parts and a table of feeders".into());
    };
    let l = load(&cfg, lang, parts_path, feeders_path)?;
    let (lots, lots_hash) = match &lots_path {
        Some(p) => {
            let text = read_text(p, cfg.max_file_bytes)?;
            (
                lottrace::parse_lots(&text).map_err(|e| e.localized(lang))?,
                serialsig::fingerprint(&text),
            )
        }
        None => (Vec::new(), serialsig::NONE.to_string()),
    };
    let trace = lottrace::trace(&l.parts, &l.feeders, &l.picks, &lots);
    if !trace.without_feeder.is_empty() && !partial {
        return Err(format!(
            "no feeder holds {}; a record is for a board that was placed whole (--partial signs what was placed)",
            trace.without_feeder.join(" ")
        ));
    }
    let record = serialsig::Record {
        serial,
        parts: serialsig::fingerprint(&l.parts_text),
        feeders: serialsig::fingerprint(&l.feeders_text),
        lots: lots_hash,
        prev,
        rows: trace
            .rows
            .iter()
            .map(|r| serialsig::Row {
                reference: r.reference.clone(),
                feeder: r.feeder.clone(),
                lot: r.lot.as_ref().map(|x| x.lot.clone()).unwrap_or_default(),
            })
            .collect(),
    };
    let text = serialsig::sign(&record, key.as_ref().map(|k| k.1.as_slice()))
        .map_err(|e| e.localized(lang))?;
    let mut inputs = vec![parts_path.clone(), feeders_path.clone()];
    inputs.extend(lots_path);
    inputs.extend(key.map(|k| k.0));
    deliver(&text, out.as_deref(), &inputs, lang)
}

/// `serial-verify FILE [--key-file F] [--prev SIG]`: reads a record that `serial-sign` wrote and says whether it is whole: the shape, the counts and the signature. With `--prev` the record
/// must name that signature as the board before. The exit code is 2 when it is not.
pub fn serial_verify(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let key = flag(&mut args, "--key-file")?
        .map(|p| read_key(&p))
        .transpose()?;
    let prev = flag(&mut args, "--prev")?
        .map(|v| signature_arg(&v, "--prev"))
        .transpose()?;
    finish(&args, 1)?;
    let [path] = args.as_slice() else {
        return Err("serial-verify needs a record".into());
    };
    let text = read_text(path, cfg.max_file_bytes)?;
    let v = serialsig::verify(&text, key.as_deref(), prev.as_deref())
        .map_err(|e| crate::cmd_nodes::not_passed(format!("{path}: {}", e.localized(lang))))?;
    print!("{}", serialsig::describe(&v));
    println!("the record is whole");
    Ok(())
}

/// `head-split PARTS FEEDERS [--heads N] [--out F]`: plays the board on a machine with N heads (2 to 8; 2 by default) that take from the same feeders, in two ways: the board cut into
/// bands, and whole feeders to a head. It says how long each head works and waits and when the board is done. `--out` writes a row for each head of each way.
pub fn head_split(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let heads = match flag(&mut args, "--heads")? {
        Some(v) => whole(&v, "--heads", 1.0, headsplit::MAX_HEADS as f64)? as usize,
        None => 2,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let [parts_path, feeders_path] = args.as_slice() else {
        return Err("head-split needs a table of parts and a table of feeders".into());
    };
    let l = load(&cfg, lang, parts_path, feeders_path)?;
    let items: Vec<headsplit::Item> = l
        .picks
        .picks
        .iter()
        .map(|p| headsplit::Item {
            reference: l.parts[p.part].reference.clone(),
            feeder: l.feeders[p.feeder].base.id.clone(),
            x: l.parts[p.part].x,
        })
        .collect();
    let timing = headsplit::Timing {
        pick_s: cfg.pick_s,
        place_s: cfg.place_s,
    };
    let outcomes = headsplit::compare(&items, heads, timing);
    print!(
        "{}",
        headsplit::text(&outcomes, headsplit::single_head_s(&items, timing))
    );
    if !l.picks.without_feeder.is_empty() {
        println!("no feeder holds: {}", l.picks.without_feeder.join(" "));
    }
    if let Some(o) = &out {
        deliver(
            &headsplit::csv(&outcomes),
            Some(o),
            &[parts_path.clone(), feeders_path.clone()],
            lang,
        )?;
    }
    Ok(())
}

/// The table of packages of `--packages` and the table of parts of `--catalog`, if given.
fn catalog_from(
    args: &mut Vec<String>,
    cfg: &MachineConfig,
    lang: Lang,
    inputs: &mut Vec<String>,
) -> Result<Catalog, String> {
    let mut catalog = Catalog::new();
    if let Some(p) = flag(args, "--packages")? {
        catalog
            .add_packages(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    if let Some(p) = flag(args, "--catalog")? {
        catalog
            .add_parts(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    Ok(catalog)
}

/// `footprint-check PARTS [--packages F] [--catalog F] [--out F]`: how each package of the board stands against the table of packages: known, an alias of a row under another name
/// (`R_0603_1608Metric` for `0603`), made from its name (as `package-gen` does), one or two letters from a name of the table, or unknown. The exit code is 2 when a package is similar or
/// unknown, for a person to look at.
pub fn footprint_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut inputs = Vec::new();
    let catalog = catalog_from(&mut args, &cfg, lang, &mut inputs)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let [path] = args.as_slice() else {
        return Err("footprint-check needs a table of parts".into());
    };
    inputs.push(path.clone());
    let parts = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?.parts;
    let rows = footcheck::check(&parts, &catalog);
    print!("{}", footcheck::text(&rows));
    if let Some(o) = &out {
        deliver(&footcheck::csv(&rows), Some(o), &inputs, lang)?;
    }
    match footcheck::problems(&rows) {
        0 => Ok(()),
        n => fail(format!("{n} packages need a look")),
    }
}

/// `reject-report PARTS FEEDERS REJECTS [--boards N] [--max-rate PERCENT] [--out F]`: the parts that were rejected in a run (`reference,reason[,board]`) by reason in the order of a Pareto
/// chart, by package, by feeder and by reference. `--boards` is the boards of the run (else the different serials of the table, or one). The exit code is 2 when a reference was rejected
/// on `--max-rate` per cent of the boards or more (not looked at by default).
pub fn reject_report(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let boards = flag(&mut args, "--boards")?
        .map(|v| whole(&v, "--boards", 1.0, rejects::MAX_BOARDS as f64))
        .transpose()?;
    let max_rate = flag(&mut args, "--max-rate")?
        .map(|v| {
            let n = num(&v, "--max-rate")?;
            if (0.0..=100.0).contains(&n) {
                Ok(n)
            } else {
                Err(format!("--max-rate {v} is not from 0 to 100 per cent"))
            }
        })
        .transpose()?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 3)?;
    let [parts_path, feeders_path, rejects_path] = args.as_slice() else {
        return Err(
            "reject-report needs a table of parts, a table of feeders and a table of rejects"
                .into(),
        );
    };
    let l = load(&cfg, lang, parts_path, feeders_path)?;
    let table = rejects::parse(&read_text(rejects_path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let report = rejects::analyse(&table, boards, &l.parts, &l.feeders, &l.picks)
        .map_err(|e| e.localized(lang))?;
    print!("{}", report.text());
    if let Some(o) = &out {
        deliver(
            &report.csv(),
            Some(o),
            &[
                parts_path.clone(),
                feeders_path.clone(),
                rejects_path.clone(),
            ],
            lang,
        )?;
    }
    match max_rate {
        Some(limit) if !report.over(limit).is_empty() => fail(format!(
            "{} references were rejected on {limit}% of the boards or more",
            report.over(limit).len()
        )),
        _ => Ok(()),
    }
}

/// `retry-policy PARTS FEEDERS RULES [--out F]`: the retries that rules which look at a reference, a value, a package or a feeder give each part (the first rule that fits wins; the
/// settings give the rest) and what that does to the chance of a board going through with no part in error. It says which rules fit nothing and which never act. `--out` writes a row for
/// each part. The exit code is 2 when a rule fits no part or never acts.
pub fn retry_policy(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 3)?;
    let [parts_path, feeders_path, rules_path] = args.as_slice() else {
        return Err(
            "retry-policy needs a table of parts, a table of feeders and a table of rules".into(),
        );
    };
    let l = load(&cfg, lang, parts_path, feeders_path)?;
    let rules = retrypolicy::parse_rules(&read_text(rules_path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let policy = retrypolicy::apply(&l.parts, &l.feeders, &l.picks, &cfg, &rules);
    print!("{}", policy.text(&rules));
    if let Some(o) = &out {
        deliver(
            &policy.csv(),
            Some(o),
            &[parts_path.clone(), feeders_path.clone(), rules_path.clone()],
            lang,
        )?;
    }
    let (unused, shadowed) = (policy.unused().len(), policy.shadowed().len());
    if unused + shadowed == 0 {
        Ok(())
    } else {
        fail(format!(
            "{unused} rules fit no part and {shadowed} never act"
        ))
    }
}

/// `clearance PARTS --packages F [--catalog F] [--min MM] [--out F]`: the pairs of parts on the same side whose bodies (from the table of packages, turned with the part) are nearer than
/// `--min` (the key `clearance_min_mm`, 0.2 mm by default), or overlap. The exit code is 2 when there is one.
pub fn clearance_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut inputs = Vec::new();
    let catalog = catalog_from(&mut args, &cfg, lang, &mut inputs)?;
    let min = match flag(&mut args, "--min")? {
        Some(v) => {
            let n = num(&v, "--min")?;
            if !(0.0..=5.0).contains(&n) {
                return Err(format!("--min {v} is not from 0 to 5 mm"));
            }
            n
        }
        None => cfg.clearance_min_mm,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let [path] = args.as_slice() else {
        return Err("clearance needs a table of parts".into());
    };
    inputs.push(path.clone());
    let parts = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?.parts;
    let found = clearance::check(&parts, &catalog, min);
    if found.checked == 0 && !parts.is_empty() {
        return Err("no part has the size of a body: give the table of packages with --packages (body_long_mm and body_short_mm)".into());
    }
    print!("{}", found.text(min));
    if let Some(o) = &out {
        deliver(&found.csv(), Some(o), &inputs, lang)?;
    }
    if found.is_clear() {
        Ok(())
    } else {
        fail(format!("{} pairs are too near", found.pairs.len()))
    }
}

/// `bom-cost BOM PRICES [--boards N] [--spare PERCENT] [--currency TEXT] [--max-rows N] [--out F]`: what the run costs, from the bill of materials and a table of prices with price breaks
/// (`part_number|value[,package],price[,qty_from]`): the pieces of each part with the spares, the break they fall in, the cost of the run and of a board. The exit code is 2 when a part
/// has no price.
pub fn bom_cost(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let boards = match flag(&mut args, "--boards")? {
        Some(v) => whole(&v, "--boards", 1.0, bomcost::MAX_BOARDS as f64)?,
        None => 1,
    };
    let spare = match flag(&mut args, "--spare")? {
        Some(v) => num(&v, "--spare")?,
        None => 0.0,
    };
    let currency = flag(&mut args, "--currency")?.unwrap_or_default();
    if currency.chars().count() > 8 || currency.chars().any(char::is_control) {
        return Err("--currency is up to 8 characters".into());
    }
    let max_rows = match flag(&mut args, "--max-rows")? {
        Some(v) => whole(&v, "--max-rows", 1.0, 1_000_000.0)? as usize,
        None => 100_000,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let [bom_path, prices_path] = args.as_slice() else {
        return Err("bom-cost needs a bill of materials and a table of prices".into());
    };
    let bom = Bom::parse(&read_text(bom_path, cfg.max_file_bytes)?, max_rows)
        .map_err(|e| e.localized(lang))?;
    let prices = bomcost::parse_prices(&read_text(prices_path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let cost = bomcost::cost(&bom, &prices, boards, spare).map_err(|e| e.localized(lang))?;
    print!("{}", cost.text(&currency));
    if let Some(o) = &out {
        deliver(
            &cost.csv(),
            Some(o),
            &[bom_path.clone(), prices_path.clone()],
            lang,
        )?;
    }
    if cost.unpriced.is_empty() {
        Ok(())
    } else {
        fail(format!("{} parts have no price", cost.unpriced.len()))
    }
}

/// `lot-age PARTS FEEDERS LOTS --week YYWW [--max-weeks N] [--out F]`: how old the lot in each feeder that gives parts is, from the date code of the table of lots (`YYWW` or `YYYYWW`),
/// against the week that is given (never read from a clock) and `--max-weeks` (the key `lot_max_weeks`, 104 by default). The exit code is 2 when a lot is old, from the future, without a
/// readable code or missing.
pub fn lot_age(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let week =
        flag(&mut args, "--week")?.ok_or("lot-age needs --week YYWW (the week to judge by)")?;
    let now = lotage::parse_week(&week)
        .ok_or_else(|| format!("--week {week} is not a week written YYWW or YYYYWW"))?;
    let max_weeks = match flag(&mut args, "--max-weeks")? {
        Some(v) => whole(&v, "--max-weeks", 1.0, 1040.0)? as u32,
        None => cfg.lot_max_weeks,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 3)?;
    let [parts_path, feeders_path, lots_path] = args.as_slice() else {
        return Err(
            "lot-age needs a table of parts, a table of feeders and a table of lots".into(),
        );
    };
    let l = load(&cfg, lang, parts_path, feeders_path)?;
    let lots = lottrace::parse_lots(&read_text(lots_path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let rows = lotage::check(&l.feeders, &l.picks, &lots, now, max_weeks);
    print!("{}", lotage::text(&rows, max_weeks));
    if let Some(o) = &out {
        deliver(
            &lotage::csv(&rows),
            Some(o),
            &[parts_path.clone(), feeders_path.clone(), lots_path.clone()],
            lang,
        )?;
    }
    match lotage::problems(&rows) {
        0 => Ok(()),
        n => fail(format!("{n} feeders have a lot to look at")),
    }
}

/// Keeps `PnpError` in use for the help of the localized messages above.
#[allow(dead_code)]
fn _localized(e: PnpError, lang: Lang) -> String {
    e.localized(lang)
}
