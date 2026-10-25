// SPDX-License-Identifier: MPL-2.0

//! What the app remembers between runs, in COSMIC's state directory, as COSMIC
//! Files does for its window.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// Where the map was looking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub zoom: f64,
}

#[derive(Debug, Default, Clone, CosmicConfigEntry, PartialEq)]
#[version = 1]
pub struct State {
    /// The map's view when the app was last closed.
    pub map: Option<MapPosition>,
}

impl State {
    /// The state saved by the last run, with the handle to save it again, if the
    /// state directory can be used.
    pub fn load(app_id: &str) -> (Option<cosmic_config::Config>, Self) {
        let handle = match cosmic_config::Config::new_state(app_id, Self::VERSION) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!("app state unavailable: {err}");
                return (None, Self::default());
            }
        };
        let state = Self::get_entry(&handle).unwrap_or_else(|(errors, state)| {
            // Missing on the first run.
            for why in errors.into_iter().filter(cosmic_config::Error::is_err) {
                tracing::warn!(%why, "error loading app state");
            }
            state
        });
        (Some(handle), state)
    }
}
