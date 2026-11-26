//! The exported tables against a spreadsheet that runs formulas: a text cell that starts like one gets a quote in front, numbers stay as they are, and a table that is read back gives the
//! very texts that were written. Round trips of the writers that are read again, with a fixed seed and with hostile texts.

use eh3d_phoenix_pnp::calibrate::{taught_csv, teach_file};
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::fiducial::{measured_csv, parse_measured};
use eh3d_phoenix_pnp::parts::{
    self, csv_field, formula_guard, formula_unguard, read_records, Part, Side,
};
use eh3d_phoenix_pnp::polarity;
use eh3d_phoenix_pnp::queue::{Entry, Kind, Queue};
use eh3d_phoenix_pnp::refill::{parse_refills, refill};
use eh3d_phoenix_pnp::slotbank::{Bank, Geometry, Layout};

/// Texts that a spreadsheet would run, or that look like the guard itself.
const HOSTILE: [&str; 14] = [
    "=1+1",
    "+3V3",
    "-5V",
    "@SUM(A1:A9)",
    "=HYPERLINK(\"http://x\",\"y\")",
    "=cmd|' /C calc'!A0",
    "'=already",
    "''-twice",
    "-a,b",
    "=a;b",
    "+say \"hi\"",
    "-",
    "-inf",
    "=-5",
];

/// Numbers and plain texts: they are written as they are.
const HARMLESS: [&str; 14] = [
    "-5",
    "-0.5",
    "+3.3",
    "-.5",
    "5.",
    "-1e-3",
    "+2E6",
    "-3.000,2.000",
    "-1, +2",
    "10k",
    "a=b",
    "'quoted'",
    "R_0603",
    "x-1",
];

/// What a spreadsheet would see in each cell of a line: the quotes of the CSV taken off and nothing else.
fn raw_cells(text: &str, delim: char) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let cell = cells.last_mut().expect("a cell");
        if quoted {
            if c == '"' && chars.peek() == Some(&'"') {
                cell.push('"');
                chars.next();
            } else if c == '"' {
                quoted = false;
            } else {
                cell.push(c);
            }
        } else if c == '"' && cell.is_empty() {
            quoted = true;
        } else if c == delim || c == '\n' {
            cells.push(String::new());
        } else {
            cell.push(c);
        }
    }
    cells
}

fn runs_in_a_spreadsheet(cell: &str) -> bool {
    cell.starts_with(['=', '@', '\t', '\r'])
        || (cell.starts_with(['+', '-'])
            && cell.split(',').any(|n| n.trim().parse::<f64>().is_err()))
}

#[test]
fn a_text_that_starts_like_a_formula_gets_a_quote_and_a_number_does_not() {
    for s in HOSTILE {
        assert_eq!(formula_guard(s), format!("'{s}"), "{s}");
    }
    for s in ["\t=1", "\r=1", "\tx", "\t-5"] {
        assert_eq!(formula_guard(s), format!("'{s}"), "{s:?}");
    }
    for s in HARMLESS {
        assert_eq!(formula_guard(s), s, "{s}");
        assert_eq!(formula_unguard(s), s, "{s}");
        assert_eq!(csv_field(s).trim_matches('"'), s, "{s}");
    }
    for s in ["", "'", "\t", "\r", "'\t", "'-5", "'+3.3"] {
        assert_eq!(formula_guard(s), s, "{s:?}");
        assert_eq!(formula_unguard(s), s, "{s:?}");
    }
}

#[test]
fn texts_made_from_a_fixed_seed_come_back_as_they_were_and_no_cell_runs() {
    const LETTERS: [char; 18] = [
        '=', '+', '-', '@', '\t', '\r', '\n', '\'', '"', ',', ';', ' ', '.', 'e', '5', '0', 'a',
        'µ',
    ];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move |below: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % below
    };
    let mut tried = 0;
    while tried < 4000 {
        let len = 1 + next(9);
        let s: String = (0..len)
            .map(|_| LETTERS[next(LETTERS.len() as u64) as usize])
            .collect();
        // the cell by itself, whatever the text
        assert_eq!(formula_unguard(&formula_guard(&s)), s, "{s:?}");
        // through a table: the reader trims blank space around a cell, so the texts that have none are the ones that must be the same
        let kept = s.trim() == s || (s.starts_with(['\t', '\r']) && s.trim_end() == s);
        if !kept || s.trim().is_empty() {
            continue;
        }
        tried += 1;
        for delim in [',', ';'] {
            let line = format!("k{delim}{}{delim}z\n", csv_field(&s));
            let rows = read_records(&line, delim).unwrap();
            assert_eq!(
                rows,
                vec![vec!["k".to_string(), s.clone(), "z".into()]],
                "{s:?}"
            );
            let seen = &raw_cells(&line, delim)[1];
            assert!(!runs_in_a_spreadsheet(seen), "{s:?} -> {seen:?}");
        }
    }
}

fn part(reference: &str, value: &str, package: &str, x: f64) -> Part {
    Part {
        reference: reference.to_string(),
        value: value.to_string(),
        package: package.to_string(),
        x,
        y: -2.5,
        rotation: 90.0,
        side: Side::Top,
    }
}

#[test]
fn a_table_of_parts_with_hostile_texts_is_read_back_and_its_negative_numbers_are_numbers() {
    let list: Vec<Part> = HOSTILE
        .iter()
        .enumerate()
        .map(|(i, s)| part(&format!("{s}_{i}"), s, s, -12.5))
        .collect();
    let csv = polarity::to_csv(&list);
    for line in csv.lines().skip(1) {
        let cells = raw_cells(line, ',');
        for cell in &cells[..3] {
            assert!(!runs_in_a_spreadsheet(cell), "{line}");
        }
        assert_eq!(cells[4], "-12.500", "{line}");
        assert_eq!(cells[5], "-2.500", "{line}");
    }
    // the columns of the position are `x_mm` and so on in this sheet: the reader of the parts takes them with their names
    let renamed = csv.replacen("x_mm,y_mm,rotation_deg", "x,y,rotation", 1);
    assert_eq!(parts::from_csv(&renamed).unwrap(), list);
}

#[test]
fn the_tables_of_feeders_marks_and_taught_feeders_keep_hostile_ids() {
    let text = "id,value,package,x,y,count\n'=F1,'+3V3,'-x,-20,-10,5\nF2,-5,'@y,1,2,3\n";
    let first = feeding::from_csv(text).unwrap();
    assert_eq!(
        (first[0].base.id.as_str(), first[0].base.value.as_str()),
        ("=F1", "+3V3")
    );
    assert_eq!(first[1].base.value, "-5");
    let written = feeding::to_csv(&first);
    assert!(written.contains("\n'=F1,'+3V3,'-x,-20,-10,5,"), "{written}");
    assert!(written.contains("\nF2,-5,'@y,"), "{written}");
    assert_eq!(feeding::from_csv(&written).unwrap(), first);

    let marks = vec![
        ("=m1".to_string(), -1.5, 2.0),
        ("-m,2".to_string(), 0.0, -3.0),
    ];
    let written = measured_csv(&marks);
    assert!(written.contains("\n'=m1,-1.5,2\n"), "{written}");
    assert_eq!(parse_measured(&written).unwrap(), marks);

    let taught = teach_file("=T1,strip,0,0,36,0,,,10,\n").unwrap();
    assert_eq!(taught[0].0, "=T1");
    let written = taught_csv(&taught);
    assert!(written.contains("\n'=T1,strip,"), "{written}");
    assert_eq!(read_records(&written, ',').unwrap()[1][0], "=T1");
}

#[test]
fn the_banks_the_queue_and_the_catalog_keep_hostile_texts() {
    let geo = Geometry::from_config(&MachineConfig::default());
    let mut layout = Layout::new(geo);
    layout.place("=a,b", Bank::Left, 1, 8.0, None).unwrap();
    let written = layout.to_csv();
    assert!(written.contains("\n\"'=a,b\",left,1,"), "{written}");
    assert_eq!(Layout::from_csv(&written, geo).unwrap().to_csv(), written);
    assert_eq!(
        Layout::from_csv(&written, geo).unwrap().entries()[0].id,
        "=a,b"
    );

    let mut q = Queue::new();
    let mut e = Entry::new("-board", Kind::Parts, "=board.pos", "@feeders.csv", 2);
    e.detail = "+1 left, =2".to_string();
    q.add(e.clone()).unwrap();
    let written = q.to_text();
    assert!(
        written.contains("\n'-board,parts,'=board.pos,'@feeders.csv,"),
        "{written}"
    );
    assert_eq!(Queue::parse(&written).unwrap().entries(), &[e]);

    let mut first = Catalog::new();
    first
        .add_packages("package,nozzle_tip,origin_dx_mm\n'=0603,'+tip,-0.5\n")
        .unwrap();
    let written = first.packages_csv();
    assert!(written.contains("\n'=0603,"), "{written}");
    assert!(written.contains(",'+tip,"), "{written}");
    assert!(written.contains(",-0.5,"), "{written}");
    let mut again = Catalog::new();
    again.add_packages(&written).unwrap();
    assert_eq!(again.packages_csv(), written);
}

#[test]
fn a_table_with_semicolons_that_is_written_again_is_guarded_too() {
    let text = "id;value;package;x;y;count\n'=F1;'+3V3;'-x;-20;-10;5\n";
    let rows = parse_refills("id,count\n'=F1,200\n").unwrap();
    let done = refill(text, &rows).unwrap();
    assert!(
        done.csv.contains("\n'=F1;'+3V3;'-x;-20;-10;200\n"),
        "{}",
        done.csv
    );
    let after = feeding::from_csv(&done.csv).unwrap();
    assert_eq!(
        (after[0].base.id.as_str(), after[0].base.count),
        ("=F1", 200)
    );
}
