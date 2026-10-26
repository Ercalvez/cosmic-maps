// SPDX-License-Identifier: MPL-2.0

//! What the app asks of the outside world: place details (Nominatim), search
//! (Photon), routes (Transitous) and the user's position (the location portal).

mod http;
pub mod location;
pub mod nominatim;
pub mod photon;
pub mod transitous;

pub use http::Error;

/// Joins the present parts, or returns `None` if there are none.
fn join<const N: usize>(parts: [Option<String>; N], separator: &str) -> Option<String> {
    let parts: Vec<String> = parts.into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(separator))
}
