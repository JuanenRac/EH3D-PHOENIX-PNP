// EH3D-PHOENIX-PNP - the commands of the queue of jobs: queue-add, queue-status, queue-confirm, queue-edit and queue-run.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// The queue is a file (see `queue`); each command reads it, changes it and writes it back whole, with the checksum. `queue-run` makes the jobs one board at a time on the simulated machine
// and writes the queue after every board, so that the program stopping loses at most the board that was under way. A job whose plan was not confirmed (`queue_confirm`) is not run.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::{plan_identity, simulate};
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::PnpError;
use eh3d_phoenix_pnp::queue::{self, Entry, Kind, Queue, State};
use std::path::Path;

/// The most bytes of a queue file.
const MAX_QUEUE_BYTES: u64 = 1_048_576;

fn load(path: &str, lang: eh3d_phoenix_pnp::error::Lang) -> Result<Queue, String> {
    queue::read_file(path, MAX_QUEUE_BYTES).map_err(|e| e.localized(lang))
}

fn store(path: &str, queue: &Queue, lang: eh3d_phoenix_pnp::error::Lang) -> Result<(), String> {
    queue::write_file(path, queue).map_err(|e| e.localized(lang))
}

/// The revision the write is to be held to (`--revision TOKEN`, as `table-revision` gives it), when the person gave one.
fn held_to(args: &mut Vec<String>) -> Result<Option<String>, String> {
    flag(args, "--revision")
}

/// Writes the queue, but only if the file is still as it was when `held` was taken (when there is one): a queue that somebody else changed since is not written over.
fn store_held(
    path: &str,
    queue: &Queue,
    lang: eh3d_phoenix_pnp::error::Lang,
    held: Option<&str>,
) -> Result<(), String> {
    if let Some(token) = held {
        eh3d_phoenix_pnp::revision::check(path, token, MAX_QUEUE_BYTES)
            .map_err(|e| e.localized(lang))?;
    }
    store(path, queue, lang)
}

/// The folder of the queue file, as an absolute path: where the relative paths of its entries are taken from.
pub fn queue_dir(queue_path: &str) -> std::path::PathBuf {
    let parent = Path::new(queue_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::path::absolute(parent).unwrap_or_else(|_| parent.to_path_buf())
}

/// A file that has to be there, as the absolute path it is found at (a queue is run from other folders than the one it was made in).
fn existing(path: &str, what: &str) -> Result<String, String> {
    if !Path::new(path).is_file() {
        return Err(format!("{what} '{path}': there is no such file"));
    }
    // kept the way every system reads it: `/` between the names, no `\?\` prefix, no `..`
    std::path::absolute(path)
        .map(|p| eh3d_phoenix_pnp::pathid::portable(&p.to_string_lossy()))
        .map_err(|e| format!("{path}: {e}"))
}

/// `queue-add QUEUE NAME SOURCE FEEDERS`: puts a job at the end of the queue (the file is made when it is not there). `--panel` says that the source is a panel, `--boards N` how many boards
/// to make, `--config`, `--packages` and `--catalog` the files that go with the job and `--side top|bottom` the side. The paths are kept absolute; with `queue_relative_paths` on in the
/// configuration of the job (or `--relative`) a file that is in the folder of the queue, or below it, is kept as a path from that folder, so that the folder can be moved.
pub fn queue_add(mut args: Vec<String>) -> Result<(), String> {
    let config = flag(&mut args, "--config")?;
    let packages = flag(&mut args, "--packages")?;
    let catalog = flag(&mut args, "--catalog")?;
    let side = flag(&mut args, "--side")?;
    let boards = match flag(&mut args, "--boards")? {
        Some(v) => {
            let n = num(&v, "--boards")?;
            if n.fract() != 0.0 || n < 1.0 || n > f64::from(queue::MAX_BOARDS) {
                return Err(format!(
                    "--boards {v} is not a whole number from 1 to {}",
                    queue::MAX_BOARDS
                ));
            }
            n as u32
        }
        None => 1,
    };
    let panel = switch(&mut args, "--panel");
    let held = held_to(&mut args)?;
    let relative_switch = switch(&mut args, "--relative");
    let (lang, _) = language_and_config(&mut args)?;
    finish(&args, 4)?;
    let [path, name, source, feeders] = args.as_slice() else {
        return Err("queue-add needs a queue file, a name, the table of parts (or the panel) and the table of feeders".into());
    };
    let mut relative = relative_switch;
    let mut entry = Entry::new(
        name,
        if panel { Kind::Panel } else { Kind::Parts },
        &existing(source, "the source")?,
        &existing(feeders, "the feeders")?,
        boards,
    );
    if let Some(c) = &config {
        let abs = existing(c, "the configuration")?;
        let text = std::fs::read_to_string(&abs).map_err(|e| format!("{abs}: {e}"))?;
        if text.len() > MAX_CONFIG_BYTES {
            return Err(format!(
                "{abs}: larger than the {MAX_CONFIG_BYTES} bytes of a configuration"
            ));
        }
        let parsed = MachineConfig::parse(&text).map_err(|e| e.localized(lang))?;
        relative |= parsed.queue_relative_paths;
        entry.config = abs;
    }
    if let Some(p) = &packages {
        entry.packages = existing(p, "the packages")?;
    }
    if let Some(c) = &catalog {
        entry.catalog = existing(c, "the catalog")?;
    }
    entry.side = side.unwrap_or_default();
    if relative {
        let dir = queue_dir(path);
        for v in [
            &mut entry.source,
            &mut entry.feeders,
            &mut entry.config,
            &mut entry.packages,
            &mut entry.catalog,
        ] {
            *v = queue::relative_to(v, &dir);
        }
    }
    let mut q = load(path, lang)?;
    q.add(entry).map_err(|e| e.localized(lang))?;
    store_held(path, &q, lang, held.as_deref())?;
    println!("added '{name}' to {path}: {} entries", q.len());
    Ok(())
}

/// What stands between an entry and its run: the reason it cannot be planned, or how its confirmation stands.
fn standing(q: &Queue, e: &Entry, need_confirm: bool, dir: &Path) -> Result<String, String> {
    let id = plan_identity(e.arguments_in(dir))?;
    match q.allowed(&e.name, &id, need_confirm) {
        Ok(()) => Ok(if e.confirmed.is_empty() {
            format!("plan {id}, not confirmed (queue_confirm is off)")
        } else {
            format!("plan {id}, confirmed")
        }),
        Err(reason) => Err(reason),
    }
}

/// `queue-status QUEUE [--check]`: the entries and what became of each. `--check` plans every job that is still to be made and says whether it could be run now; the exit code is 2 when one
/// could not.
pub fn queue_status(mut args: Vec<String>) -> Result<(), String> {
    let check = switch(&mut args, "--check");
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("queue-status needs a queue file")?;
    let q = load(path, lang)?;
    let dir = queue_dir(path);
    print!("{}", q.text());
    if !check {
        return Ok(());
    }
    let mut blocked = 0;
    for e in q.to_do() {
        match standing(&q, e, cfg.queue_confirm, &dir) {
            Ok(s) => println!("{}: can run - {s}", e.name),
            Err(reason) => {
                blocked += 1;
                println!("{}: cannot run - {reason}", e.name);
            }
        }
    }
    if blocked > 0 {
        fail(format!("{blocked} entries cannot be run now"))
    } else {
        Ok(())
    }
}

/// `queue-confirm QUEUE NAME`: records that the person looked at the plan of the entry as it is now and wants it run. When the files or the plan change, the confirmation is void.
pub fn queue_confirm(mut args: Vec<String>) -> Result<(), String> {
    let held = held_to(&mut args)?;
    let (lang, _) = language_and_config(&mut args)?;
    finish(&args, 2)?;
    let [path, name] = args.as_slice() else {
        return Err("queue-confirm needs a queue file and the name of an entry".into());
    };
    let mut q = load(path, lang)?;
    let e = q
        .get(name)
        .ok_or_else(|| format!("there is no entry '{name}' in {path}"))?
        .clone();
    let id = plan_identity(e.arguments_in(&queue_dir(path)))?;
    q.confirm(name, &id).map_err(|e| e.localized(lang))?;
    store_held(path, &q, lang, held.as_deref())?;
    println!("confirmed '{name}' for the plan {id}");
    Ok(())
}

/// `queue-edit QUEUE NAME skip|reset|revoke|remove|up|down|top|bottom`: leaves an entry out, puts it back to the start, takes its confirmation back, takes it out of the queue or
/// moves it in the order (what is running or made keeps its place).
pub fn queue_edit(mut args: Vec<String>) -> Result<(), String> {
    let held = held_to(&mut args)?;
    let (lang, _) = language_and_config(&mut args)?;
    finish(&args, 3)?;
    let [path, name, action] = args.as_slice() else {
        return Err(
            "queue-edit needs a queue file, the name of an entry and skip, reset, revoke, remove, up, down, top or bottom"
                .into(),
        );
    };
    let mut q = load(path, lang)?;
    let l = |e: PnpError| e.localized(lang);
    match action.as_str() {
        "skip" => q.skip(name).map_err(l)?,
        "reset" => q.reset(name).map_err(l)?,
        "revoke" => q.revoke(name).map_err(l)?,
        "remove" => q.remove(name).map_err(l)?,
        other => match eh3d_phoenix_pnp::queue::Move::parse(other) {
            Some(to) => {
                let place = q.move_entry(name, to).map_err(l)?;
                store_held(path, &q, lang, held.as_deref())?;
                println!("{action}: '{name}' is now number {}", place + 1);
                return Ok(());
            }
            None => {
                return Err(format!(
                    "'{other}' is not skip, reset, revoke, remove, up, down, top or bottom"
                ))
            }
        },
    }
    store_held(path, &q, lang, held.as_deref())?;
    println!("{action}: '{name}'");
    Ok(())
}

/// A file that says that a queue is being run, so that two runs do not write the same queue; it is taken away when the run ends.
struct Lock(String);

impl Lock {
    fn take(queue_path: &str) -> Result<Lock, String> {
        let path = format!("{queue_path}.lock");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| Lock(path.clone()))
            .map_err(|_| {
                format!("{path} exists: another queue-run is under way, or one stopped without finishing (when none is running, take the file away)")
            })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `queue-run QUEUE [--only NAME]`: makes the jobs that are still to be made, in order, a board at a time on the simulated machine. A job runs only when its plan is confirmed
/// (`queue_confirm`, with `--config` for the settings of the queue); the queue is written after every board; a job that fails stops the queue unless `queue_stop_on_failure` is off.
/// The exit code is 2 when a job failed or was not run.
pub fn queue_run(mut args: Vec<String>) -> Result<(), String> {
    let only = flag(&mut args, "--only")?;
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("queue-run needs a queue file")?.clone();
    let _lock = Lock::take(&path)?;
    let mut q = load(&path, lang)?;
    let dir = queue_dir(&path);
    let names: Vec<String> = q
        .to_do()
        .filter(|e| only.as_ref().is_none_or(|o| *o == e.name))
        .map(|e| e.name.clone())
        .collect();
    if names.is_empty() {
        println!("nothing to run");
        return Ok(());
    }
    let l = |e: PnpError| e.localized(lang);
    let mut problems = 0;
    'jobs: for name in names {
        let e = q.get(&name).cloned().ok_or("the entry is gone")?;
        for _ in e.done..e.boards {
            // the plan is looked at again before every board: a file that changed voids the confirmation
            let id = match plan_identity(e.arguments_in(&dir)) {
                Ok(id) => id,
                Err(msg) => {
                    q.set_state(&name, State::Failed, &msg).map_err(l)?;
                    store(&path, &q, lang)?;
                    println!("{name}: failed - {msg}");
                    problems += 1;
                    if cfg.queue_stop_on_failure {
                        break 'jobs;
                    }
                    continue 'jobs;
                }
            };
            if let Err(reason) = q.allowed(&name, &id, cfg.queue_confirm) {
                println!("{name}: not run - {reason}");
                problems += 1;
                if cfg.queue_stop_on_failure {
                    break 'jobs;
                }
                continue 'jobs;
            }
            q.set_state(&name, State::Running, "a board is under way")
                .map_err(l)?;
            store(&path, &q, lang)?;
            match simulate(e.arguments_in(&dir)) {
                Ok(()) => {
                    q.board_done(&name).map_err(l)?;
                    store(&path, &q, lang)?;
                }
                Err(msg) => {
                    q.set_state(&name, State::Failed, &msg).map_err(l)?;
                    store(&path, &q, lang)?;
                    println!("{name}: failed - {msg}");
                    problems += 1;
                    if cfg.queue_stop_on_failure {
                        break 'jobs;
                    }
                    continue 'jobs;
                }
            }
        }
        println!("{name}: done");
    }
    print!("{}", q.text());
    if problems > 0 {
        fail(format!("{problems} jobs failed or were not run"))
    } else {
        Ok(())
    }
}
