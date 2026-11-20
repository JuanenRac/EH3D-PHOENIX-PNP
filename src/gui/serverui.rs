// EH3D-PHOENIX-PNP - the server panel of the window, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The settings of the EH3D server, the state of the link and the login are the panel of `eh3d-view` of EH3D-COMMON, the same one that the other programs of the suite show, with its
//! words in the languages of the suite. This ties it to this program: the keys of the settings (`server_address`, `server_token`, `server_machine`, `server_timeout_s`,
//! `server_retries`, `server_mdns`) are what the panel shows and what it gives back when the person applies them, the program is `phoenix-pnp` for the server, and the lines that are
//! kept in the settings file are the keys above and `mode`, never the token. A login is made with the name and password of a person (the password is wiped as soon as the call is
//! made) and lives in the client, not in any file. Everything here works without a screen; `window` only draws the panel and hands its actions back.

use crate::config::MachineConfig;
use crate::error::Lang;
use crate::server::{Mode, APP};
use eh3d_serverclient as sc;
use eh3d_view::server_panel::{Action, ServerPanel};
use eh3d_view::texts::Lang as ViewLang;

/// The language of the panel for a language of the window.
pub fn view_lang(lang: Lang) -> ViewLang {
    match lang {
        Lang::En => ViewLang::En,
        Lang::Es => ViewLang::Es,
        Lang::De => ViewLang::De,
        Lang::Fr => ViewLang::Fr,
        Lang::It => ViewLang::It,
        Lang::Ja => ViewLang::Ja,
        Lang::Zh => ViewLang::Zh,
        Lang::Pt => ViewLang::Pt,
    }
}

/// The settings of the client for the keys of the settings of the program (the time of a try is rounded up to whole seconds, 1 to 120). A value the client does not accept is told.
pub fn settings_of(cfg: &MachineConfig) -> Result<sc::Settings, String> {
    let mut s = sc::Settings::default();
    let mut put = |key: &str, value: &str| s.set(key, value).map(|_| ()).map_err(|e| e.to_string());
    put("server_address", &cfg.server_address)?;
    put("server_token", &cfg.server_token)?;
    put("server_machine", &cfg.server_machine)?;
    put(
        "server_timeout_s",
        &(cfg.server_timeout_s.ceil().clamp(1.0, 120.0) as u64).to_string(),
    )?;
    put("server_retries", &cfg.server_retries.to_string())?;
    put("server_mdns", if cfg.server_mdns { "on" } else { "off" })?;
    Ok(s)
}

/// Puts the settings of the client in the keys of the settings of the program. `mode` follows the address: with one the program talks to the server, with none it works locally.
pub fn apply_to(cfg: &mut MachineConfig, s: &sc::Settings) {
    cfg.server_address = s.server_address.clone();
    cfg.server_token = s.server_token.expose().to_string();
    cfg.server_machine = s.server_machine.clone();
    cfg.server_timeout_s = s.server_timeout_s as f64;
    cfg.server_retries = s.server_retries;
    cfg.server_mdns = s.server_mdns;
    cfg.mode = if s.server_address.is_empty() {
        Mode::Local
    } else {
        Mode::Server
    };
}

/// The lines of the settings file that the panel keeps: the mode and the keys of the server but the token, which is never written by the window.
pub fn lines_of(cfg: &MachineConfig) -> Vec<(&'static str, String)> {
    vec![
        ("mode", cfg.mode.name().to_string()),
        ("server_address", cfg.server_address.clone()),
        ("server_machine", cfg.server_machine.clone()),
        ("server_timeout_s", cfg.server_timeout_s.to_string()),
        ("server_retries", cfg.server_retries.to_string()),
        (
            "server_mdns",
            if cfg.server_mdns { "on" } else { "off" }.to_string(),
        ),
    ]
}

/// What the person applied: the lines to keep in the settings file.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub lines: Vec<(&'static str, String)>,
}

/// The panel and the client it works on.
pub struct ServerUi {
    pub panel: ServerPanel,
    client: sc::Client,
}

impl std::fmt::Debug for ServerUi {
    // neither the token nor a password is written out
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerUi")
            .field("link", &self.client.link())
            .field("logged_in", &self.client.session().is_some())
            .finish()
    }
}

impl ServerUi {
    /// The panel for the settings of `cfg`, over the plain HTTP client of the suite. Settings the client does not accept start the panel empty, for the person to fix.
    pub fn new(cfg: &MachineConfig) -> ServerUi {
        let settings = settings_of(cfg).unwrap_or_default();
        let client = sc::Client::connect(settings.clone());
        ServerUi {
            panel: ServerPanel::new(&settings, APP),
            client,
        }
    }

    /// The panel for the settings of `cfg` over a client that was made apart (a server in memory, a tunnel).
    pub fn with_client(cfg: &MachineConfig, client: sc::Client) -> ServerUi {
        let settings = settings_of(cfg).unwrap_or_default();
        ServerUi {
            panel: ServerPanel::new(&settings, APP),
            client,
        }
    }

    /// Starts again from the settings of `cfg` (when another file of settings is loaded); a login is let go.
    pub fn reload(&mut self, cfg: &MachineConfig) {
        *self = ServerUi::new(cfg);
    }

    /// The client, to read the state of the link and the login.
    pub fn client(&self) -> &sc::Client {
        &self.client
    }

    /// Reads the state of the link again (the window calls it every frame; it only reads).
    pub fn refresh(&mut self) {
        self.panel.refresh(&self.client);
    }

    /// Does what the window asked. When the person applied the settings they are put in `cfg`, a client is made with them and the lines to keep in the file are given.
    pub fn handle(&mut self, action: Action, cfg: &mut MachineConfig) -> Option<Applied> {
        let settings = self.panel.handle(action, &mut self.client)?;
        apply_to(cfg, &settings);
        self.client = sc::Client::connect(settings);
        self.panel.refresh(&self.client);
        Some(Applied {
            lines: lines_of(cfg),
        })
    }
}
