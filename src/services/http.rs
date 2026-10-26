// SPDX-License-Identifier: MPL-2.0

//! HTTP access shared by the online services.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
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

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to create HTTP client")
});

/// Why a service could not answer. Shared, so that it can travel in messages.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// No answer, an error status, or an answer that was not understood.
    #[error("request failed: {0}")]
    Request(#[source] Arc<reqwest::Error>),
    /// The service does not know the requested object.
    #[error("the service does not know the object")]
    NotFound,
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Request(Arc::new(err))
    }
}

/// Asks `url` for JSON, once `limit` allows a request.
pub async fn get_json<T: DeserializeOwned>(
    limit: &RateLimit,
    url: &str,
    query: &(impl Serialize + ?Sized),
) -> Result<T, Error> {
    limit.wait_turn().await;

    Ok(CLIENT
        .get(url)
        .query(query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
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
