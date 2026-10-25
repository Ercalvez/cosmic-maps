// SPDX-License-Identifier: MPL-2.0

//! Online services: place details (Nominatim), search (Photon) and routes
//! (Transitous).

mod http;
pub mod nominatim;
pub mod photon;
pub mod transitous;

pub use http::Error;

/// Joins the present parts, or returns `None` if there are none.
fn join<const N: usize>(parts: [Option<String>; N], separator: &str) -> Option<String> {
    let parts: Vec<String> = parts.into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(separator))
}
