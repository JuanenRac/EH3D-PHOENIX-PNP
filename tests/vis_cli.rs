//! The commands `part-inspect`, `fiducial-check` and `feeder-check`, and the report of the feeders of `simulate`, run as the program.

use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_vis_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn part_inspect_prints_the_verdicts_and_fails_while_a_part_is_not_accepted() {
    let o = run(&[
        "part-inspect",
        "examples/nozzle_reads.csv",
        "--sizes",
        "examples/package_sizes.csv",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let out = text(&o.stdout);
    assert!(out.contains("R1,R_0603,ok,"), "{out}");
    assert!(out.contains("C1,C_0603,reject,"), "{out}");
    assert!(out.contains("D9,DO-214,unknown,"), "{out}");
    let err = text(&o.stderr);
    assert!(
        err.contains("4 accepted, 1 refused, 1 without a size"),
        "{err}"
    );
}

#[test]
fn part_inspect_uses_the_limits_of_the_configuration_and_writes_a_new_file_only() {
    let dir = scratch("inspect");
    let cfg = dir.join("loose.cfg");
    std::fs::write(&cfg, "part_size_tol_mm = 5\n").unwrap();
    let out_file = dir.join("result.csv");
    let args = [
        "part-inspect",
        "examples/nozzle_reads.csv",
        "--sizes",
        "examples/package_sizes.csv",
        "--config",
        cfg.to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
    ];
    let o = run(&args);
    // C1 is accepted by the loose limit; D9 still has no size
    assert!(
        text(&o.stderr).contains("5 accepted, 0 refused, 1 without"),
        "{}",
        text(&o.stderr)
    );
    assert!(std::fs::read_to_string(&out_file)
        .unwrap()
        .contains("C1,C_0603,ok,"));
    let again = run(&args);
    assert_eq!(again.status.code(), Some(1));
    assert!(
        text(&again.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&again.stderr)
    );
    // the output is never one of the inputs
    let same = run(&[
        "part-inspect",
        "examples/nozzle_reads.csv",
        "--sizes",
        "examples/package_sizes.csv",
        "--out",
        "examples/nozzle_reads.csv",
    ]);
    assert_eq!(same.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn part_inspect_needs_its_sizes_and_refuses_options_it_does_not_know() {
    let o = run(&["part-inspect", "examples/nozzle_reads.csv"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("--sizes"), "{}", text(&o.stderr));
    let o = run(&[
        "part-inspect",
        "examples/nozzle_reads.csv",
        "--sizes",
        "examples/package_sizes.csv",
        "--nope",
    ]);
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn fiducial_check_takes_the_example_marks_apart_and_accepts_them() {
    let o = run(&[
        "fiducial-check",
        "examples/fid_design.csv",
        "examples/fid_measured.csv",
    ]);
    let out = text(&o.stdout);
    assert!(o.status.success(), "{out}{}", text(&o.stderr));
    assert!(out.contains("turn +0.300 degrees"), "{out}");
    assert!(out.contains("check: ok"), "{out}");
}

#[test]
fn fiducial_check_refuses_a_panel_that_is_turned_too_far_and_a_mark_that_is_off() {
    let dir = scratch("fid");
    let turned = dir.join("turned.csv");
    // a turn of 10 degrees about the origin
    let (s, c) = 10f64.to_radians().sin_cos();
    let mut text_turned = String::from("id,x,y\n");
    for (id, x, y) in [
        ("f1", -5.0, -5.0),
        ("f2", 105.0, -5.0),
        ("f3", -5.0, 65.0),
        ("f4", 105.0, 65.0),
    ] {
        text_turned.push_str(&format!("{id},{:.4},{:.4}\n", x * c - y * s, x * s + y * c));
    }
    std::fs::write(&turned, text_turned).unwrap();
    let o = run(&[
        "fiducial-check",
        "examples/fid_design.csv",
        turned.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stdout).contains("check: turned +10.00 degrees"),
        "{}",
        text(&o.stdout)
    );
    // with a looser limit it passes
    let cfg = dir.join("loose.cfg");
    std::fs::write(
        &cfg,
        "fiducial_max_rotation_deg = 20\nfiducial_max_shift_mm = 100\n",
    )
    .unwrap();
    let o = run(&[
        "fiducial-check",
        "examples/fid_design.csv",
        turned.to_str().unwrap(),
        "--config",
        cfg.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o.stdout));
    // a mark far from where the others put it
    let off = dir.join("off.csv");
    std::fs::write(&off, "id,x,y\nf1,-5,-5\nf2,105,-5\nf3,-5,65\nf4,110,65\n").unwrap();
    let o = run(&[
        "fiducial-check",
        "examples/fid_design.csv",
        off.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("from where the fit puts it"),
        "{}",
        text(&o.stderr)
    );
    // a mark that the design does not have
    let extra = dir.join("extra.csv");
    std::fs::write(&extra, "id,x,y\nzz,1,1\nf1,-5,-5\n").unwrap();
    let o = run(&[
        "fiducial-check",
        "examples/fid_design.csv",
        extra.to_str().unwrap(),
    ]);
    assert!(
        text(&o.stderr).contains("'zz' is not in the design"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn feeder_check_accepts_the_example_and_fails_on_a_clash() {
    let o = run(&[
        "feeder-check",
        "examples/feeders_kinds.csv",
        "--packages",
        "examples/packages.csv",
    ]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains("0 errors, 0 warnings"),
        "{}",
        text(&o.stdout)
    );
    let dir = scratch("feed");
    let clash = dir.join("clash.csv");
    std::fs::write(
        &clash,
        "id,value,package,x,y,count\nF1,10k,R_0603,0,0,10\nF2,10k,R_0603,1,0,10\n",
    )
    .unwrap();
    let o = run(&["feeder-check", clash.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stdout).contains("error: feeder F2:"),
        "{}",
        text(&o.stdout)
    );
    // a gap of 0 does not check, and the clash is gone
    let cfg = dir.join("gap.cfg");
    std::fs::write(&cfg, "feeder_min_gap_mm = 0\n").unwrap();
    let o = run(&[
        "feeder-check",
        clash.to_str().unwrap(),
        "--config",
        cfg.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o.stdout));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_writes_the_report_of_the_feeders_once() {
    let dir = scratch("sim");
    let out_file = dir.join("feeders.csv");
    let args = [
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        "examples/machine.cfg",
        "--packages",
        "examples/packages.csv",
        "--report-feeders",
        out_file.to_str().unwrap(),
    ];
    let o = run(&args);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let written = std::fs::read_to_string(&out_file).unwrap();
    assert!(
        written.starts_with("feeder,planned_picks,planned_busy_s,picked,pick_failures,fail_percent,faults,disabled,flagged,placed,busy_s\n"),
        "{written}"
    );
    assert!(written.lines().count() > 1, "{written}");
    let again = run(&args);
    assert_eq!(again.status.code(), Some(1));
    assert!(
        text(&again.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&again.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn part_inspect_takes_the_sizes_from_the_bodies_of_the_table_of_packages() {
    let o = run(&[
        "part-inspect",
        "examples/nozzle_reads.csv",
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let out = text(&o.stdout);
    assert!(out.contains("R1,R_0603,ok,"), "{out}");
    assert!(out.contains("C1,C_0603,reject,"), "{out}");
    assert!(out.contains("D9,DO-214,unknown,"), "{out}");
    assert!(
        text(&o.stderr).contains("4 accepted, 1 refused, 1 without a size"),
        "{}",
        text(&o.stderr)
    );
    // with neither table there is nothing to judge against
    let none = run(&["part-inspect", "examples/nozzle_reads.csv"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(
        text(&none.stderr).contains("--sizes"),
        "{}",
        text(&none.stderr)
    );
}
