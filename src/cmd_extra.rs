// EH3D-PHOENIX-PNP - the commands toolpath-hash, height-map, equivalents and drag-plan.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine, and a file named by --out that exists is never written over. They end with the exit code 2 when a
// check of theirs is not passed (a job that is not the one that was authorised, a board that bends too much, a part with no feeder, a tape that cannot be dragged) and with 1 when the
// arguments or the files are wrong.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_drive::load_profile;
use crate::cmd_flow::deliver;
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::feeders::assign_side;
use eh3d_phoenix_pnp::msg8::{num as show, say};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::plan::{plan, Settings};
use eh3d_phoenix_pnp::{drag, equivalents, feeding, heightmap, toolpath};

/// `toolpath-hash PARTS FEEDERS PROFILE [--side top|bottom] [--board-width MM] [--partial] [--expect HASH] [--script-out F]`: plans the board, writes the lines that the profile gives for
/// the whole job and prints their SHA-256 as `lines_sha256`. With `--expect` the command ends with 2 when the lines are not the ones of that hash. It is a local check: a job of the
/// server is authorised with the hash of the server (`server-job ID --authorize`), never with this one.
pub fn toolpath_hash(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let expect = flag(&mut args, "--expect")?;
    let script_out = flag(&mut args, "--script-out")?;
    let partial = switch(&mut args, "--partial");
    let side = match flag(&mut args, "--side")?.as_deref() {
        None | Some("top") => Side::Top,
        Some("bottom") => Side::Bottom,
        Some(o) => return Err(format!("--side '{o}' is not top or bottom")),
    };
    let width = match flag(&mut args, "--board-width")? {
        Some(v) => Some(num(&v, "--board-width")?),
        None => None,
    };
    finish(&args, 3)?;
    if args.len() < 3 {
        return Err("toolpath-hash needs the parts, the feeders and a profile".into());
    }
    if let Some(e) = &expect {
        if !toolpath::is_fingerprint(e.trim()) {
            return Err(format!("--expect '{e}' is not 64 hexadecimal digits"));
        }
    }
    let profile = load_profile(&args[2], lang)?;
    let parts = parse(&read_text(&args[0], cfg.max_file_bytes)?)?;
    let feeders = eh3d_phoenix_pnp::feeders::from_csv(&read_text(&args[1], cfg.max_file_bytes)?)?;
    let assigned = assign_side(&parts, &feeders, Some(side));
    if (!assigned.without_feeder.is_empty() || !assigned.short.is_empty()) && !partial {
        return Err(
            "the feeders do not cover the board (use --partial to place only what can be placed)"
                .into(),
        );
    }
    let settings = Settings {
        nozzles: profile.nozzles,
        side,
        board_width_mm: width,
        ..Settings::default()
    };
    let planned = plan(&parts, &feeders, &assigned, &settings)?;
    let lines = toolpath::script(&profile, &planned.placements)?;
    let hash = toolpath::fingerprint(&lines);
    let row: [&str; 8] = [
        "lines_sha256: {hash} ({n} lines, {p} placements)",
        "lines_sha256: {hash} ({n} líneas, {p} colocaciones)",
        "lines_sha256: {hash} ({n} Zeilen, {p} Bestückungen)",
        "lines_sha256 : {hash} ({n} lignes, {p} placements)",
        "lines_sha256: {hash} ({n} righe, {p} posizionamenti)",
        "lines_sha256: {hash}（{n} 行、{p} 件の実装）",
        "lines_sha256：{hash}（{n} 行，{p} 个贴装）",
        "lines_sha256: {hash} ({n} linhas, {p} colocações)",
    ];
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("hash", hash),
                ("n", lines.len().to_string()),
                ("p", planned.placements.len().to_string())
            ]
        )
    );
    if let Some(o) = &script_out {
        let inputs = vec![args[0].clone(), args[1].clone(), args[2].clone()];
        deliver(&toolpath::text(&lines), Some(o), &inputs, lang)?;
    }
    let evidence: [&str; 8] = [
        "evidence=simulator: the lines were written from the profile and nothing was sent to a controller; this is a local check of the lines of this program, never the hash that authorises a job of the server (server-status JOB shows that one)",
        "evidence=simulator: las líneas se escribieron desde el perfil y no se envió nada a un controlador; es una comprobación local de las líneas de este programa, nunca la huella que autoriza un trabajo del servidor (esa la muestra server-status JOB)",
        "evidence=simulator: die Zeilen wurden aus dem Profil geschrieben und nichts an eine Steuerung gesendet; das ist eine örtliche Prüfung der Zeilen dieses Programms, niemals der Hash, der einen Auftrag des Servers genehmigt (den zeigt server-status JOB)",
        "evidence=simulator : les lignes ont été écrites depuis le profil et rien n'a été envoyé à un contrôleur ; c'est une vérification locale des lignes de ce programme, jamais l'empreinte qui autorise un travail du serveur (server-status JOB montre celle-là)",
        "evidence=simulator: le righe sono state scritte dal profilo e non è stato inviato nulla a un controllore; è un controllo locale delle righe di questo programma, mai l'impronta che autorizza un lavoro del server (quella la mostra server-status JOB)",
        "evidence=simulator：プロファイルから行を書き出しただけで、コントローラには何も送っていません。これはこのプログラムの行をローカルで確認するためのもので、サーバーのジョブを承認するハッシュではありません（そちらは server-status JOB で表示します）",
        "evidence=simulator：仅根据配置文件写出了这些行，没有向控制器发送任何内容；这只是对本程序所写行的本地校验，绝不是用于授权服务器作业的哈希（后者由 server-status JOB 显示）",
        "evidence=simulator: as linhas foram escritas a partir do perfil e nada foi enviado a um controlador; é uma verificação local das linhas deste programa, nunca a impressão que autoriza um trabalho do servidor (essa é mostrada por server-status JOB)",
    ];
    println!("{}", say(lang, &evidence, &[]));
    if let Some(e) = expect {
        if !toolpath::matches(&lines, &e) {
            let no: [&str; 8] = [
                "the lines of the job are not the ones that were authorised: they give {got} and {want} was expected",
                "las líneas del trabajo no son las que se autorizaron: dan {got} y se esperaba {want}",
                "die Zeilen des Auftrags sind nicht die genehmigten: sie ergeben {got}, erwartet war {want}",
                "les lignes du travail ne sont pas celles qui ont été autorisées : elles donnent {got} et {want} était attendu",
                "le righe del lavoro non sono quelle autorizzate: danno {got} e ci si aspettava {want}",
                "ジョブの行は承認されたものと違います：{got} になり、期待は {want} でした",
                "作业的行与已授权的不同：得到 {got}，期望 {want}",
                "as linhas do trabalho não são as que foram autorizadas: dão {got} e esperava-se {want}",
            ];
            return fail(say(
                lang,
                &no,
                &[
                    ("got", toolpath::fingerprint(&lines)),
                    ("want", e.trim().to_ascii_lowercase()),
                ],
            ));
        }
    }
    Ok(())
}

fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (a, b) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} {text} is not X,Y"))?;
    Ok((num(a, what)?, num(b, what)?))
}

/// `height-map GRID [--parts PARTS] [--zero X,Y] [--out F]`: reads a grid of measured heights (`x_mm,y_mm,z_mm`), says how much the board bends and, with `--parts`, how much the height of
/// each placement changes (`--out` writes the table). The exit code is 2 when the board bends more than `heightmap_max_warp_mm` or a part is outside the measured grid.
pub fn height_map(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let parts_file = flag(&mut args, "--parts")?;
    let zero = match flag(&mut args, "--zero")? {
        Some(v) => Some(pair(&v, "--zero")?),
        None => None,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("height-map needs a table of heights")?
        .clone();
    if out.is_some() && parts_file.is_none() {
        return Err("--out writes the corrections of the parts: give them with --parts".into());
    }
    let grid = heightmap::parse(&read_text(&path, heightmap::MAX_BYTES)?)
        .map_err(|e| format!("{path}: {e}"))?;
    let warp = grid.warp();
    let row: [&str; 8] = [
        "{n} points ({c} columns by {r} rows); the board bends {w} mm (the most allowed is {max} mm)",
        "{n} puntos ({c} columnas por {r} filas); la placa se alabea {w} mm (lo máximo permitido es {max} mm)",
        "{n} Punkte ({c} Spalten mal {r} Zeilen); die Platine wölbt sich um {w} mm (erlaubt sind höchstens {max} mm)",
        "{n} points ({c} colonnes sur {r} lignes) ; la carte se déforme de {w} mm (le maximum permis est {max} mm)",
        "{n} punti ({c} colonne per {r} righe); la scheda si imbarca di {w} mm (il massimo consentito è {max} mm)",
        "{n} 点（{c} 列 × {r} 行）。基板の反りは {w} mm（許容上限は {max} mm）",
        "{n} 个点（{c} 列 × {r} 行）；板子翘曲 {w} mm（允许的最大值为 {max} mm）",
        "{n} pontos ({c} colunas por {r} linhas); a placa empena {w} mm (o máximo permitido é {max} mm)",
    ];
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("n", grid.points().to_string()),
                ("c", grid.xs.len().to_string()),
                ("r", grid.ys.len().to_string()),
                ("w", format!("{warp:.3}")),
                ("max", show(cfg.heightmap_max_warp_mm)),
            ]
        )
    );
    let mut problems: Vec<String> = Vec::new();
    if warp > cfg.heightmap_max_warp_mm {
        let t: [&str; 8] = [
            "the board bends {w} mm, more than the {max} mm that heightmap_max_warp_mm allows",
            "la placa se alabea {w} mm, más de los {max} mm que permite heightmap_max_warp_mm",
            "die Platine wölbt sich um {w} mm, mehr als die {max} mm, die heightmap_max_warp_mm erlaubt",
            "la carte se déforme de {w} mm, plus que les {max} mm permis par heightmap_max_warp_mm",
            "la scheda si imbarca di {w} mm, più dei {max} mm consentiti da heightmap_max_warp_mm",
            "基板の反りは {w} mm で、heightmap_max_warp_mm が許す {max} mm を超えています",
            "板子翘曲 {w} mm，超过 heightmap_max_warp_mm 允许的 {max} mm",
            "a placa empena {w} mm, mais do que os {max} mm que heightmap_max_warp_mm permite",
        ];
        problems.push(say(
            lang,
            &t,
            &[
                ("w", format!("{warp:.3}")),
                ("max", show(cfg.heightmap_max_warp_mm)),
            ],
        ));
    }
    if let Some(pf) = &parts_file {
        let parts = parse(&read_text(pf, cfg.max_file_bytes)?)?;
        let zero = zero.unwrap_or_else(|| grid.default_zero());
        let list = heightmap::corrections(&grid, &parts, zero)?;
        let outside: Vec<&str> = list
            .iter()
            .filter(|c| c.dz_mm.is_none())
            .map(|c| c.reference.as_str())
            .collect();
        let known: Vec<f64> = list.iter().filter_map(|c| c.dz_mm).collect();
        if let (Some(lo), Some(hi)) = (
            known.iter().copied().reduce(f64::min),
            known.iter().copied().reduce(f64::max),
        ) {
            let t: [&str; 8] = [
                "the height of the placements changes from {lo} to {hi} mm against the zero at {x},{y}",
                "la altura de las colocaciones cambia de {lo} a {hi} mm respecto al cero en {x},{y}",
                "die Höhe der Bestückungen ändert sich von {lo} bis {hi} mm gegenüber dem Nullpunkt bei {x},{y}",
                "la hauteur des placements varie de {lo} à {hi} mm par rapport au zéro en {x},{y}",
                "l'altezza dei posizionamenti cambia da {lo} a {hi} mm rispetto allo zero in {x},{y}",
                "実装の高さは、{x},{y} のゼロ点に対して {lo} から {hi} mm の間で変わります",
                "贴装高度相对于 {x},{y} 处的零点在 {lo} 到 {hi} mm 之间变化",
                "a altura das colocações varia de {lo} a {hi} mm em relação ao zero em {x},{y}",
            ];
            let round = |v: f64| if v.abs() < 0.0005 { 0.0 } else { v };
            println!(
                "{}",
                say(
                    lang,
                    &t,
                    &[
                        ("lo", format!("{:.3}", round(lo))),
                        ("hi", format!("{:.3}", round(hi))),
                        ("x", show(zero.0)),
                        ("y", show(zero.1)),
                    ]
                )
            );
        }
        if !outside.is_empty() {
            let t: [&str; 8] = [
                "{n} parts are outside the measured grid and have no correction: {list}",
                "{n} piezas están fuera de la cuadrícula medida y no tienen corrección: {list}",
                "{n} Teile liegen außerhalb des gemessenen Rasters und haben keine Korrektur: {list}",
                "{n} pièces sont hors de la grille mesurée et n'ont pas de correction : {list}",
                "{n} componenti sono fuori dalla griglia misurata e non hanno correzione: {list}",
                "{n} 個の部品が測定グリッドの外にあり、補正がありません：{list}",
                "{n} 个元件在已测网格之外，没有校正：{list}",
                "{n} peças estão fora da grelha medida e não têm correção: {list}",
            ];
            let shown: Vec<&str> = outside.iter().take(8).copied().collect();
            let mut names = shown.join(", ");
            if outside.len() > shown.len() {
                names.push_str(", ...");
            }
            problems.push(say(
                lang,
                &t,
                &[("n", outside.len().to_string()), ("list", names)],
            ));
        }
        if let Some(o) = &out {
            let inputs = vec![path.clone(), pf.clone()];
            deliver(&heightmap::csv(&list), Some(o), &inputs, lang)?;
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    for p in &problems[1..] {
        eprintln!("{p}");
    }
    fail(problems[0].clone())
}

/// `equivalents PARTS FEEDERS GROUPS [--out F]`: for the parts that no feeder holds, the feeder that holds an equivalent part (the table of groups says which) and has parts left. `--out`
/// writes the table of parts with the swaps made. The exit code is 2 when a part still has no feeder.
pub fn equivalents_cmd(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 3)?;
    if args.len() < 3 {
        return Err("equivalents needs the parts, the feeders and the table of groups".into());
    }
    let parts = parse(&read_text(&args[0], cfg.max_file_bytes)?)?;
    let specs = feeding::from_csv(&read_text(&args[1], cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let feeders: Vec<_> = specs
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.base.clone())
        .collect();
    let groups = equivalents::Groups::parse(&read_text(&args[2], cfg.max_file_bytes)?)
        .map_err(|e| format!("{}: {e}", args[2]))?;
    let outcome = equivalents::resolve(&parts, &feeders, &groups);
    for s in &outcome.swaps {
        println!(
            "{}: {} {} -> {} ({} {})",
            s.reference, s.value, s.package, s.feeder, s.feeder_value, s.feeder_package
        );
    }
    let row: [&str; 8] = [
        "{own} parts have a feeder of their own; {swaps} go on a feeder that holds an equivalent part, so {saved} reels are not needed; {missing} have no feeder",
        "{own} piezas tienen alimentador propio; {swaps} van a un alimentador con una pieza equivalente, así que no hacen falta {saved} bobinas; {missing} no tienen alimentador",
        "{own} Teile haben eine eigene Zuführung; {swaps} kommen auf eine Zuführung mit einem gleichwertigen Teil, so entfallen {saved} Rollen; {missing} haben keine Zuführung",
        "{own} pièces ont leur propre alimentateur ; {swaps} vont sur un alimentateur qui porte une pièce équivalente, donc {saved} bobines sont évitées ; {missing} n'ont pas d'alimentateur",
        "{own} componenti hanno un feeder proprio; {swaps} vanno su un feeder con un componente equivalente, quindi {saved} bobine non servono; {missing} non hanno un feeder",
        "{own} 個の部品には専用のフィーダーがあります。{swaps} 個は同等の部品を持つフィーダーを使うため、リール {saved} 本が不要です。フィーダーのない部品は {missing} 個です",
        "{own} 个元件有自己的供料器；{swaps} 个使用装有等效元件的供料器，因此省去 {saved} 卷料；{missing} 个没有供料器",
        "{own} peças têm alimentador próprio; {swaps} vão para um alimentador com uma peça equivalente, pelo que {saved} bobinas não são precisas; {missing} não têm alimentador",
    ];
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("own", outcome.own.to_string()),
                ("swaps", outcome.swaps.len().to_string()),
                ("saved", outcome.reels_saved().to_string()),
                ("missing", outcome.missing.len().to_string()),
            ]
        )
    );
    if let Some(o) = &out {
        let inputs = vec![args[0].clone(), args[1].clone(), args[2].clone()];
        deliver(
            &equivalents::parts_csv(&parts, &outcome),
            Some(o),
            &inputs,
            lang,
        )?;
    }
    if outcome.missing.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "{} parts have no feeder: {}",
            outcome.missing.len(),
            outcome.missing.join(", ")
        ))
    }
}

/// `drag-plan FEEDERS [--out F]`: for each strip of the table of feeders, the stroke of a drag feeder (a whole number of sprocket holes and of pockets), the picks that one stroke serves and
/// the strokes the tape takes. The exit code is 2 when a strip cannot be dragged.
pub fn drag_plan(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("drag-plan needs a table of feeders")?
        .clone();
    let specs =
        feeding::from_csv(&read_text(&path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    let result = drag::plan(
        &specs,
        cfg.drag_hole_pitch_mm,
        u64::from(cfg.drag_max_picks_per_stroke),
    );
    let row: [&str; 8] = [
        "{id}: pockets {p} mm apart, a stroke of {s} mm ({h} holes) serves {k} picks, {n} strokes for the tape",
        "{id}: bolsillos a {p} mm, una pasada de {s} mm ({h} agujeros) sirve {k} recogidas, {n} pasadas para la cinta",
        "{id}: Taschen {p} mm auseinander, ein Hub von {s} mm ({h} Löcher) bedient {k} Aufnahmen, {n} Hübe für den Gurt",
        "{id} : alvéoles espacés de {p} mm, une course de {s} mm ({h} trous) sert {k} prises, {n} courses pour la bande",
        "{id}: tasche a {p} mm, una corsa di {s} mm ({h} fori) serve {k} prelievi, {n} corse per il nastro",
        "{id}：ポケット間隔 {p} mm、ストローク {s} mm（{h} 穴）で {k} 回ピックし、テープ全体で {n} ストロークです",
        "{id}：料穴间距 {p} mm，一次拖动 {s} mm（{h} 个孔）可供 {k} 次取料，整盘料带需 {n} 次拖动",
        "{id}: bolsos a {p} mm, um curso de {s} mm ({h} furos) serve {k} recolhas, {n} cursos para a fita",
    ];
    for d in &result.rows {
        println!(
            "{}",
            say(
                lang,
                &row,
                &[
                    ("id", d.id.clone()),
                    ("p", show(d.pocket_pitch_mm)),
                    ("s", show(d.stroke_mm)),
                    ("h", d.holes_per_stroke.to_string()),
                    ("k", d.picks_per_stroke.to_string()),
                    ("n", d.strokes.to_string()),
                ]
            )
        );
    }
    for (id, why) in &result.refused {
        println!("{id}: {why}");
    }
    let sum: [&str; 8] = [
        "{ok} strips can be dragged, {no} cannot; {other} feeders are not strips and were not looked at (holes {h} mm apart)",
        "{ok} tiras se pueden arrastrar, {no} no; {other} alimentadores no son tiras y no se miraron (agujeros a {h} mm)",
        "{ok} Streifen lassen sich ziehen, {no} nicht; {other} Zuführungen sind keine Streifen und wurden nicht geprüft (Löcher {h} mm auseinander)",
        "{ok} bandes peuvent être tirées, {no} non ; {other} alimentateurs ne sont pas des bandes et n'ont pas été regardés (trous espacés de {h} mm)",
        "{ok} strisce si possono trascinare, {no} no; {other} feeder non sono strisce e non sono stati guardati (fori a {h} mm)",
        "ドラッグできるストリップは {ok} 本、できないものは {no} 本です。ストリップでない {other} 個のフィーダーは調べていません（穴の間隔 {h} mm）",
        "{ok} 条料带可以拖动，{no} 条不能；{other} 个供料器不是料带，未检查（孔距 {h} mm）",
        "{ok} tiras podem ser arrastadas, {no} não; {other} alimentadores não são tiras e não foram vistos (furos a {h} mm)",
    ];
    println!(
        "{}",
        say(
            lang,
            &sum,
            &[
                ("ok", result.rows.len().to_string()),
                ("no", result.refused.len().to_string()),
                ("other", result.other.to_string()),
                ("h", show(cfg.drag_hole_pitch_mm)),
            ]
        )
    );
    if let Some(o) = &out {
        let inputs = vec![path.clone()];
        deliver(&drag::csv(&result), Some(o), &inputs, lang)?;
    }
    if result.refused.is_empty() {
        Ok(())
    } else {
        fail(format!("{} strips cannot be dragged", result.refused.len()))
    }
}
