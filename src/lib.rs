// EH3D-PHOENIX-PNP - pick and place assembly.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]
//! The parts of a board with their positions, the feeders that hold them and the order in which a machine with one or more nozzles places them; the configuration of the machine,
//! a planner that knows it, a simulated machine and a job that carries a plan out on it step by step, and the window that shows them (its logic needs no screen). It plans and simulates: it does not drive a machine.

pub mod catalog;
pub mod config;
pub mod error;
pub mod export;
pub mod feeders;
pub mod feeding;
pub mod fiducial;
pub mod gui;
pub mod job;
pub mod loadsheet;
pub mod marks;
pub mod model;
pub mod motion;
pub mod panel;
pub mod parts;
pub mod plan;
pub mod planner;
pub mod preflight;
pub mod report;
pub mod rotation;
pub mod sim;
pub mod units;
pub mod vision;
