// The frame of panels of the window: the panels, the three arrangements, the menu bar, the layout kept in the settings file and the way a tab comes forward, all without a screen.
use eh3d_dock::core::DockCore;
use eh3d_dock::menu::{Chosen, DockAction, Kind, Resolved, StdAction};
use eh3d_dock::model::{Layout, PanelState};
use eh3d_dock::sectors::workspace_title;
use eh3d_dock::shortcuts::Shortcut;
use eh3d_dock::texts::Text8;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::Tab;
use eh3d_phoenix_pnp::gui::cmdpanel::Command;
use eh3d_phoenix_pnp::gui::docking::{
    bring_forward, menu_bar, menu_state, panel_defs, panel_tab, tab_panel, text8, workspaces, Cmd,
    MenuFacts, SampleKind, EXTRA_PANELS,
};
use eh3d_phoenix_pnp::gui::serverui::view_lang;
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::gui::texts::Text;

fn core() -> DockCore {
    DockCore::new(panel_defs(), workspaces())
}

const SECTOR: [&str; 7] = [
    "board_view",
    "parts",
    "feeders",
    "job",
    "camera",
    "machine",
    "console",
];

#[test]
fn the_window_has_the_seven_panels_of_the_sector_and_those_that_the_tabs_had() {
    let defs = panel_defs();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    for want in SECTOR {
        assert!(ids.contains(&want), "{want}");
    }
    for want in EXTRA_PANELS {
        assert!(ids.contains(&want), "{want}");
    }
    assert_eq!(ids.len(), 7 + EXTRA_PANELS.len() + Command::ALL.len());
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "a name is used twice");
    for id in &ids {
        assert!(eh3d_dock::model::valid_id(id), "{id}");
    }
}

#[test]
fn every_tab_of_the_earlier_window_has_its_panel_and_back() {
    let ids: Vec<String> = panel_defs().into_iter().map(|d| d.id).collect();
    for tab in Tab::ALL {
        let panel = tab_panel(tab);
        assert!(ids.iter().any(|i| i == panel), "{panel}");
        assert_eq!(panel_tab(panel), Some(tab));
    }
    assert_eq!(panel_tab("cmd_bringup"), None);
    assert_eq!(panel_tab("nothing"), None);
}

#[test]
fn every_panel_is_named_in_the_eight_languages() {
    for d in panel_defs() {
        let Text8(words) = d.title;
        for (i, w) in words.iter().enumerate() {
            assert!(!w.trim().is_empty(), "{} language {i}", d.id);
        }
        // the names of a panel are not all the same word in the eight languages (apart from a few proper names)
        let mut distinct = words.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() >= 3, "{} {:?}", d.id, words);
    }
    // the order of the frame is English, Spanish, Italian, German, French, Portuguese of Portugal, Chinese, Japanese
    let Text8(w) = text8(Text::TabFeeders);
    assert_eq!(w[0], "Feeders");
    assert_eq!(w[1], "Alimentadores");
    assert_eq!(w[2], "Alimentatori");
    assert_eq!(w[3], "Zuführungen");
    assert_eq!(w[4], "Alimentateurs");
    assert_eq!(w[6], "供料器");
    assert_eq!(w[7], "フィーダー");
}

#[test]
fn the_three_arrangements_are_setup_run_and_compact_and_each_is_sound() {
    let c = core();
    assert_eq!(c.workspaces().names(), ["Setup", "Run", "Compact"]);
    for name in ["Setup", "Run", "Compact"] {
        let layout = c.workspaces().factory_layout(name).unwrap();
        assert!(layout.validate(&known(&c)).is_empty(), "{name}");
    }
    assert!(c.layout_is_sound());
}

fn known(c: &DockCore) -> Vec<&str> {
    c.defs().iter().map(|d| d.id.as_str()).collect()
}

#[test]
fn setup_shows_what_a_person_sets_up_with_and_run_shows_the_job() {
    let c = core();
    let setup = c.workspaces().factory_layout("Setup").unwrap();
    for id in SECTOR.iter().chain(&[
        "guide",
        "packages",
        "panels",
        "boards",
        "load_sheet",
        "tools",
        "problems",
    ]) {
        assert_eq!(setup.state_of(id), Some(PanelState::Docked), "setup {id}");
    }
    let run = c.workspaces().factory_layout("Run").unwrap();
    for id in [
        "board_view",
        "job",
        "console",
        "problems",
        "machine",
        "camera",
    ] {
        assert_eq!(run.state_of(id), Some(PanelState::Docked), "run {id}");
    }
    for id in ["parts", "feeders", "tools"] {
        assert_eq!(run.state_of(id), Some(PanelState::Hidden), "run {id}");
    }
    let compact = c.workspaces().factory_layout("Compact").unwrap();
    assert_eq!(compact.state_of("board_view"), Some(PanelState::Docked));
    assert_eq!(compact.state_of("job"), Some(PanelState::Docked));
    for id in ["parts", "feeders", "console"] {
        assert!(
            matches!(compact.state_of(id), Some(PanelState::AutoHidden(_))),
            "compact {id}"
        );
    }
}

#[test]
fn the_panels_of_the_commands_start_hidden_in_every_arrangement_and_can_be_shown() {
    let mut c = core();
    for name in ["Setup", "Run", "Compact"] {
        let layout = c.workspaces().factory_layout(name).unwrap();
        for cmd in Command::ALL {
            assert_eq!(
                layout.state_of(cmd.panel_id()),
                Some(PanelState::Hidden),
                "{name} {}",
                cmd.panel_id()
            );
        }
    }
    for cmd in Command::ALL {
        c.apply(&DockAction::ShowPanel(cmd.panel_id().to_string()))
            .unwrap();
        assert_eq!(
            c.layout().state_of(cmd.panel_id()),
            Some(PanelState::Docked)
        );
    }
    assert!(c.layout_is_sound());
}

#[test]
fn the_arrangements_have_their_names_in_each_language() {
    assert_eq!(
        workspace_title("Setup").unwrap().get(view_lang(Lang::Es)),
        "Configurar"
    );
    assert_eq!(
        workspace_title("Run").unwrap().get(view_lang(Lang::Es)),
        "Ejecutar"
    );
    assert_eq!(
        workspace_title("Compact").unwrap().get(view_lang(Lang::Es)),
        "Compacto"
    );
    for lang in Lang::ALL {
        for name in ["Setup", "Run", "Compact"] {
            assert!(!workspace_title(name)
                .unwrap()
                .get(view_lang(lang))
                .is_empty());
        }
    }
}

fn items<'a>(menus: &'a [eh3d_dock::menu::ResolvedMenu<Cmd>], id: &str) -> &'a [Resolved<Cmd>] {
    &menus.iter().find(|m| m.id == id).unwrap().items
}

#[test]
fn the_menu_bar_is_file_edit_view_tools_window_and_help_and_has_no_clash() {
    let bar = menu_bar();
    assert!(bar.validate().is_empty(), "{:?}", bar.validate());
    let c = core();
    for lang in Lang::ALL {
        let mut ctx = c.menu_context();
        ctx.lang = view_lang(lang);
        assert!(
            bar.conflicts(&ctx).is_empty(),
            "{lang:?} {:?}",
            bar.conflicts(&ctx)
        );
        let built = bar.resolve(&ctx);
        let ids: Vec<&str> = built.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["file", "edit", "view", "tools", "window", "help"]);
        for m in &built {
            assert!(!m.title.is_empty());
        }
    }
    let mut ctx = c.menu_context();
    ctx.lang = view_lang(Lang::Es);
    let titles: Vec<String> = bar.resolve(&ctx).into_iter().map(|m| m.title).collect();
    assert_eq!(
        titles,
        [
            "Archivo",
            "Editar",
            "Ver",
            "Herramientas",
            "Ventana",
            "Ayuda"
        ]
    );
}

#[test]
fn the_view_menu_lists_every_panel_and_the_tools_menu_opens_the_eleven_commands() {
    let bar = menu_bar();
    let c = core();
    let built = bar.resolve(&c.menu_context());
    let view = items(&built, "view");
    // one checked entry for each panel (and the one of the lock of the layout)
    let checked = view.iter().filter(|i| i.checked.is_some()).count();
    assert_eq!(checked, panel_defs().len() + 1);
    let tools = items(&built, "tools");
    let commands = tools
        .iter()
        .find_map(|i| match &i.kind {
            Kind::Submenu(sub) => Some(sub),
            _ => None,
        })
        .expect("a submenu of commands");
    assert_eq!(commands.len(), 11);
    for (entry, cmd) in commands.iter().zip(Command::ALL) {
        assert_eq!(
            entry.activate(),
            Some(&Chosen::Dock(DockAction::ShowPanel(
                cmd.panel_id().to_string()
            )))
        );
    }
}

#[test]
fn the_file_menu_holds_the_three_samples_and_the_actions_come_back_as_the_programs() {
    let bar = menu_bar();
    let c = core();
    let built = bar.resolve(&c.menu_context());
    let file = items(&built, "file");
    let samples = file
        .iter()
        .find_map(|i| match &i.kind {
            Kind::Submenu(sub) if i.id == "file.samples" => Some(sub),
            _ => None,
        })
        .expect("the samples");
    let got: Vec<Option<&Chosen<Cmd>>> = samples.iter().map(|i| i.activate()).collect();
    assert_eq!(
        got,
        [
            Some(&Chosen::App(Cmd::Sample(SampleKind::Mini))),
            Some(&Chosen::App(Cmd::Sample(SampleKind::Files))),
            Some(&Chosen::App(Cmd::Sample(SampleKind::Large))),
        ]
    );
    let ctx = c.menu_context();
    assert_eq!(
        bar.shortcut_action(&ctx, &Shortcut::plain("F5")),
        Some(Chosen::App(Cmd::Plan))
    );
    assert_eq!(
        bar.shortcut_action(&ctx, &Shortcut::plain("F6")),
        Some(Chosen::App(Cmd::Start))
    );
    assert_eq!(
        bar.shortcut_action(&ctx, &Shortcut::ctrl("O")),
        Some(Chosen::App(Cmd::Std(StdAction::Open)))
    );
}

#[test]
fn an_entry_that_is_off_gives_nothing_even_to_its_shortcut() {
    let bar = menu_bar();
    let c = core();
    // nothing loaded: nothing can be planned, started, paused, stepped, reset or exported
    let (off, checks) = menu_state(&MenuFacts::default());
    for id in [
        "tools.plan",
        "tools.start",
        "tools.pause",
        "tools.step",
        "tools.reset",
        "file.export",
    ] {
        assert!(off.iter().any(|o| o == id), "{id}");
    }
    let mut ctx = c.menu_context();
    ctx.disabled = off;
    ctx.checks = checks;
    for key in ["F5", "F6", "F7", "F8"] {
        assert_eq!(
            bar.shortcut_action(&ctx, &Shortcut::plain(key)),
            None,
            "{key}"
        );
    }
    // with a plan, the entries that need one are on again
    let ready = MenuFacts {
        can_plan: true,
        can_start: true,
        running: false,
        can_step: true,
        has_job: false,
        can_export: true,
    };
    let (off, _) = menu_state(&ready);
    assert!(!off
        .iter()
        .any(|o| o == "tools.plan" || o == "tools.start" || o == "file.export"));
    assert!(off.iter().any(|o| o == "tools.pause"));
    ctx.disabled = off;
    assert_eq!(
        bar.shortcut_action(&ctx, &Shortcut::plain("F5")),
        Some(Chosen::App(Cmd::Plan))
    );
    // the entries the program has no action for stay off in every state
    for facts in [MenuFacts::default(), ready] {
        let (off, _) = menu_state(&facts);
        for id in [
            "file.new",
            "file.save",
            "edit.redo",
            "edit.cut",
            "edit.copy",
            "edit.paste",
        ] {
            assert!(off.iter().any(|o| o == id), "{id}");
        }
    }
}

#[test]
fn the_layout_a_person_makes_is_kept_in_the_settings_file_and_comes_back() {
    let mut c = core();
    c.apply(&DockAction::HidePanel("parts".into())).unwrap();
    c.apply(&DockAction::SwitchWorkspace("Run".into())).unwrap();
    c.apply(&DockAction::AutoHidePanel(
        "job".into(),
        eh3d_dock::geom::Side::Right,
    ))
    .unwrap();
    let lines = c.config_text("");
    assert!(lines.lines().all(|l| l.starts_with("dock.")), "{lines}");
    // the lines travel through the settings of the window, with the other settings around them
    let settings = UiSettings {
        lang: Lang::Pt,
        dock: lines.clone(),
        ..UiSettings::default()
    };
    let text = settings.to_text();
    let back = UiSettings::parse(&text);
    assert_eq!(back.lang, Lang::Pt);
    assert_eq!(back.dock.trim_end(), lines.trim_end());
    let mut again = core();
    again.load_config(&back.dock);
    assert_eq!(again.workspaces().current_name(), "Run");
    assert!(matches!(
        again.layout().state_of("job"),
        Some(PanelState::AutoHidden(_))
    ));
    assert!(again.take_warnings().is_empty());
    // the first workspace kept the change that was made in it
    let mut setup = core();
    setup.load_config(&back.dock);
    setup
        .apply(&DockAction::SwitchWorkspace("Setup".into()))
        .unwrap();
    assert_eq!(setup.layout().state_of("parts"), Some(PanelState::Hidden));
}

#[test]
fn a_damaged_layout_or_one_of_another_version_gives_the_factory_one_and_a_warning() {
    let mut damaged = core();
    damaged.load_config(
        "dock.version = 1\ndock.current = 0\ndock.ws.1.name = Setup\ndock.ws.1.tree = H200(T0(((\n",
    );
    assert!(damaged.layout_is_sound());
    assert_eq!(
        damaged.layout().state_of("board_view"),
        Some(PanelState::Docked)
    );
    assert!(!damaged.warnings().is_empty());
    let mut other = core();
    other.load_config("dock.version = 99\ndock.current = 1\n");
    assert_eq!(other.workspaces().current_name(), "Setup");
    assert!(other
        .warnings()
        .iter()
        .any(|w| w.name() == "layout_other_version"));
    // a panel that this version does not know is dropped and the rest stays
    let mut unknown = core();
    unknown.load_config("dock.version = 1\ndock.current = 0\ndock.ws.1.name = Setup\ndock.ws.1.tree = H500(T0(board_view,not_a_panel),T0(job))\n");
    assert!(unknown.layout_is_sound());
    assert_eq!(unknown.layout().state_of("job"), Some(PanelState::Docked));
}

#[test]
fn the_settings_file_keeps_many_lines_of_the_frame_and_refuses_the_hostile_ones() {
    let mut text = String::from("language = es\n");
    for i in 0..500 {
        text.push_str(&format!("dock.recent.{i} = C:\\files\\board{i}.pos\n"));
    }
    let s = UiSettings::parse(&text);
    assert_eq!(s.lang, Lang::Es);
    assert_eq!(s.dock.lines().count(), 500);
    // a line with a control character is left out, and a huge file is cut at the limit
    let hostile = format!(
        "dock.version = 1\ndock.x = a\u{7}b\n{}",
        "dock.pad = x\n".repeat(100_000)
    );
    let s = UiSettings::parse(&hostile);
    assert!(!s.dock.contains('\u{7}'));
    assert!(s.dock.len() <= eh3d_phoenix_pnp::gui::settings::MAX_DOCK_BYTES);
    assert!(s.dock.starts_with("dock.version = 1\n"));
}

#[test]
fn the_time_a_command_may_run_is_a_setting_kept_between_5_and_600_seconds() {
    assert_eq!(UiSettings::default().command_timeout_s, 60);
    assert_eq!(
        UiSettings::parse("command_timeout_s = 1\n").command_timeout_s,
        5
    );
    assert_eq!(
        UiSettings::parse("command_timeout_s = 90\n").command_timeout_s,
        90
    );
    assert_eq!(
        UiSettings::parse("command_timeout_s = 99999\n").command_timeout_s,
        600
    );
    assert_eq!(
        UiSettings::parse("command_timeout_s = soon\n").command_timeout_s,
        60
    );
    assert!(UiSettings::default()
        .to_text()
        .contains("command_timeout_s = 60\n"));
}

#[test]
fn a_tab_comes_to_the_front_of_its_group_and_a_hidden_panel_is_shown() {
    let mut c = core();
    // `parts` and `feeders` share a group: bring `feeders` forward, then `parts` back
    bring_forward(c.layout_mut(), "feeders");
    assert_eq!(c.layout().active_of("feeders"), Some("feeders"));
    bring_forward(c.layout_mut(), "parts");
    assert_eq!(c.layout().active_of("parts"), Some("parts"));
    // a panel that is hidden is shown first
    c.apply(&DockAction::HidePanel("tools".into())).unwrap();
    assert_eq!(c.layout().state_of("tools"), Some(PanelState::Hidden));
    bring_forward(c.layout_mut(), "tools");
    assert_eq!(c.layout().state_of("tools"), Some(PanelState::Docked));
    assert_eq!(c.layout().active_of("tools"), Some("tools"));
    // a locked layout keeps its arrangement, but the tab on show still changes
    c.layout_mut().set_locked(true);
    bring_forward(c.layout_mut(), "feeders");
    assert_eq!(c.layout().active_of("feeders"), Some("feeders"));
    c.layout_mut().set_locked(false);
    // a name that is not a panel changes nothing
    let before: Layout = c.layout().clone();
    bring_forward(c.layout_mut(), "not_a_panel");
    assert_eq!(*c.layout(), before);
}

#[test]
fn the_word_of_a_panel_follows_the_language_of_the_window() {
    let mut c = core();
    for (lang, want) in [
        (Lang::En, "Parts"),
        (Lang::Es, "Piezas"),
        (Lang::De, "Teile"),
        (Lang::Ja, "部品"),
    ] {
        c.lang = view_lang(lang);
        assert_eq!(c.title("parts"), want, "{lang:?}");
    }
}
