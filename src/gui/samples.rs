// EH3D-PHOENIX-PNP - samples, from the smallest to the largest.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Samples to try the window with, from simple to big: a board of six parts, the files of `examples/` (a board with its feeders, tables, configuration and slots) and a generated board
//! of hundreds of parts. Each sample is a list of files as text, in the order they should be loaded (the configuration and the tables before the parts).

use std::fmt::Write;

/// A file of a sample: its name and its text.
pub type SampleFile = (String, String);

/// Six parts and three feeders.
pub fn mini() -> Vec<SampleFile> {
    let parts = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,10,0,top\nR2,10k,R_0603,20,10,90,top\nR3,10k,R_0603,30,10,180,top\nC1,100n,C_0603,10,20,0,top\nC2,100n,C_0603,20,20,0,top\nU1,MCU,QFN-32,40,15,0,top\n";
    let feeders = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,10\nF2,100n,C_0603,-20,20,10\nF3,MCU,QFN-32,-20,40,2\n";
    vec![
        ("mini_parts.csv".to_string(), parts.to_string()),
        ("mini_feeders.csv".to_string(), feeders.to_string()),
    ]
}

/// The files of `examples/`.
pub fn examples() -> Vec<SampleFile> {
    let files: [(&str, &str); 6] = [
        ("machine.cfg", include_str!("../../examples/machine.cfg")),
        ("packages.csv", include_str!("../../examples/packages.csv")),
        ("catalog.csv", include_str!("../../examples/catalog.csv")),
        ("board.pos", include_str!("../../examples/board.pos")),
        ("feeders.csv", include_str!("../../examples/feeders.csv")),
        ("slots.csv", include_str!("../../examples/slots.csv")),
    ];
    files
        .iter()
        .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
        .collect()
}

/// A generated board of `count` parts (at least 1, at most 5000) in rows of twenty, of four kinds, with a feeder for each kind that holds all it needs.
pub fn large(count: usize) -> Vec<SampleFile> {
    let count = count.clamp(1, 5000);
    let kinds = [
        ("10k", "R_0603"),
        ("100n", "C_0603"),
        ("LED", "LED_0603"),
        ("1u", "C_0805"),
    ];
    let mut parts = String::from("ref,val,package,x,y,rot,side\n");
    let mut needed = [0usize; 4];
    for i in 0..count {
        let kind = i % kinds.len();
        needed[kind] += 1;
        let (col, row) = (i % 20, i / 20);
        let _ = writeln!(
            parts,
            "P{},{},{},{},{},{},top",
            i + 1,
            kinds[kind].0,
            kinds[kind].1,
            5 + col * 5,
            5 + row * 5,
            (i % 4) * 90
        );
    }
    let mut feeders = String::from("id,value,package,x,y,count\n");
    for (k, (value, package)) in kinds.iter().enumerate() {
        let _ = writeln!(
            feeders,
            "F{},{},{},-20,{},{}",
            k + 1,
            value,
            package,
            10 + k * 10,
            needed[k] + 1
        );
    }
    vec![
        ("large_parts.csv".to_string(), parts),
        ("large_feeders.csv".to_string(), feeders),
    ]
}
