// EH3D-PHOENIX-PNP - the panels, the workspaces and the menus of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! What the window hands to the frame of panels of the suite (`eh3d-dock`): which panels the program has and what each is called in the eight languages, the three arrangements it starts
//! with (Setup, Run and Compact), the menu bar as data (File, Edit, View, Tools, Window and Help) with the actions of the program, the shortcuts, the entries that are turned off and
//! the way the layout the person made is kept in the settings file of the window. None of it draws anything, so all of it is tested without a screen; `window` (built with `gui`)
//! only gives each panel the function that draws its inside.

use super::app::Tab;
use super::cmdpanel::Command;
use super::texts::{tr, Text};
use crate::error::Lang;
use eh3d_dock::about::{AboutBrand, Phoenix};
use eh3d_dock::core::{AboutInfo, PanelDef};
use eh3d_dock::geom::Side;
use eh3d_dock::menu::{DockAction, Entry, Gen, Item, Label, Menu, MenuBar, StdAction};
use eh3d_dock::model::{Layout, Node};
use eh3d_dock::sectors::Sector;
use eh3d_dock::shortcuts::Shortcut;
use eh3d_dock::stdmenus::{edit_menu, file_menu, help_menu, view_menu, window_menu, HelpOptions};
use eh3d_dock::texts::{DockText, Text8};
use eh3d_dock::workspaces::Workspace;
use eh3d_project::suitetext::S;

/// The look of the window «About»: which program of the family this is and its own picture, drawn in a ring of its colour.
pub fn about_brand() -> AboutBrand {
    AboutBrand {
        program: Phoenix::Pnp,
        logo_png: include_bytes!("../../images/brand/icon-256.png"),
    }
}

/// The panels of the sector of pick and place, by name.
pub const BOARD_VIEW: &str = "board_view";
pub const PARTS: &str = "parts";
pub const FEEDERS: &str = "feeders";
pub const JOB: &str = "job";
pub const CAMERA: &str = "camera";
pub const MACHINE: &str = "machine";
pub const CONSOLE: &str = "console";
/// The panels that the program adds to the ones of the sector.
pub const GUIDE: &str = "guide";
pub const PACKAGES: &str = "packages";
pub const PANELS: &str = "panels";
pub const BOARDS: &str = "boards";
pub const LOAD_SHEET: &str = "load_sheet";
pub const TOOLS: &str = "tools";
pub const PROBLEMS: &str = "problems";
/// The panels of the slots of the banks of feeders and of the nodes of the feeders and of the head. They start hidden in every arrangement.
pub const SLOTS: &str = "slots";
pub const NODES: &str = "nodes";

/// The attachments of a project of the suite: data sheets, manuals and other files, with their viewer.
pub const ATTACHMENTS: &str = eh3d_attachpanel::PANEL_ID;

/// The panels the program adds to the sector's seven, in the order they are listed.
pub const EXTRA_PANELS: [&str; 10] = [
    GUIDE,
    PACKAGES,
    PANELS,
    BOARDS,
    LOAD_SHEET,
    TOOLS,
    PROBLEMS,
    SLOTS,
    NODES,
    ATTACHMENTS,
];

/// A word of the window in the eight languages of the frame (English, Spanish, Italian, German, French, Portuguese of Portugal, Chinese, Japanese).
pub fn text8(word: Text) -> Text8 {
    Text8([
        tr(word, Lang::En),
        tr(word, Lang::Es),
        tr(word, Lang::It),
        tr(word, Lang::De),
        tr(word, Lang::Fr),
        tr(word, Lang::Pt),
        tr(word, Lang::Zh),
        tr(word, Lang::Ja),
    ])
}

/// The panel that shows what a tab of the earlier window showed.
pub fn tab_panel(tab: Tab) -> &'static str {
    match tab {
        Tab::Job => JOB,
        Tab::Parts => PARTS,
        Tab::Packages => PACKAGES,
        Tab::Feeders => FEEDERS,
        Tab::Panels => PANELS,
        Tab::Boards => BOARDS,
        Tab::LoadSheet => LOAD_SHEET,
        Tab::Machine => MACHINE,
        Tab::Tools => TOOLS,
        Tab::Problems => PROBLEMS,
        Tab::Log => CONSOLE,
    }
}

/// The tab that a panel stands for, when it stands for one.
pub fn panel_tab(panel: &str) -> Option<Tab> {
    Tab::ALL.into_iter().find(|t| tab_panel(*t) == panel)
}

/// The panels of the program: the seven of the sector, then the ones the program adds, then one for each of the fifteen commands.
pub fn panel_defs() -> Vec<PanelDef> {
    let mut defs = Sector::Pnp.panels();
    let mut add = |id: &str, word: Text, icon: &'static str, w: f32, h: f32| {
        defs.push(PanelDef::new(id, text8(word)).icon(icon).min(w, h));
    };
    add(GUIDE, Text::GuidePanel, "G", 200.0, 120.0);
    add(PACKAGES, Text::TabPackages, "K", 240.0, 140.0);
    add(PANELS, Text::TabPanels, "N", 240.0, 140.0);
    add(BOARDS, Text::TabBoards, "D", 240.0, 140.0);
    add(LOAD_SHEET, Text::TabLoadSheet, "S", 240.0, 140.0);
    add(TOOLS, Text::TabTools, "T", 280.0, 200.0);
    add(PROBLEMS, Text::TabProblems, "!", 240.0, 120.0);
    add(SLOTS, Text::SlotsPanel, "F", 280.0, 160.0);
    add(NODES, Text::NodesPanel, "H", 240.0, 160.0);
    defs.push(eh3d_attachpanel::panel_def());
    let mut add = |id: &str, word: Text, icon: &'static str, w: f32, h: f32| {
        defs.push(PanelDef::new(id, text8(word)).icon(icon).min(w, h));
    };
    for c in Command::ALL {
        add(c.panel_id(), c.title(), "", 280.0, 200.0);
    }
    defs
}

/// The three arrangements the window starts with. The names are the standard English ones; the frame shows them in the language of the window.
pub fn workspaces() -> Vec<Workspace> {
    let t = |ids: &[&str]| Node::tabs(ids);
    let setup = Node::row(
        0.24,
        Node::column(
            0.3,
            t(&[GUIDE]),
            t(&[
                PARTS,
                FEEDERS,
                PACKAGES,
                PANELS,
                BOARDS,
                LOAD_SHEET,
                ATTACHMENTS,
            ]),
        ),
        Node::row(
            0.74,
            Node::column(0.72, t(&[BOARD_VIEW, CAMERA]), t(&[CONSOLE, PROBLEMS])),
            t(&[MACHINE, JOB, TOOLS]),
        ),
    );
    let run = Node::row(
        0.68,
        Node::column(0.74, t(&[BOARD_VIEW]), t(&[CONSOLE, PROBLEMS])),
        Node::column(0.5, t(&[JOB]), t(&[MACHINE, CAMERA, LOAD_SHEET])),
    );
    vec![
        Workspace::new("Setup", Layout::with_root(setup)),
        Workspace::new("Run", Layout::with_root(run)),
        Workspace::new(
            "Compact",
            Layout::with_root(t(&[BOARD_VIEW, JOB]))
                .with_auto_hidden(PARTS, Side::Left)
                .with_auto_hidden(FEEDERS, Side::Left)
                .with_auto_hidden(CONSOLE, Side::Bottom),
        ),
    ]
}

/// What the window about the program shows.
pub fn about() -> AboutInfo {
    AboutInfo {
        app_name: "EH3D-PHOENIX-PNP".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        license: "GPL-3.0-or-later".to_string(),
        extra: Vec::new(),
    }
}

/// The three samples of the File menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKind {
    Mini,
    Files,
    Large,
}

/// What the program does when an entry of its menus is chosen (the standard entries come as `Std`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Std(StdAction),
    Sample(SampleKind),
    /// Open a project file of the suite (`.eh3d`) to see its attachments.
    SuiteOpen,
    Plan,
    Start,
    Pause,
    Step,
    Reset,
}

impl From<StdAction> for Cmd {
    fn from(a: StdAction) -> Cmd {
        Cmd::Std(a)
    }
}

fn own(word: Text) -> Label {
    Label::Own(text8(word))
}

/// A word of the project of the suite as the text of the frame (English, Spanish, Italian, German, French, Portuguese, Chinese,
/// Japanese).
fn suite_word(word: S) -> Label {
    let [en, es, de, fr, it, ja, zh, pt] = word.words();
    Label::Own(Text8([en, es, it, de, fr, pt, zh, ja]))
}

/// The menu bar: File (with the samples), Edit, View (one entry for each panel), Tools (plan and run, and the panels of the driver and the assistants), Window (the workspaces) and Help.
pub fn menu_bar() -> MenuBar<Cmd> {
    let mut file = file_menu::<Cmd>(true);
    let samples = Menu::new("file.samples", own(Text::Sample))
        .entry(Entry::app(
            "sample.mini",
            own(Text::SampleMini),
            Cmd::Sample(SampleKind::Mini),
        ))
        .entry(Entry::app(
            "sample.files",
            own(Text::SampleFiles),
            Cmd::Sample(SampleKind::Files),
        ))
        .entry(Entry::app(
            "sample.large",
            own(Text::SampleLarge),
            Cmd::Sample(SampleKind::Large),
        ));
    let at = file
        .items
        .iter()
        .position(|i| matches!(i, Item::Generated(Gen::Recent)))
        .map_or(file.items.len(), |p| p + 1);
    file.items.insert(at, Item::Submenu(samples));
    let suite = Menu::new("file.suite", suite_word(S::Menu))
        .entry(Entry::app(
            "file.suite.open",
            suite_word(S::MenuOpen),
            Cmd::SuiteOpen,
        ))
        .entry(Entry::dock(
            "file.suite.attachments",
            Label::Own(eh3d_attachpanel::panel_title()),
            DockAction::ShowPanel(ATTACHMENTS.to_string()),
        ));
    file.items.insert(at + 1, Item::Submenu(suite));

    let mut commands = Menu::new("tools.commands", own(Text::CommandsMenu));
    for c in Command::ALL {
        commands = commands.entry(Entry::dock(
            &format!("tools.{}", c.panel_id()),
            Label::Own(text8(c.title())),
            DockAction::ShowPanel(c.panel_id().to_string()),
        ));
    }
    let tools = Menu::new("tools", Label::Std(DockText::MenuTools))
        .entry(Entry::app("tools.plan", own(Text::Plan), Cmd::Plan).key(Shortcut::plain("F5")))
        .entry(Entry::app("tools.start", own(Text::Start), Cmd::Start).key(Shortcut::plain("F6")))
        .entry(Entry::app("tools.pause", own(Text::Pause), Cmd::Pause).key(Shortcut::plain("F7")))
        .entry(Entry::app("tools.step", own(Text::Step), Cmd::Step).key(Shortcut::plain("F8")))
        .entry(
            Entry::app("tools.reset", own(Text::Reset), Cmd::Reset).key(Shortcut::ctrl_shift("R")),
        )
        .sep()
        .entry(Entry::dock(
            "tools.slots",
            own(Text::SlotsPanel),
            DockAction::ShowPanel(SLOTS.to_string()),
        ))
        .entry(Entry::dock(
            "tools.nodes",
            own(Text::NodesPanel),
            DockAction::ShowPanel(NODES.to_string()),
        ))
        .sep()
        .submenu(commands);

    MenuBar::new(vec![
        file,
        edit_menu(),
        view_menu(),
        tools,
        window_menu(),
        help_menu(HelpOptions {
            online_help: true,
            check_updates: false,
        }),
    ])
}

/// What decides which entries of the menus are turned off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuFacts {
    pub can_plan: bool,
    pub can_start: bool,
    pub running: bool,
    pub can_step: bool,
    pub has_job: bool,
    pub can_export: bool,
}

/// The entries that are off at the moment, and the check marks, for `Dock::set_menu_state`. The entries the program has no action for (a new file, cut, copy and paste, redo, save)
/// are always off, so a shortcut cannot reach them.
pub fn menu_state(f: &MenuFacts) -> (Vec<String>, Vec<(String, bool)>) {
    let mut off: Vec<&str> = vec![
        "file.new",
        "file.save",
        "edit.redo",
        "edit.cut",
        "edit.copy",
        "edit.paste",
    ];
    for (needed, id) in [
        (f.can_plan, "tools.plan"),
        (f.can_start, "tools.start"),
        (f.running, "tools.pause"),
        (f.can_step, "tools.step"),
        (f.has_job, "tools.reset"),
        (f.can_export, "file.export"),
    ] {
        if !needed {
            off.push(id);
        }
    }
    (off.into_iter().map(str::to_string).collect(), Vec::new())
}

/// Shows a panel and brings its tab to the front of its group. A locked layout keeps its arrangement, but the tab on show can still change.
pub fn bring_forward(layout: &mut Layout, panel: &str) {
    let _ = layout.show(panel);
    let _ = layout.set_active(panel);
}

#[cfg(test)]
mod about_tests {
    use super::*;

    #[test]
    fn the_window_about_has_the_name_the_repository_and_the_picture_of_this_program() {
        let brand = about_brand();
        assert_eq!(brand.program, Phoenix::Pnp);
        assert_eq!(brand.program.name(), "EH3D-PHOENIX-PNP");
        assert_eq!(
            brand.program.repository(),
            "https://github.com/JuanenRac/EH3D-PHOENIX-PNP"
        );
        let png = brand.logo_png;
        assert_eq!(png.get(1..4), Some(&b"PNG"[..]));
        // the width and the height in the header of the picture
        assert_eq!(png.get(16..24), Some(&[0, 0, 1, 0, 0, 0, 1, 0][..]));
    }
}
