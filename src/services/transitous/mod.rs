// SPDX-License-Identifier: MPL-2.0

//! Walking, cycling, driving and public transit routes from Transitous, the
//! community-run MOTIS service GNOME Maps also uses for public transit.
//!
//! Usage policy (transitous.org/api): open-source, non-commercial apps sending a
//! `User-Agent` with a contact, with visible attribution of transitous.org/sources
//! and OpenStreetMap. Maintainers ask to be contacted before heavy `/plan` use.

use std::time::Duration;

use self::raw::Response;
use super::http::{Error, RateLimit, get_json};
use crate::domain::route::{Plan, RouteMode};
use crate::geo::LatLon;

const PLAN_URL: &str = "https://api.transitous.org/api/v6/plan";
/// Transitous gives no numbers; plans are only requested on user actions, so one
/// per second is plenty.
static TRANSITOUS: RateLimit = RateLimit::new(Duration::from_secs(1));

/// Routes from `from` to `to`, both latitude and longitude, leaving now, or the
/// earlier or later page given by a cursor from a previous [`Plan`].
pub async fn plan(
    from: LatLon,
    to: LatLon,
    mode: RouteMode,
    page: Option<String>,
) -> Result<Plan, Error> {
    let response: Response = get_json(&TRANSITOUS, PLAN_URL, &query(from, to, mode, page)).await?;

    Ok(response.into_plan())
}

/// The `/plan` query parameters.
fn query(
    from: LatLon,
    to: LatLon,
    mode: RouteMode,
    page: Option<String>,
) -> Vec<(&'static str, String)> {
    // Without transit modes, only the route all the way by street is computed.
    let transit_modes = if mode == RouteMode::Transit {
        "TRANSIT"
    } else {
        ""
    };
    let mut query = vec![
        ("fromPlace", format!("{},{}", from.lat, from.lon)),
        ("toPlace", format!("{},{}", to.lat, to.lon)),
        ("directModes", direct_mode(mode).to_owned()),
        ("transitModes", transit_modes.to_owned()),
        ("maxDirectTime", max_direct_time(mode).to_owned()),
    ];
    if mode == RouteMode::Bike {
        // Flatter streets when the detour is small.
        query.push(("elevationCosts", "LOW".to_owned()));
    }
    if let Some(page) = page {
        query.push(("pageCursor", page));
    }
    query
}

/// The MOTIS mode of the route all the way by street.
fn direct_mode(mode: RouteMode) -> &'static str {
    match mode {
        RouteMode::Walk | RouteMode::Transit => "WALK",
        RouteMode::Bike => "BIKE",
        RouteMode::Car => "CAR",
    }
}

/// Longest route all the way by street searched, in seconds. The server's default
/// of 30 minutes leaves out walks across a city.
fn max_direct_time(mode: RouteMode) -> &'static str {
    match mode {
        RouteMode::Walk | RouteMode::Transit => "7200",
        RouteMode::Bike => "14400",
        RouteMode::Car => "43200",
    }
}

mod polyline;
mod raw;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_follow_the_mode() {
        let louvre = LatLon::new(48.8606, 2.3376);
        let gare_de_lyon = LatLon::new(48.8443, 2.3744);
        let param = |mode, key| {
            query(louvre, gare_de_lyon, mode, None)
                .into_iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value)
        };

        assert_eq!(
            param(RouteMode::Walk, "directModes").as_deref(),
            Some("WALK")
        );
        assert_eq!(param(RouteMode::Walk, "transitModes").as_deref(), Some(""));
        assert_eq!(
            param(RouteMode::Transit, "directModes").as_deref(),
            Some("WALK")
        );
        assert_eq!(
            param(RouteMode::Transit, "transitModes").as_deref(),
            Some("TRANSIT")
        );
        assert_eq!(
            param(RouteMode::Bike, "directModes").as_deref(),
            Some("BIKE")
        );
        assert_eq!(
            param(RouteMode::Bike, "elevationCosts").as_deref(),
            Some("LOW")
        );
        assert_eq!(param(RouteMode::Car, "directModes").as_deref(), Some("CAR"));
        assert_eq!(param(RouteMode::Car, "elevationCosts"), None);
        assert_eq!(
            param(RouteMode::Car, "maxDirectTime").as_deref(),
            Some("43200")
        );
        assert_eq!(
            param(RouteMode::Walk, "fromPlace").as_deref(),
            Some("48.8606,2.3376")
        );

        let page = query(
            louvre,
            gare_de_lyon,
            RouteMode::Transit,
            Some("LATER|1".to_owned()),
        );
        assert!(page.contains(&("pageCursor", "LATER|1".to_owned())));
    }
}
