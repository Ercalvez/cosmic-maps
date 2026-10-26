// SPDX-License-Identifier: MPL-2.0

//! What identifies the app to the servers it talks to.

/// Sent with every request, as the services' usage policies ask.
pub const USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);
