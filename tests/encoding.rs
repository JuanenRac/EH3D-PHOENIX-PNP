//! The encoding of the text files: UTF-16 with a byte order mark (what Windows programs and spreadsheets write as "Unicode text") is read like UTF-8, in the library, in the commands and in
//! the window; text that is neither is read as Latin-1 with a note, or refused where only text is wanted.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::toolbox::{Tool, Toolbox};
use eh3d_phoenix_pnp::gui::workspace::{read_file, FileKind, Workspace};
use eh3d_phoenix_pnp::parts::decode_text;
use std::path::PathBuf;
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> String {
    std::fs::read_to_string(root().join("examples").join(name)).unwrap()
}

fn utf16(text: &str, little: bool, bom: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if bom {
        out.extend(if little { [0xFF, 0xFE] } else { [0xFE, 0xFF] });
    }
    for unit in text.encode_utf16() {
        out.extend(if little {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    out
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_encoding_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

#[test]
fn the_bytes_of_a_file_are_decoded_by_their_byte_order_mark() {
    let text = "ref,val,package\nR1,10 kΩ,R_0603\nC1,100 nF,C_0603\nD1,LED ✓,LED_0603\n";
    // UTF-8 as it is, with or without the mark of its own
    assert_eq!(
        decode_text(text.as_bytes().to_vec()),
        (text.to_string(), false)
    );
    let mut with_mark = vec![0xEF, 0xBB, 0xBF];
    with_mark.extend(text.as_bytes());
    assert_eq!(decode_text(with_mark).0, format!("\u{feff}{text}"));
    // UTF-16 of both orders, the mark taken off, a character outside the first plane kept
    for little in [true, false] {
        assert_eq!(
            decode_text(utf16(text, little, true)),
            (text.to_string(), false)
        );
    }
    let astral = "ref\nU1 \u{1F4A1}\n";
    assert_eq!(decode_text(utf16(astral, true, true)).0, astral);
    // an odd byte at the end is let go, an empty text after the mark is empty, a lone surrogate does not stop the reading
    let mut odd = utf16("ab", true, true);
    odd.push(0x41);
    assert_eq!(decode_text(odd).0, "ab");
    assert_eq!(decode_text(vec![0xFF, 0xFE]), (String::new(), false));
    assert_eq!(decode_text(vec![0xFF, 0xFE, 0x00, 0xD8]).0, "\u{FFFD}");
    // nothing that has no mark is taken for UTF-16: it is Latin-1 and says so
    let (latin, flag) = decode_text(vec![b'1', b'0', b' ', 0xB5, b'F']);
    assert_eq!((latin.as_str(), flag), ("10 µF", true));
    assert_eq!(decode_text(Vec::new()), (String::new(), false));
}

#[test]
fn a_table_of_parts_in_utf16_gives_the_parts_of_the_one_in_utf8() {
    let dir = scratch("window");
    let utf8 = example("board.pos");
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", &utf8).unwrap();
    for (little, name) in [(true, "le.csv"), (false, "be.csv")] {
        let path = dir.join(name);
        std::fs::write(&path, utf16(&example("board.pos"), little, true)).unwrap();
        let (text, latin1) = read_file(&path, 1_000_000).unwrap();
        assert!(!latin1 && text == utf8, "{name}");
        let mut other = Workspace::new(Lang::En);
        assert_eq!(other.load_path(&path).unwrap(), FileKind::Parts);
        assert_eq!(other.parts, ws.parts, "{name}");
    }
    // the feeders too, with the delimiter and the header found as in UTF-8
    let path = dir.join("feeders.csv");
    std::fs::write(&path, utf16(&example("feeders.csv"), true, true)).unwrap();
    let mut other = Workspace::new(Lang::En);
    assert_eq!(other.load_path(&path).unwrap(), FileKind::Feeders);
    assert!(!other.feeders.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_commands_read_utf16_and_say_nothing_about_it() {
    let dir = scratch("cli");
    let board = dir.join("board16.pos");
    let feeders = dir.join("feeders16.csv");
    std::fs::write(&board, utf16(&example("board.pos"), true, true)).unwrap();
    std::fs::write(&feeders, utf16(&example("feeders.csv"), false, true)).unwrap();
    let plain = run(&["preflight", "examples/board.pos", "examples/feeders.csv"]);
    let wide = run(&[
        "preflight",
        board.to_str().unwrap(),
        feeders.to_str().unwrap(),
    ]);
    assert!(plain.status.success() && wide.status.success());
    assert_eq!(plain.stdout, wide.stdout);
    assert!(
        wide.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&wide.stderr)
    );
    // the same text in Latin-1 is read, with the note that it was
    let latin = dir.join("latin.csv");
    std::fs::write(
        &latin,
        b"ref,val,package,x,y,rot,side\nR1,10 k\xB5,R_0603,1,1,0,top\n",
    )
    .unwrap();
    let o = run(&["preflight", latin.to_str().unwrap(), "examples/feeders.csv"]);
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("read as Latin-1"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tool_reads_utf16_from_a_file_and_refuses_what_is_not_text() {
    let dir = scratch("tool");
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Camera;
    let points = example("camera_points.csv");
    let path = dir.join("points.csv");
    std::fs::write(&path, utf16(&points, true, true)).unwrap();
    tb.path_text = path.to_string_lossy().into_owned();
    assert!(tb.read_file(), "{}", tb.error);
    assert_eq!(tb.camera_text, points);
    // bytes that are neither UTF-8 nor UTF-16 are not text
    let bin = dir.join("bin.csv");
    std::fs::write(&bin, [0x80u8, 0xff, 0x00, 0x80]).unwrap();
    tb.path_text = bin.to_string_lossy().into_owned();
    assert!(!tb.read_file());
    assert!(tb.error.contains("not UTF-8 or UTF-16"), "{}", tb.error);
    let _ = std::fs::remove_dir_all(&dir);
}
