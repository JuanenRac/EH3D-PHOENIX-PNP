//! The panels of the slots and of the nodes, without a screen: they are in the frame, hidden at the start, named in the eight languages and reachable from the menus; the arrangement a person
//! saved before they existed still loads; and the bench behind them (a head node and a feeder node that are made up, the banks and the holds) behaves as the rules say.

use eh3d_dock::core::DockCore;
use eh3d_dock::menu::{Chosen, DockAction};
use eh3d_dock::model::PanelState;
use eh3d_dock::texts::Text8;
use eh3d_dock::workspaces::WarnKind;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::docking::{menu_bar, panel_defs, workspaces, NODES, SLOTS};
use eh3d_phoenix_pnp::gui::nodeview::{
    bank_text, light_of, state_text, Bench, Light, EXAMPLE_TABLE,
};
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::nodelink::{Hold, LinkError};
use eh3d_phoenix_pnp::slotbank::{Bank, State};

fn core() -> DockCore {
    DockCore::new(panel_defs(), workspaces())
}

#[test]
fn the_panels_of_the_slots_and_of_the_nodes_are_in_the_frame_hidden_and_named_in_eight_languages() {
    let defs = panel_defs();
    for id in [SLOTS, NODES] {
        let d = defs
            .iter()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        assert!(eh3d_dock::model::valid_id(id));
        let Text8(words) = &d.title;
        assert!(words.iter().all(|w| !w.trim().is_empty()), "{id}");
    }
    let mut c = core();
    for name in ["Setup", "Run", "Compact"] {
        let layout = c.workspaces().factory_layout(name).unwrap();
        for id in [SLOTS, NODES] {
            assert_eq!(layout.state_of(id), Some(PanelState::Hidden), "{name} {id}");
        }
    }
    for id in [SLOTS, NODES] {
        c.apply(&DockAction::ShowPanel(id.to_string())).unwrap();
        assert_eq!(c.layout().state_of(id), Some(PanelState::Docked));
    }
    assert!(c.layout_is_sound());
}

#[test]
fn the_tools_menu_opens_both_panels() {
    let bar = menu_bar();
    let c = core();
    let built = bar.resolve(&c.menu_context());
    let tools = built
        .iter()
        .find(|m| m.id == "tools")
        .expect("a tools menu");
    for id in ["tools.slots", "tools.nodes"] {
        let entry = tools
            .items
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        assert!(
            matches!(
                entry.activate(),
                Some(Chosen::Dock(DockAction::ShowPanel(_)))
            ),
            "{id}"
        );
    }
}

#[test]
fn an_arrangement_saved_before_the_panels_existed_loads_with_the_new_ones_hidden_and_keeps_its_shape(
) {
    // the frame as it was: every panel but the two new ones
    let old_defs: Vec<_> = panel_defs()
        .into_iter()
        .filter(|d| d.id != SLOTS && d.id != NODES)
        .collect();
    let mut old = DockCore::new(old_defs, workspaces());
    old.apply(&DockAction::HidePanel("parts".into())).unwrap();
    old.apply(&DockAction::SwitchWorkspace("Run".into()))
        .unwrap();
    old.apply(&DockAction::AutoHidePanel(
        "job".into(),
        eh3d_dock::geom::Side::Right,
    ))
    .unwrap();
    let saved = old.config_text("");
    assert!(saved.lines().all(|l| l.starts_with("dock.")), "{saved}");
    let mut now = core();
    now.load_config(&saved);
    // the frame tells that it put the two panels it did not find among the hidden ones (a repair); it does not refuse the arrangement, and nothing else is wrong with it
    let warned = now.take_warnings();
    assert!(
        warned.iter().all(|w| w.kind == WarnKind::Repaired),
        "the saved arrangement was refused: {warned:?}"
    );
    assert_eq!(now.workspaces().current_name(), "Run");
    assert!(matches!(
        now.layout().state_of("job"),
        Some(PanelState::AutoHidden(_))
    ));
    assert_eq!(now.layout().state_of(SLOTS), Some(PanelState::Hidden));
    assert_eq!(now.layout().state_of(NODES), Some(PanelState::Hidden));
    assert!(now.layout_is_sound());
    // and the first arrangement kept the change that was made in it
    now.apply(&DockAction::SwitchWorkspace("Setup".into()))
        .unwrap();
    assert_eq!(now.layout().state_of("parts"), Some(PanelState::Hidden));
    // what is saved now comes back too, with the new panel shown
    now.apply(&DockAction::ShowPanel(SLOTS.into())).unwrap();
    let again = now.config_text("");
    let mut third = core();
    third.load_config(&again);
    assert!(
        third.take_warnings().is_empty(),
        "an arrangement saved by this version is taken whole"
    );
    assert_eq!(third.layout().state_of(SLOTS), Some(PanelState::Docked));
}

#[test]
fn the_new_words_are_all_there_in_every_language_and_the_old_unused_one_has_its_row() {
    // every variant from the warning of the arrangement on has a row; the table is as long as the words
    assert_eq!(table().len(), Text::COUNT);
    for lang in Lang::ALL {
        for w in [
            Text::DockLayoutWarning,
            Text::SlotsPanel,
            Text::NodesPanel,
            Text::NodeSimOnly,
            Text::NodeXyBlocked,
        ] {
            assert!(!tr(w, lang).is_empty());
        }
    }
    assert_eq!(tr(Text::NodeAsk, Lang::Es), "Preguntar a los nodos");
    assert_eq!(tr(Text::NodeAsk, Lang::Pt), "Perguntar aos nós");
    assert_eq!(tr(Text::SlotJammed, Lang::De), "verklemmt");
    assert_eq!(tr(Text::NodesPanel, Lang::Ja), "ノード");
    // the words of the four states and of the five lights are different from one another in English
    let states: Vec<&str> = State::ALL
        .iter()
        .map(|s| tr(state_text(*s), Lang::En))
        .collect();
    let lights: Vec<&str> = Light::ALL.iter().map(|l| tr(l.text(), Lang::En)).collect();
    for set in [states, lights] {
        let mut sorted = set.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), set.len(), "{set:?}");
    }
    assert_ne!(
        tr(bank_text(Bank::Left), Lang::En),
        tr(bank_text(Bank::Right), Lang::En)
    );
}

#[test]
fn the_light_of_a_node_follows_how_the_last_question_ended_and_a_hold_wins() {
    let ok: Result<u8, LinkError> = Ok(1);
    let silent: Result<u8, LinkError> = Err(LinkError::Silent);
    let denied: Result<u8, LinkError> = Err(LinkError::Unauthorized);
    let refused: Result<u8, LinkError> = Err(LinkError::Refused {
        status: 409,
        why: None,
    });
    assert_eq!(light_of::<u8>(None, None), Light::Unknown);
    assert_eq!(light_of(Some(&ok), None), Light::Answers);
    assert_eq!(light_of(Some(&silent), None), Light::Silent);
    assert_eq!(light_of(Some(&denied), None), Light::Denied);
    assert_eq!(light_of(Some(&refused), None), Light::Answers);
    for hold in [Hold::Silent, Hold::Safe] {
        for r in [&ok, &silent, &denied] {
            assert_eq!(light_of(Some(r), Some(hold)), Light::Held);
        }
        assert_eq!(light_of::<u8>(None, Some(hold)), Light::Held);
    }
}

#[test]
fn the_bench_shows_the_banks_and_loads_a_table_or_says_why_not() {
    let cfg = MachineConfig::default();
    let mut b = Bench::new(&cfg);
    let cells = b.cells(Bank::Left);
    assert_eq!(cells.len(), 60);
    assert_eq!(cells[0].id.as_deref(), Some("F1"));
    assert!(cells[0].first && !cells[1].first && !cells[2].first && cells[3].first);
    assert_eq!(cells[0].state, State::Occupied);
    assert_eq!(cells[40].state, State::Free);
    assert!(cells[40].id.is_none());
    assert_eq!(b.cells(Bank::Right)[0].id.as_deref(), Some("F5"));
    // a table that is wrong leaves the one on show and says why
    assert!(!b.load_table("id,bank\nA,left\n", &cfg));
    assert!(b.message.as_deref().unwrap_or("").contains("exactly"));
    assert_eq!(b.layout.entries().len(), 6);
    assert!(b.load_table(
        "id,bank,first_slot,tape_mm,state,chain\nZ,right,1,8,,\n",
        &cfg
    ));
    assert!(b.message.is_none());
    assert_eq!(b.layout.entries().len(), 1);
    assert!(b.load_table(EXAMPLE_TABLE, &cfg));
    assert_eq!(b.layout.entries().len(), 6);
}

#[test]
fn the_machine_may_move_only_while_the_head_answers_clear_and_a_cut_link_holds_until_a_person_acknowledges(
) {
    let cfg = MachineConfig::default();
    let mut b = Bench::new(&cfg);
    assert_eq!(
        (b.head_light, b.feeders_light, b.xy_clear),
        (Light::Unknown, Light::Unknown, false)
    );
    b.ask();
    assert_eq!(
        (b.head_light, b.feeders_light, b.xy_clear),
        (Light::Answers, Light::Answers, true)
    );
    // a nozzle goes down: the head answers, the machine must stay still
    b.lower_nozzle(true);
    b.ask();
    assert_eq!((b.head_light, b.xy_clear), (Light::Answers, false));
    b.lower_nozzle(false);
    b.ask();
    assert!(b.xy_clear);
    // the link is cut: silent until the limit of the settings, then held
    b.cut_head(true);
    for _ in 1..cfg.node_silence_limit {
        b.ask();
        assert_eq!((b.head_light, b.xy_clear), (Light::Silent, false));
    }
    b.ask();
    assert_eq!(b.head_light, Light::Held);
    assert!(b.held());
    // acknowledging while it is still cut does not take
    b.acknowledge();
    assert_eq!((b.head_light, b.xy_clear), (Light::Held, false));
    // the link is back, but the machine stays still until a person acknowledges
    b.cut_head(false);
    b.ask();
    assert_eq!((b.head_light, b.xy_clear), (Light::Held, false));
    b.acknowledge();
    assert_eq!(
        (b.head_light, b.xy_clear, b.held()),
        (Light::Answers, true, false)
    );
}

#[test]
fn a_feeder_that_jams_in_the_chain_turns_its_slot_red_and_clears_when_the_cause_goes() {
    let cfg = MachineConfig::default();
    let mut b = Bench::new(&cfg);
    assert_eq!(b.ask(), 0);
    b.jam_feeder(true);
    assert_eq!(b.ask(), 1);
    assert_eq!(b.layout.find("F2").unwrap().state, State::Jammed);
    assert_eq!(b.cells(Bank::Left)[4].state, State::Jammed);
    // asking again changes nothing more
    assert_eq!(b.ask(), 0);
    b.jam_feeder(false);
    assert_eq!(b.ask(), 1);
    assert_eq!(b.layout.find("F2").unwrap().state, State::Occupied);
}
