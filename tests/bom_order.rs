//! The list of parts to buy: the rows that are the same part added together, the spare, the parts not to be mounted, the settings and the command `bom-order`.

use eh3d_phoenix_pnp::bom::{order, order_csv, Bom, MAX_BOM_ROWS, MAX_ORDER_BOARDS};
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use std::path::PathBuf;
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const EXAMPLE: &str = include_str!("../examples/bom.csv");

fn bom(text: &str) -> Bom {
    Bom::parse(text, MAX_BOM_ROWS).unwrap()
}

#[test]
fn the_example_needs_seven_parts_in_four_lines_for_one_board() {
    let lines = order(&bom(EXAMPLE), 1, 0.0, 0, &[]).unwrap();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines.iter().map(|l| l.total).sum::<usize>(), 7);
    let r = lines.iter().find(|l| l.value == "10k").unwrap();
    assert_eq!((r.per_board, r.boards, r.spare, r.total), (3, 1, 0, 3));
    assert_eq!(r.references, ["R1", "R2", "R3"]);
    assert_eq!(r.part_number, "RC0603FR-0710KL");
}

#[test]
fn the_boards_multiply_and_the_spare_is_the_larger_of_the_per_cent_and_the_least() {
    let b = bom("Designator,Comment,Footprint\n\"R1,R2,R3\",10k,R_0603\nU1,MCU,QFN-32\n");
    let l = order(&b, 10, 5.0, 0, &[]).unwrap();
    // 30 resistors: 5% is 1.5, rounded up to 2; one chip on ten boards: 10 and 0.5 up to 1
    let r = l.iter().find(|x| x.value == "10k").unwrap();
    assert_eq!((r.per_board * r.boards, r.spare, r.total), (30, 2, 32));
    let u = l.iter().find(|x| x.value == "MCU").unwrap();
    assert_eq!((u.spare, u.total), (1, 11));
    let l = order(&b, 10, 5.0, 5, &[]).unwrap();
    assert_eq!(l.iter().map(|x| x.spare).collect::<Vec<_>>(), [5, 5]);
    // the least alone
    let l = order(&b, 1, 0.0, 3, &[]).unwrap();
    assert_eq!(l.iter().map(|x| x.total).sum::<usize>(), 10);
}

#[test]
fn rows_that_are_the_same_part_are_added_together_by_part_number_or_by_value_and_package() {
    let b = bom("Designator,Comment,Footprint,Part Number\nR1,10k,R_0603,RC-1\nR2,10 kΩ,R_0603,\nR3,10K,r_0603,\nR4,10k,R_0402,RC-1\nR5,22k,R_0603,\n");
    let l = order(&b, 1, 0.0, 0, &[]).unwrap();
    // R1 and R4 are one part number (the package of the first is kept); R2 and R3 are one value and package; R5 is alone
    assert_eq!(l.len(), 3);
    let rc = l.iter().find(|x| x.part_number == "RC-1").unwrap();
    assert_eq!(
        (rc.per_board, rc.references.as_slice()),
        (2, ["R1".to_string(), "R4".to_string()].as_slice())
    );
    let tens = l
        .iter()
        .find(|x| x.part_number.is_empty() && x.value.starts_with("10"))
        .unwrap();
    assert_eq!(tens.per_board, 2);
    assert_eq!(l.iter().find(|x| x.value == "22k").unwrap().per_board, 1);
}

#[test]
fn the_parts_not_to_be_mounted_and_the_references_listed_twice_are_not_counted() {
    let b = bom(
        "Designator,Comment,Footprint\n\"R1,R2,R3\",10k,R_0603\nR1,10k,R_0603\nD1,LED,LED_0603\n",
    );
    let l = order(&b, 1, 0.0, 0, &["R2".to_string(), "D1".to_string()]).unwrap();
    // R2 and D1 are left out; R1 is counted once; the LED line has no part left and goes
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].references, ["R1", "R3"]);
    assert_eq!(l[0].total, 2);
}

#[test]
fn the_lines_are_sorted_by_package_then_value() {
    let b = bom("Designator,Comment,Footprint\nU1,MCU,QFN-32\nR1,22k,R_0603\nC1,100n,C_0603\nR2,10k,R_0603\n");
    let l = order(&b, 1, 0.0, 0, &[]).unwrap();
    let got: Vec<(&str, &str)> = l
        .iter()
        .map(|x| (x.package.as_str(), x.value.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("C_0603", "100n"),
            ("QFN-32", "MCU"),
            ("R_0603", "10k"),
            ("R_0603", "22k")
        ]
    );
}

#[test]
fn the_numbers_of_an_order_are_checked() {
    let b = bom(EXAMPLE);
    for (boards, percent, least, needle) in [
        (0, 0.0, 0, "number of boards"),
        (MAX_ORDER_BOARDS + 1, 0.0, 0, "number of boards"),
        (1, -1.0, 0, "spare per cent"),
        (1, 100.5, 0, "spare per cent"),
        (1, f64::NAN, 0, "spare per cent"),
        (1, 0.0, 1001, "least spare"),
    ] {
        let e = order(&b, boards, percent, least, &[]).unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{needle}");
        assert!(e.detail.contains(needle), "{needle}: {}", e.detail);
    }
    assert!(order(&b, MAX_ORDER_BOARDS, 100.0, 1000, &[]).is_ok());
}

#[test]
fn the_csv_has_one_line_per_part_and_quotes_what_needs_it() {
    let b = bom("Designator,Comment,Footprint\n\"R1,R2\",\"10k, 1%\",R_0603\n");
    let csv = order_csv(&order(&b, 2, 50.0, 0, &[]).unwrap());
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "value,package,part_number,per_board,boards,spare,total,references"
    );
    assert_eq!(lines[1], "\"10k, 1%\",R_0603,,2,2,2,6,R1 R2");
    assert_eq!(lines.len(), 2);
}

#[test]
fn the_settings_have_their_ranges_and_the_example_is_the_default() {
    for key in ["order_spare_percent", "order_spare_min"] {
        assert!(KEYS.contains(&key), "{key}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(
        (e.order_spare_percent, e.order_spare_min),
        (d.order_spare_percent, d.order_spare_min)
    );
    assert_eq!((d.order_spare_percent, d.order_spare_min), (0.0, 0));
    for (key, ok, bad) in [
        ("order_spare_percent", ["0", "100"], ["-1", "100.1"]),
        ("order_spare_min", ["0", "1000"], ["-1", "1001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            let err = MachineConfig::parse(&format!("{key} = {v}")).unwrap_err();
            assert_eq!(err.code, Code::ConfigOutOfRange, "{key} {v}");
        }
    }
}

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

#[test]
fn bom_order_prints_the_list_and_the_options_win_over_the_configuration() {
    let o = run(&[
        "bom-order",
        "examples/bom.csv",
        "--boards",
        "10",
        "--spare-percent",
        "10",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.contains("R_0603") && out.contains("RC0603FR-0710KL"),
        "{out}"
    );
    // 3 resistors on 10 boards is 30 and 10% is 3
    assert!(out.contains(",3,10,3,33,R1 R2 R3"), "{out}");
    assert!(
        text(&o.stderr).contains("ok: 4 lines, 77 parts for 10 boards with 10% spare (at least 0)"),
        "{}",
        text(&o.stderr)
    );
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_order_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("order.cfg");
    std::fs::write(&cfg, "order_spare_percent = 50\norder_spare_min = 4\n").unwrap();
    let from_cfg = run(&[
        "bom-order",
        "examples/bom.csv",
        "--config",
        cfg.to_str().unwrap(),
    ]);
    assert!(
        text(&from_cfg.stderr).contains("with 50% spare (at least 4)"),
        "{}",
        text(&from_cfg.stderr)
    );
    let both = run(&[
        "bom-order",
        "examples/bom.csv",
        "--config",
        cfg.to_str().unwrap(),
        "--spare-percent",
        "0",
        "--spare-min",
        "0",
    ]);
    assert!(
        text(&both.stderr).contains("7 parts for 1 boards with 0% spare"),
        "{}",
        text(&both.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bom_order_leaves_out_the_parts_the_table_does_not_mount_and_never_writes_over_a_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_order2_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("order.csv");
    let args = [
        "bom-order",
        "examples/bom.csv",
        "--parts",
        "examples/board_dnp.csv",
        "--out",
        out.to_str().unwrap(),
    ];
    let o = run(&args);
    assert!(o.status.success(), "{}", text(&o.stderr));
    // R2 and C2 are not mounted in the table: 2 resistors and 1 capacitor are left, plus the chip and the LED
    assert!(
        text(&o.stdout).starts_with("ok: 4 lines, 5 parts for 1 boards"),
        "{}",
        text(&o.stdout)
    );
    let written = std::fs::read_to_string(&out).unwrap();
    assert!(written.contains(",2,1,0,2,R1 R3"), "{written}");
    let again = run(&args);
    assert!(!again.status.success());
    assert!(text(&again.stderr).contains("PHOENIX-PNP-040"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), written);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bom_order_is_strict_and_reports_its_errors_in_the_language_asked() {
    for bad in [
        vec!["bom-order"],
        vec!["bom-order", "examples/bom.csv", "extra"],
        vec!["bom-order", "examples/bom.csv", "--boards", "x"],
        vec!["bom-order", "examples/bom.csv", "--boards", "0"],
        vec!["bom-order", "examples/bom.csv", "--spare-percent", "abc"],
        vec!["bom-order", "examples/bom.csv", "--oops"],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
    let o = run(&[
        "bom-order",
        "examples/bom.csv",
        "--boards",
        "0",
        "--lang",
        "fr",
    ]);
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-017: Un réglage du plan"),
        "{}",
        text(&o.stderr)
    );
}
