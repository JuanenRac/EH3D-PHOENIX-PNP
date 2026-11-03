// EH3D-PHOENIX-PNP - pick and place assembly.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]
//! The parts of a board with their positions, the feeders that hold them and the order in which a machine with one or more nozzles places them.

pub mod feeders;
pub mod parts;
pub mod plan;
