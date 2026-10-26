// SPDX-License-Identifier: MPL-2.0

//! HTTP access shared by the online services.

use std::sync::LazyLock;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Identifies the application, as the services' usage policies ask.
const USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to create HTTP client")
});

#[derive(Debug, Clone)]
pub enum Error {
    Network(String),
    /// The service does not know the requested object.
    NotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(message) => write!(f, "network error: {message}"),
            Self::NotFound => f.write_str("not found"),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Network(err.to_string())
    }
}

/// Spaces requests to a service out by at least `interval`.
pub struct RateLimit {
    interval: Duration,
    last_request: Mutex<Option<Instant>>,
}

impl RateLimit {
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_request: Mutex::const_new(None),
        }
    }

    /// Waits until a request may be sent. The lock is held while waiting, so
    /// concurrent requests go out one per interval.
    pub async fn wait_turn(&self) {
        let mut last_request = self.last_request.lock().await;
        if let Some(last) = *last_request {
            tokio::time::sleep_until(last + self.interval).await;
        }
        *last_request = Some(Instant::now());
    }
}
