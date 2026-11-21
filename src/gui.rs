// EH3D-PHOENIX-PNP - the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The window of the program. All it does besides drawing lives in modules that need no screen: the texts in eight languages, the colours, the workspace with the files that were
//! loaded, the plan and its problems, the simulated job, the geometry of the view, the tables, the guide and the settings; `app` ties them together. The panels, the arrangements and
//! the menus of the docking frame of the suite are data in `docking`, and the panels of the commands of the driver and the assistants are in `cmdpanel`. Only `window` (built with
//! the `gui` feature) draws, and it holds no logic of its own.

pub mod app;
pub mod browse;
pub mod cmdpanel;
pub mod docking;
pub mod jobtab;
pub mod log;
pub mod nodeview;
pub mod problems;
pub mod queuetab;
pub mod samples;
pub mod serverui;
pub mod session;
pub mod settings;
pub mod shot;
pub mod tables;
pub mod texts;
pub mod theme;
pub mod toolbox;
pub mod toolmore;
pub mod view;
pub mod wizard;
pub mod workspace;

#[cfg(feature = "gui")]
pub mod window;
