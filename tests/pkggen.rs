//! The sizes of the common packages from their names: the chips of the EIA codes, the small outlines, the SOIC, TSSOP, QFN, DFN and QFP, the table they make, and the command.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::inspect::SizeTable;
use eh3d_phoenix_pnp::pkggen::{generate, table_csv, tape_for, PackageSpec};
use std::path::PathBuf;
use std::process::{Command, Output};

fn size(name: &str) -> (f64, f64, f64, f64) {
    let s = generate(name).unwrap_or_else(|e| panic!("{name}: {}", e.detail));
    (s.body_long_mm, s.body_short_mm, s.height_mm, s.tape_mm)
}

#[test]
fn the_chips_have_the_bodies_of_their_codes_whatever_the_letters_and_the_separator() {
    for (code, long, short, height) in [
        ("01005", 0.4, 0.2, 0.13),
        ("0201", 0.6, 0.3, 0.23),
        ("0402", 1.0, 0.5, 0.35),
        ("0603", 1.6, 0.8, 0.5),
        ("0805", 2.0, 1.25, 0.5),
        ("1206", 3.2, 1.6, 0.55),
        ("1210", 3.2, 2.5, 0.55),
        ("1812", 4.5, 3.2, 0.6),
        ("2010", 5.0, 2.5, 0.6),
        ("2512", 6.3, 3.2, 0.6),
    ] {
        for name in [
            code.to_string(),
            format!("R{code}"),
            format!("c_{code}"),
            format!("LED-{code}"),
            format!("FB_{code}"),
        ] {
            let (l, s, h, _) = size(&name);
            assert_eq!((l, s, h), (long, short, height), "{name}");
        }
    }
    // the name is kept as it was written
    assert_eq!(generate(" r_0603 ").unwrap().name, "r_0603");
    // a chip of the other letters or of a code that is not one is not guessed
    for bad in ["0604", "R", "R06030", "RESISTOR_0603", "R-0603-X", "0603X"] {
        let e = generate(bad).unwrap_err();
        assert_eq!(e.code, Code::CatalogBadRow, "{bad}");
    }
}

#[test]
fn the_small_outlines_the_soic_and_the_tssop_have_their_bodies() {
    let table: [(&str, f64, f64, f64); 19] = [
        ("SOT-23", 2.9, 1.3, 1.1),
        ("sot-23-3", 2.9, 1.3, 1.1),
        ("SOT_23_5", 2.9, 1.6, 1.1),
        ("SOT-23-6", 2.9, 1.6, 1.1),
        ("SOT-223", 6.5, 3.5, 1.6),
        ("SOT-89", 4.5, 2.5, 1.5),
        ("SOT-323", 2.0, 1.25, 0.95),
        ("SC-70", 2.0, 1.25, 0.95),
        ("SOD-123", 2.7, 1.6, 1.1),
        ("SOD-323", 1.7, 1.25, 0.95),
        ("SOD-523", 1.2, 0.8, 0.6),
        ("SOIC-8", 4.9, 3.9, 1.75),
        ("SO-8", 4.9, 3.9, 1.75),
        ("SOIC-14", 8.65, 3.9, 1.75),
        ("SOIC-16", 9.9, 3.9, 1.75),
        ("SOIC-16-W", 9.9, 7.5, 2.65),
        ("SOIC-28-W", 17.9, 7.5, 2.65),
        ("TSSOP-8", 3.0, 4.4, 1.1),
        ("TSSOP-20", 6.5, 4.4, 1.1),
    ];
    for (name, long, short, height) in table {
        let (l, s, h, _) = size(name);
        // the long side first, whichever way the body is written
        assert_eq!(
            (l, s, h),
            (long.max(short), long.min(short), height),
            "{name}"
        );
    }
    // the wide body is for the SOIC of 16 pins and more, the narrow one for 16 and fewer
    assert!(generate("SOIC-20")
        .unwrap_err()
        .detail
        .contains("SOIC-20-W"));
    assert!(generate("SOIC-8-W").unwrap_err().detail.contains("narrow"));
    // a number of pins that the table does not have says which it has
    let e = generate("SOIC-10").unwrap_err();
    assert!(e.detail.contains("8, 14, 16"), "{}", e.detail);
    assert!(generate("TSSOP-10")
        .unwrap_err()
        .detail
        .contains("8, 14, 16"));
}

#[test]
fn the_square_packages_take_the_body_of_their_pins_or_the_one_in_the_name() {
    for (name, side, height) in [
        ("QFN-16", 3.0, 0.9),
        ("QFN-32", 5.0, 0.9),
        ("qfn_48", 7.0, 0.9),
        ("VQFN-64", 9.0, 0.9),
        ("DFN-20", 4.0, 0.9),
        ("LQFP-48", 7.0, 1.4),
        ("QFP-64", 10.0, 1.4),
        ("TQFP-100", 14.0, 1.0),
        ("LQFP-144", 20.0, 1.4),
    ] {
        let (l, s, h, _) = size(name);
        assert_eq!((l, s, h), (side, side, height), "{name}");
    }
    // the body in the name wins, and the long side comes first
    assert_eq!(size("QFN-32-4x5"), (5.0, 4.0, 0.9, 12.0));
    assert_eq!(size("DFN-8-3x3").0, 3.0);
    assert_eq!(size("QFN-24-4.5X3.2").0, 4.5);
    assert_eq!(size("TQFP-44-10x10").2, 1.0);
    // a number of pins the table does not have, a body that is not one, pins that are not a number
    assert!(generate("QFN-33").unwrap_err().detail.contains("32-5x5"));
    for bad in [
        "QFN-32-5",
        "QFN-32-0x5",
        "QFN-32-ax5",
        "QFN-32-99x5",
        "QFN-x",
        "QFN-1",
        "QFN-1001",
        "QFN",
        "LQFP-",
        "QFN-32-5x5-1",
    ] {
        assert_eq!(
            generate(bad).unwrap_err().code,
            Code::CatalogBadRow,
            "{bad}"
        );
    }
}

#[test]
fn the_tape_is_the_width_that_holds_the_body() {
    assert_eq!(tape_for(1.6, 0.8), 8.0);
    assert_eq!(tape_for(3.2, 3.2), 8.0);
    assert_eq!(tape_for(3.6, 1.0), 12.0);
    assert_eq!(tape_for(6.5, 3.5), 12.0);
    assert_eq!(tape_for(9.9, 7.5), 16.0);
    assert_eq!(tape_for(7.5, 9.9), 16.0, "the sides in either order");
    assert_eq!(tape_for(12.0, 12.0), 24.0);
    assert_eq!(tape_for(20.0, 20.0), 32.0);
    assert_eq!(tape_for(40.0, 40.0), 44.0);
    // the tape of every size in the tables is one of the standard widths and the body fits it
    for name in [
        "0201", "0805", "SOT-223", "SOIC-16", "TSSOP-28", "QFN-64", "LQFP-144",
    ] {
        let s = generate(name).unwrap();
        assert!(
            [8.0, 12.0, 16.0, 24.0, 32.0, 44.0].contains(&s.tape_mm),
            "{name}"
        );
        assert!(
            s.body_long_mm >= s.body_short_mm && s.height_mm > 0.0,
            "{name}"
        );
    }
}

#[test]
fn nothing_that_is_typed_panics_and_what_is_not_a_package_is_an_error() {
    let mut state = 7u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    let alphabet: Vec<char> = "RCLQSOTFNDP-_x.0123456789 \u{e9}\u{4e2d}".chars().collect();
    for _ in 0..3000 {
        let len = next() % 14;
        let name: String = (0..len)
            .map(|_| alphabet[next() % alphabet.len()])
            .collect();
        let _ = generate(&name);
    }
    for bad in [
        "",
        "   ",
        "a\u{7}b",
        "R0603\n",
        "FOO",
        "BGA-256",
        "0603 0603",
    ] {
        let e = generate(bad);
        assert!(e.is_err() || bad == "R0603\n", "{bad:?}");
    }
    assert!(generate("BGA-256")
        .unwrap_err()
        .detail
        .contains("write its row by hand"));
}

#[test]
fn the_rows_are_a_table_of_packages_that_the_other_commands_read() {
    let specs: Vec<PackageSpec> = ["R_0603", "SOT-23-5", "SOIC-8", "QFN-32-5x5", "LQFP-48"]
        .iter()
        .map(|n| generate(n).unwrap())
        .collect();
    let csv = table_csv(&specs);
    assert_eq!(
        csv.lines().next().unwrap(),
        "package,height_mm,tape_mm,body_long_mm,body_short_mm"
    );
    assert_eq!(csv.lines().nth(1).unwrap(), "R_0603,0.5,8,1.6,0.8");
    assert_eq!(csv.lines().nth(4).unwrap(), "QFN-32-5x5,0.9,12,5,5");
    // the catalog takes it, with the bodies
    let mut catalog = Catalog::new();
    catalog.add_packages(&csv).unwrap();
    assert_eq!(catalog.package_body("r_0603"), Some((1.6, 0.8)));
    assert_eq!(catalog.package_body("SOIC-8"), Some((4.9, 3.9)));
    // and so does the table of sizes of the check of the parts
    let sizes = SizeTable::from_catalog(&catalog);
    assert_eq!(sizes.lookup("LQFP-48").unwrap().long_mm, 7.0);
    // a name that needs quoting is quoted
    let odd = PackageSpec {
        name: "a,b".to_string(),
        height_mm: 1.0,
        tape_mm: 8.0,
        body_long_mm: 2.0,
        body_short_mm: 1.0,
    };
    assert!(table_csv(&[odd]).contains("\"a,b\",1,8,2,1"));
    assert_eq!(table_csv(&[]).lines().count(), 1);
}

// ---- the command

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn the_command_writes_the_rows_for_the_names_and_for_a_position_file() {
    let o = run(&[
        "package-gen",
        "0603",
        "SOT-23",
        "0603",
        "sot-23",
        "QFN-32-5x5",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    // a package named twice is written once
    assert_eq!(out.lines().count(), 4, "{out}");
    assert!(out.contains("\n0603,0.5,8,1.6,0.8\n") && out.contains("SOT-23,1.1,8,2.9,1.3"));
    // the packages of a position file
    let o = run(&["package-gen", "--from", "examples/board.pos"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    for line in [
        "R_0603,0.5,8,1.6,0.8",
        "QFN-32,0.9,12,5,5",
        "LED_0603,0.5,8,1.6,0.8",
    ] {
        assert!(out.contains(line), "{out}");
    }
    assert_eq!(out.lines().count(), 5, "{out}");
    // a name that is not known is told and the exit code is 1, the others are written all the same
    let o = run(&["package-gen", "0402", "FOO-7", "SOIC-20", "--lang", "es"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stdout).contains("0402,0.35,8,1,0.5"));
    let err = text(&o.stderr);
    assert!(
        err.contains("FOO-7") && err.contains("SOIC-20-W") && err.contains("PHOENIX-PNP-010"),
        "{err}"
    );
    assert!(err.contains("2 packages are not known"), "{err}");
    // the file it writes is read by part-inspect as the sizes of the packages
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_pkggen_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let table = dir.join("packages.csv");
    let t = table.to_str().unwrap();
    let o = run(&["package-gen", "--from", "examples/board.pos", "--out", t]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).is_empty());
    let o = run(&["part-inspect", "examples/nozzle_reads.csv", "--packages", t]);
    assert!(text(&o.stderr).contains("accepted"), "{}", text(&o.stderr));
    // never over a file that exists, nor over the file it reads
    assert!(!run(&["package-gen", "0603", "--out", t]).status.success());
    assert!(!run(&[
        "package-gen",
        "--from",
        "examples/board.pos",
        "--out",
        "examples/board.pos"
    ])
    .status
    .success());
    // mistakes
    assert!(!run(&["package-gen"]).status.success());
    assert!(!run(&["package-gen", "0603", "--oops"]).status.success());
    assert!(!run(&["package-gen", "--from", "nothing.pos"])
        .status
        .success());
    assert!(text(&run(&["help"]).stdout).contains("package-gen"));
    let _ = std::fs::remove_dir_all(&dir);
}
