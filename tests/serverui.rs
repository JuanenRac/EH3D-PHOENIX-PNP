//! The server panel of the window, without the window: the settings of the program are what the panel of `eh3d-view` shows, what it gives back when they are applied goes to the
//! configuration and to the file of settings (never the token), and a login is made and ended on a server in memory.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::AppState;
use eh3d_phoenix_pnp::gui::serverui::{apply_to, lines_of, settings_of, view_lang, ServerUi};
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::server::Mode;
use eh3d_phoenix_pnp::toolvalues;
use eh3d_serverclient as sc;
use eh3d_serverclient::sim::SimServer;
use eh3d_view::server_panel::{Action, LinkState, Notice};
use std::path::PathBuf;

fn config(text: &str) -> MachineConfig {
    MachineConfig::parse(text).unwrap()
}

fn served() -> MachineConfig {
    config("mode = server\nserver_address = 192.168.0.180:18200\nserver_machine = pnp-1\nserver_token = never-print-this\nserver_timeout_s = 2.5\nserver_retries = 3\nserver_mdns = on\n")
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_serverui_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn over_sim(cfg: &MachineConfig, sim: &SimServer) -> ServerUi {
    let client = sc::Client::new(settings_of(cfg).unwrap(), Box::new(sim.clone()))
        .with_sleeper(Box::new(sim.sleeper()))
        .with_clock(Box::new(sim.clock()));
    ServerUi::with_client(cfg, client)
}

#[test]
fn the_keys_of_the_settings_are_the_settings_of_the_client_and_back() {
    let cfg = served();
    let s = settings_of(&cfg).unwrap();
    assert_eq!(s.server_address, "192.168.0.180:18200");
    assert_eq!(s.server_machine, "pnp-1");
    assert_eq!(s.server_token.expose(), "never-print-this");
    // a try of 2.5 seconds is three whole seconds for the client
    assert_eq!(s.server_timeout_s, 3);
    assert_eq!((s.server_retries, s.server_mdns), (3, true));
    let mut back = MachineConfig::default();
    apply_to(&mut back, &s);
    assert_eq!(back.server_address, cfg.server_address);
    assert_eq!(back.server_token, cfg.server_token);
    assert_eq!(back.server_machine, cfg.server_machine);
    assert_eq!(back.server_timeout_s, 3.0);
    assert_eq!((back.server_retries, back.server_mdns), (3, true));
    assert_eq!(back.mode, Mode::Server);
}

#[test]
fn the_default_settings_start_the_panel_empty_and_local() {
    let cfg = MachineConfig::default();
    let s = settings_of(&cfg).unwrap();
    assert!(s.server_address.is_empty());
    let mut again = cfg.clone();
    apply_to(&mut again, &s);
    assert_eq!(again.mode, Mode::Local);
    let ui = ServerUi::new(&cfg);
    assert_eq!(ui.panel.state(), LinkState::Unknown);
    assert!(ui.panel.who().is_none());
}

#[test]
fn the_lines_that_are_kept_never_have_the_token() {
    let lines = lines_of(&served());
    let keys: Vec<&str> = lines.iter().map(|l| l.0).collect();
    assert_eq!(
        keys,
        [
            "mode",
            "server_address",
            "server_machine",
            "server_timeout_s",
            "server_retries",
            "server_mdns"
        ]
    );
    assert!(lines.iter().all(|(_, v)| !v.contains("never-print-this")));
    assert_eq!(lines[0].1, "server");
    assert_eq!(lines[3].1, "2.5");
    assert_eq!(lines[5].1, "on");
}

#[test]
fn the_languages_of_the_window_are_the_languages_of_the_panel() {
    for lang in Lang::ALL {
        let v = view_lang(lang);
        assert_eq!(format!("{v:?}").to_lowercase(), lang.code(), "{lang:?}");
    }
}

#[test]
fn a_person_logs_in_and_out_on_the_server_in_memory() {
    let sim = SimServer::new();
    let cfg =
        config("mode = server\nserver_address = 192.168.0.180:18200\nserver_machine = pnp-1\n");
    let mut ui = over_sim(&cfg, &sim);
    let mut cfg = cfg;
    ui.refresh();
    assert!(ui.panel.who().is_none());
    ui.handle(Action::OpenLogin, &mut cfg);
    assert!(ui.panel.login.open);
    ui.panel.login.name = "admin".into();
    ui.panel.login.password = "secret-password".into();
    ui.handle(Action::SubmitLogin, &mut cfg);
    // the password is wiped as soon as the call is made
    assert_eq!(ui.panel.login.password, "");
    assert_eq!(ui.panel.who().map(|w| w.name.as_str()), Some("admin"));
    assert_eq!(sim.open_logins(), 1);
    // the program is told to the server as phoenix-pnp
    assert_eq!(ui.panel.login.app, "phoenix-pnp");
    ui.handle(Action::Logout, &mut cfg);
    assert!(ui.panel.who().is_none());
    assert_eq!(sim.open_logins(), 0);
}

#[test]
fn a_wrong_password_is_told_and_nothing_is_logged_in() {
    let sim = SimServer::new();
    let mut cfg =
        config("mode = server\nserver_address = 192.168.0.180:18200\nserver_machine = pnp-1\n");
    let mut ui = over_sim(&cfg, &sim);
    ui.handle(Action::OpenLogin, &mut cfg);
    ui.panel.login.name = "admin".into();
    ui.panel.login.password = "not-the-password".into();
    ui.handle(Action::SubmitLogin, &mut cfg);
    assert!(ui.panel.who().is_none());
    assert!(ui.panel.login.open, "the window stays open with the error");
    assert!(ui
        .panel
        .login
        .error_text(eh3d_view::texts::Lang::En)
        .is_some());
    assert_eq!(ui.panel.login.password, "");
    assert_eq!(sim.open_logins(), 0);
}

#[test]
fn applying_the_settings_changes_the_configuration_and_the_mode_follows_the_address() {
    let mut cfg = MachineConfig::default();
    let mut ui = ServerUi::new(&cfg);
    ui.panel.form.address = "192.168.0.180:18200".into();
    ui.panel.form.machine = "pnp-2".into();
    ui.panel.form.token_input = "typed-token".into();
    ui.panel.form.timeout_s = "7".into();
    ui.panel.form.retries = "4".into();
    ui.panel.form.mdns = true;
    let applied = ui.handle(Action::Apply, &mut cfg).expect("applied");
    assert_eq!(cfg.server_address, "192.168.0.180:18200");
    assert_eq!(cfg.server_machine, "pnp-2");
    assert_eq!(cfg.server_token, "typed-token");
    assert_eq!(cfg.server_timeout_s, 7.0);
    assert_eq!((cfg.server_retries, cfg.server_mdns), (4, true));
    assert_eq!(cfg.mode, Mode::Server);
    assert!(applied.lines.iter().all(|(_, v)| v != "typed-token"));
    assert_eq!(ui.panel.notice, Notice::Applied);
    // an empty address is the program working on its own again
    ui.panel.form.address = String::new();
    ui.handle(Action::Apply, &mut cfg).expect("applied");
    assert_eq!(cfg.mode, Mode::Local);
    assert!(cfg.server_address.is_empty());
}

#[test]
fn settings_that_are_wrong_are_refused_and_change_nothing() {
    let mut cfg = served();
    let before = cfg.clone();
    let mut ui = ServerUi::new(&cfg);
    for (address, timeout, retries) in [
        ("no port here", "5", "2"),
        ("192.168.0.180:18200", "0", "2"),
        ("192.168.0.180:18200", "121", "2"),
        ("192.168.0.180:18200", "5", "6"),
        ("192.168.0.180:18200", "five", "2"),
    ] {
        ui.panel.form.address = address.into();
        ui.panel.form.timeout_s = timeout.into();
        ui.panel.form.retries = retries.into();
        assert!(
            ui.handle(Action::Apply, &mut cfg).is_none(),
            "{address} {timeout} {retries}"
        );
        assert!(matches!(ui.panel.notice, Notice::Invalid(_)));
        assert_eq!(cfg, before);
    }
    ui.handle(Action::Undo, &mut cfg);
    assert_eq!(ui.panel.form.address, "192.168.0.180:18200");
}

#[test]
fn the_lines_go_to_the_file_of_settings_without_the_token_and_the_rest_of_the_file_stays() {
    let dir = scratch("keep");
    let file = dir.join("machine.cfg");
    std::fs::write(
        &file,
        "# my machine\nnozzles = 2\r\nmode = local\n\nserver_machine = old\n",
    )
    .unwrap();
    let mut cfg = config(&std::fs::read_to_string(&file).unwrap());
    let mut ui = ServerUi::new(&cfg);
    ui.panel.form.address = "192.168.0.180:18200".into();
    ui.panel.form.machine = "pnp-3".into();
    ui.panel.form.token_input = "secret-typed".into();
    let applied = ui.handle(Action::Apply, &mut cfg).expect("applied");
    toolvalues::keep_lines(&file, &applied.lines).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("# my machine"));
    assert!(text.contains("nozzles = 2"));
    assert!(text.contains("mode = server"));
    assert!(text.contains("server_address = 192.168.0.180:18200"));
    assert!(text.contains("server_machine = pnp-3"));
    assert!(!text.contains("secret-typed"));
    assert!(!text.contains("server_token"));
    // the file reads back as the configuration that was applied, but for the token
    let read = config(&text);
    assert_eq!(read.server_machine, "pnp-3");
    assert_eq!(read.mode, Mode::Server);
    // a line that would make the file wrong is refused and the file is not touched
    let before = std::fs::read(&file).unwrap();
    let bad = vec![("server_retries", "99".to_string())];
    assert!(toolvalues::keep_lines(&file, &bad).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_the_panel_writes_out_has_the_token_or_the_password() {
    let sim = SimServer::new();
    let mut cfg = served();
    let mut ui = over_sim(&cfg, &sim);
    ui.panel.form.token_input = "typed-secret-token".into();
    ui.handle(Action::OpenLogin, &mut cfg);
    ui.panel.login.name = "admin".into();
    ui.panel.login.password = "typed-secret-password".into();
    let shown = format!("{ui:?} {:?} {:?}", ui.panel, ui.panel.form);
    assert!(!shown.contains("never-print-this"), "{shown}");
    assert!(!shown.contains("typed-secret-token"), "{shown}");
    assert!(!shown.contains("typed-secret-password"), "{shown}");
}

#[test]
fn the_window_state_keeps_the_applied_settings_in_the_file_that_was_loaded_and_not_the_token() {
    let dir = scratch("state");
    let file = dir.join("machine.cfg");
    std::fs::write(&file, "nozzles = 2\n").unwrap();
    let mut s = AppState::new(UiSettings::default());
    assert_eq!(s.load_paths(std::slice::from_ref(&file)), 1);
    s.server.panel.form.address = "192.168.0.180:18200".into();
    s.server.panel.form.machine = "pnp-4".into();
    s.server.panel.form.token_input = "state-secret".into();
    s.server_action(Action::Apply);
    assert_eq!(s.ws.cfg.server_machine, "pnp-4");
    assert_eq!(s.ws.cfg.server_token, "state-secret");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("server_machine = pnp-4"));
    assert!(!text.contains("state-secret"));
    assert!(s
        .log
        .entries()
        .iter()
        .any(|e| e.text.contains(&file.display().to_string())));
    // another file of settings loaded starts the panel again from it
    let other = dir.join("other.cfg");
    std::fs::write(
        &other,
        "server_address = 10.0.0.9:18200\nserver_machine = other\nmode = server\n",
    )
    .unwrap();
    assert_eq!(s.load_paths(&[other]), 1);
    assert_eq!(s.server.panel.form.address, "10.0.0.9:18200");
    assert_eq!(s.server.panel.form.machine, "other");
    // with no file loaded the settings stay in the session and the log says so
    let mut free = AppState::new(UiSettings::default());
    free.server.panel.form.address = "192.168.0.180:18200".into();
    free.server_action(Action::Apply);
    assert_eq!(free.ws.cfg.server_address, "192.168.0.180:18200");
    assert!(free.log.entries().len() == 1);
    let _ = std::fs::remove_dir_all(&dir);
}
