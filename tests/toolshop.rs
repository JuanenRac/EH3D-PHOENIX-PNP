//! The six new panels of the Tools tab, without the window: the heights of the nozzles, the marks of the panel, the tapes by step, the effectiveness of a shift, the file of an assembler
//! and several bills of materials. Each gives what its command gives, with the same refusals, and writes its table once and never over a file.

use eh3d_phoenix_pnp::asmexport::{Format, Unit};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::gui::toolbox::{Tool, Toolbox};
use eh3d_phoenix_pnp::gui::toolmore::{
    bom_merge_text, import_assembler_text, nozzle_z_text, panel_fids_text, shift_oee_text,
    tape_steps_text,
};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use std::path::PathBuf;
use std::process::{Command, Output};

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn ready() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", &example("board.pos")).unwrap();
    ws.load_text("feeders.csv", &example("feeders.csv"))
        .unwrap();
    ws
}

fn marked() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board_marks.pos", &example("board_marks.pos"))
        .unwrap();
    ws.load_path(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("panel_marks.json"),
    )
    .unwrap();
    ws
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_toolshop_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn said(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn the_six_tools_have_a_key_a_word_a_file_and_the_texts_they_read() {
    assert_eq!(Tool::ALL.len(), 40);
    for (tool, key, name, first, second) in [
        (Tool::NozzleZ, "zcal", "nozzle_z.csv", true, false),
        (
            Tool::PanelFids,
            "panelfids",
            "panel_marks.csv",
            false,
            false,
        ),
        (Tool::TapeSteps, "tapesteps", "tape_steps.csv", false, false),
        (Tool::ShiftOee, "shiftoee", "shift_oee.csv", false, false),
        (
            Tool::ImportAssembler,
            "assembler",
            "parts_imported.csv",
            true,
            false,
        ),
        (Tool::BomMerge, "bommerge", "bom_merged.csv", true, true),
    ] {
        assert_eq!(tool.key(), key);
        assert_eq!(Tool::from_key(key), Some(tool));
        assert_eq!(tool.output_name(), Some(name));
        assert_eq!(
            (tool.reads_text(), tool.reads_second()),
            (first, second),
            "{key}"
        );
        assert!(!tr(tool.text(), Lang::En).is_empty());
    }
    let mut keys: Vec<&str> = Tool::ALL.iter().map(|t| t.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 40);
    let mut files: Vec<&str> = Tool::ALL.iter().filter_map(|t| t.output_name()).collect();
    files.sort_unstable();
    let before = files.len();
    files.dedup();
    assert_eq!(files.len(), before, "two tools write the same file");
}

#[test]
fn the_new_words_are_in_the_eight_languages_and_in_the_language_of_portugal() {
    let new = [
        Text::ToolNozzleZ,
        Text::ToolPanelFids,
        Text::ToolTapeSteps,
        Text::ToolShiftOee,
        Text::ToolImportAssembler,
        Text::ToolBomMerge,
        Text::HintNozzleZ,
        Text::HintPanelFids,
        Text::HintTapeSteps,
        Text::HintShiftOee,
        Text::HintImportAssembler,
        Text::HintBomMerge,
        Text::NozzleZTolerance,
        Text::PanelFidsMinSep,
        Text::ShiftMinutes,
        Text::ReelChangeSeconds,
        Text::AsmShape,
        Text::AsmUnit,
        Text::BomBoardsFirst,
        Text::BomBoardsSecond,
    ];
    assert_eq!(Text::COUNT, table().len());
    assert_eq!(Text::BomBoardsSecond as usize + 1, Text::COUNT);
    assert_eq!(new[0] as usize, Text::ServerSimulated as usize + 1);
    for (i, word) in new.iter().enumerate() {
        assert_eq!(*word as usize, new[0] as usize + i, "{word:?} is in order");
        let row = &table()[*word as usize];
        assert_eq!(row.len(), 8);
        assert!(row.iter().all(|w| !w.trim().is_empty()), "{word:?}");
        // the languages that do not share a word with English are really translated
        for lang in [
            Lang::Es,
            Lang::De,
            Lang::Fr,
            Lang::It,
            Lang::Ja,
            Lang::Zh,
            Lang::Pt,
        ] {
            assert_ne!(tr(*word, lang), tr(*word, Lang::En), "{word:?} {lang:?}");
        }
        assert!(
            !tr(*word, Lang::Ja).is_ascii() && !tr(*word, Lang::Zh).is_ascii(),
            "{word:?}"
        );
        // the language of Portugal, not the one of Brazil
        let pt = tr(*word, Lang::Pt);
        for brazil in ["arquivo", "tela", "usuário", "configurações"] {
            assert!(!pt.contains(brazil), "{word:?}: {pt}");
        }
    }
    assert!(tr(Text::HintImportAssembler, Lang::Pt).contains("ficheiro"));
    assert_eq!(tr(Text::NozzleZTolerance, Lang::En), "Tolerance (mm)");
}

// ---- the heights of the nozzles

#[test]
fn the_heights_of_the_nozzles_are_what_the_command_says_for_the_probings_that_were_typed() {
    let ws = ready();
    let text = example("z_probe.csv");
    let (info, csv) = nozzle_z_text(&ws, &text, 0.02, Lang::En).unwrap();
    let o = run(&["nozzle-zcal", "examples/z_probe.csv"]);
    assert_eq!(info.trim_end(), said(&o).trim_end());
    assert!(csv.starts_with("nozzle,"), "{csv}");
    // a tolerance that is too small or too large is refused with the code
    assert!(nozzle_z_text(&ws, &text, 0.0, Lang::En).is_err());
    assert!(nozzle_z_text(&ws, &text, 2.0, Lang::En).is_err());
    // damaged probings are an error with a line
    assert!(nozzle_z_text(&ws, "nozzle,z_mm\n1,abc\n", 0.02, Lang::En).is_err());
}

#[test]
fn with_nothing_typed_the_probings_are_made_up_on_the_simulated_machine_and_say_so() {
    let ws = ready();
    let (info, csv) = nozzle_z_text(&ws, "  \n", 0.02, Lang::En).unwrap();
    assert!(info.contains("simulated probings of"), "{info}");
    assert!(info.contains("no machine was touched"));
    assert!(csv.lines().count() >= 2);
    // the same seed gives the same probings
    assert_eq!(nozzle_z_text(&ws, "", 0.02, Lang::En).unwrap(), (info, csv));
}

// ---- the marks of the panel

#[test]
fn the_marks_of_the_panel_are_what_the_command_says_and_a_panel_must_be_loaded() {
    let ws = marked();
    let (info, csv) = panel_fids_text(&ws, 10.0, Lang::En).unwrap();
    assert!(info.contains("2 boards in the panel"), "{info}");
    assert!(info.contains("C: marked as bad and left out"));
    assert!(info.contains("every board can be aligned by its own marks"));
    assert!(csv.lines().count() > 1);
    // marks that are too near for the distance asked are the boards that cannot be aligned
    let (info, _) = panel_fids_text(&ws, 50.0, Lang::En).unwrap();
    assert!(info.contains("cannot be aligned"), "{info}");
    assert!(panel_fids_text(&ws, -1.0, Lang::En).is_err());
    assert!(panel_fids_text(&ws, 1001.0, Lang::En).is_err());
    let e = panel_fids_text(&ready(), 10.0, Lang::En).unwrap_err();
    assert!(e.contains("load a panel"), "{e}");
}

// ---- the tapes

#[test]
fn the_tapes_by_step_are_what_the_command_says() {
    let ws = ready();
    let (info, csv) = tape_steps_text(&ws, Lang::En).unwrap();
    let o = run(&["tape-steps", "examples/board.pos", "examples/feeders.csv"]);
    let all = said(&o);
    assert!(
        all.starts_with(info.lines().next().unwrap()),
        "{info}\n{all}"
    );
    // the six parts of the top side, the one the plan is made for
    assert_eq!(csv.lines().count() - 1, 6);
    let e = tape_steps_text(&Workspace::new(Lang::En), Lang::En).unwrap_err();
    assert!(e.contains("load a table of parts"), "{e}");
}

// ---- the shift

#[test]
fn the_effectiveness_of_a_shift_is_what_the_command_says_for_the_same_runs_and_the_same_seed() {
    let ws = ready();
    let (info, csv) = shift_oee_text(&ws, 5, 7, 480, 60.0, Lang::En).unwrap();
    let o = run(&[
        "shift-oee",
        "examples/board.pos",
        "examples/feeders.csv",
        "--runs",
        "5",
        "--seed",
        "7",
        "--shift",
        "480",
        "--reel-change",
        "60",
    ]);
    assert_eq!(info.trim_end(), said(&o).trim_end());
    assert!(csv.contains("oee"), "{csv}");
    // a shorter shift holds fewer boards
    let (short, _) = shift_oee_text(&ws, 5, 7, 60, 60.0, Lang::En).unwrap();
    assert_ne!(info, short);
    assert!(shift_oee_text(&ws, 0, 7, 480, 60.0, Lang::En).is_err());
    assert!(shift_oee_text(&ws, 5, 7, 480, -1.0, Lang::En).is_err());
    assert!(shift_oee_text(&Workspace::new(Lang::En), 5, 7, 480, 60.0, Lang::En).is_err());
}

// ---- the file of an assembler

#[test]
fn a_file_of_an_assembler_is_read_in_millimetres_and_given_back_in_the_shape_and_unit_asked() {
    let text = example("altium_pnp.csv");
    let (info, out) =
        import_assembler_text(&text, Format::Generic, Unit::Mm, 1 << 20, Lang::En).unwrap();
    assert!(
        info.starts_with("7 parts read (shape of the file: altium)"),
        "{info}"
    );
    let o = run(&["import-assembler", "examples/altium_pnp.csv"]);
    assert_eq!(out, said(&o));
    let (_, inch) =
        import_assembler_text(&text, Format::Generic, Unit::Inch, 1 << 20, Lang::En).unwrap();
    assert_ne!(out, inch);
    let (_, cpl) = import_assembler_text(&text, Format::Cpl, Unit::Mm, 1 << 20, Lang::En).unwrap();
    assert!(cpl.starts_with("Designator,Val,Package,Mid X,Mid Y,Rotation,Layer"));
    // a file that is not a table of positions, or too large, is refused
    assert!(
        import_assembler_text("nothing\n", Format::Generic, Unit::Mm, 1 << 20, Lang::En).is_err()
    );
    assert!(import_assembler_text(&text, Format::Generic, Unit::Mm, 10, Lang::En).is_err());
}

// ---- the bills

#[test]
fn two_bills_are_one_list_to_buy_and_a_bill_left_empty_is_not_part_of_it() {
    let (info, csv) = bom_merge_text(
        &example("bom.csv"),
        &example("bom_b.csv"),
        (2, 3),
        1 << 20,
        Lang::En,
    )
    .unwrap();
    let o = run(&["bom-merge", "examples/bom.csv:2", "examples/bom_b.csv:3"]);
    // the command names the bills by their files, the panel calls them first and second
    let by_command = said(&o).replace("bom_b", "second").replace("bom", "first");
    assert_eq!(info.trim_end(), by_command.trim_end());
    assert!(csv.lines().count() > 1);
    let (alone, _) = bom_merge_text(&example("bom.csv"), "  ", (2, 3), 1 << 20, Lang::En).unwrap();
    assert_ne!(info, alone);
    let e = bom_merge_text("", "", (1, 1), 1 << 20, Lang::En).unwrap_err();
    assert!(e.contains("at least one bill"), "{e}");
    assert!(bom_merge_text(&example("bom.csv"), "", (0, 1), 1 << 20, Lang::En).is_err());
    assert!(bom_merge_text("not a bill\n", "", (1, 1), 1 << 20, Lang::En).is_err());
}

// ---- the tab

#[test]
fn the_tab_runs_the_six_tools_and_writes_each_result_once() {
    let mut ws = ready();
    let mut tb = Toolbox::new(&ws.cfg);
    let dir = scratch("tab");
    let out = dir.to_str().unwrap();
    // the forms start with what the settings say
    assert_eq!(tb.zcal_tol_mm, ws.cfg.nozzle_z_tol_mm);
    assert_eq!(tb.panelfids_sep_mm, ws.cfg.panel_fid_min_sep_mm);
    assert_eq!((tb.shift_minutes, tb.reel_change_s), (480, 60.0));
    assert_eq!((tb.asm_format, tb.asm_unit), (Format::Generic, Unit::Mm));
    assert_eq!((tb.merge_boards_first, tb.merge_boards_second), (1, 1));
    tb.zcal_text = example("z_probe.csv");
    tb.asm_text = example("altium_pnp.csv");
    tb.merge_first = example("bom.csv");
    tb.merge_second = example("bom_b.csv");
    for (tool, file) in [
        (Tool::NozzleZ, "nozzle_z.csv"),
        (Tool::TapeSteps, "tape_steps.csv"),
        (Tool::ShiftOee, "shift_oee.csv"),
        (Tool::ImportAssembler, "parts_imported.csv"),
        (Tool::BomMerge, "bom_merged.csv"),
    ] {
        tb.tool = tool;
        tb.run(&ws, Lang::En);
        assert!(tb.error.is_empty(), "{tool:?} {}", tb.error);
        assert!(!tb.output.is_empty(), "{tool:?}");
        assert!(tb.save(out, Lang::En).unwrap().ends_with(file));
        // never over a file that is there
        assert!(tb.save(out, Lang::En).is_none());
    }
    // the panel of marks needs a panel
    tb.tool = Tool::PanelFids;
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("load a panel"), "{}", tb.error);
    let mut with_panel = marked();
    with_panel.cfg = ws.cfg.clone();
    tb.run(&with_panel, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb.save(out, Lang::En).unwrap().ends_with("panel_marks.csv"));
    // the texts of the tools are the ones the boxes edit
    for (tool, second) in [
        (Tool::NozzleZ, false),
        (Tool::ImportAssembler, false),
        (Tool::BomMerge, false),
        (Tool::BomMerge, true),
    ] {
        tb.tool = tool;
        let slot = if second {
            tb.second_text_mut()
        } else {
            tb.text_mut()
        };
        assert!(slot.is_some(), "{tool:?} {second}");
    }
    ws.cfg.shift_minutes = 120;
    tb.take_defaults(&ws.cfg);
    assert_eq!(tb.shift_minutes, 120);
    let _ = std::fs::remove_dir_all(&dir);
}
