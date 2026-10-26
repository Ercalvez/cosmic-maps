// SPDX-License-Identifier: MPL-2.0

//! Walking, cycling, driving and public transit routes from Transitous, the
//! community-run MOTIS service GNOME Maps also uses for public transit.
//!
//! Usage policy (transitous.org/api): open-source, non-commercial apps sending a
//! `User-Agent` with a contact, with visible attribution of transitous.org/sources
//! and OpenStreetMap. Maintainers ask to be contacted before heavy `/plan` use.

use std::time::Duration;

use cosmic::iced::Color;
use jiff::Timestamp;
use serde::Deserialize;

use super::http::{CLIENT, Error, RateLimit};
use crate::domain::route::{
    Itinerary, Leg, Line, PassedStop, Plan, RouteMode, Step, Stop, is_transit_mode,
};
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
    TRANSITOUS.wait_turn().await;

    let response: Response = CLIENT
        .get(PLAN_URL)
        .query(&query(from, to, mode, page))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

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

/// The parts of a MOTIS `/api/v6/plan` response that are used.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    #[serde(default)]
    direct: Vec<RawItinerary>,
    #[serde(default)]
    itineraries: Vec<RawItinerary>,
    previous_page_cursor: Option<String>,
    next_page_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawItinerary {
    duration: u64,
    start_time: Timestamp,
    end_time: Timestamp,
    #[serde(default)]
    transfers: u32,
    legs: Vec<RawLeg>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLeg {
    mode: String,
    from: RawPlace,
    to: RawPlace,
    start_time: Timestamp,
    end_time: Timestamp,
    scheduled_start_time: Option<Timestamp>,
    scheduled_end_time: Option<Timestamp>,
    duration: u64,
    distance: Option<f64>,
    route_short_name: Option<String>,
    route_color: Option<String>,
    route_text_color: Option<String>,
    headsign: Option<String>,
    agency_name: Option<String>,
    leg_geometry: Option<RawPolyline>,
    #[serde(default)]
    steps: Vec<RawStep>,
    #[serde(default)]
    intermediate_stops: Vec<RawStop>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStop {
    name: String,
    arrival: Option<Timestamp>,
    scheduled_arrival: Option<Timestamp>,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct RawPolyline {
    points: String,
    precision: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStep {
    relative_direction: String,
    #[serde(default)]
    street_name: String,
    distance: f64,
}

impl Response {
    fn into_plan(self) -> Plan {
        Plan {
            direct: self
                .direct
                .into_iter()
                .next()
                .map(RawItinerary::into_itinerary),
            transit: self
                .itineraries
                .into_iter()
                .map(RawItinerary::into_itinerary)
                .collect(),
            earlier: self.previous_page_cursor,
            later: self.next_page_cursor,
        }
    }
}

impl RawItinerary {
    fn into_itinerary(self) -> Itinerary {
        let legs: Vec<Leg> = self.legs.into_iter().map(RawLeg::into_leg).collect();
        let scheduled_start = legs
            .first()
            .map_or(self.start_time, |leg| leg.scheduled_start);
        let scheduled_end = legs.last().map_or(self.end_time, |leg| leg.scheduled_end);

        Itinerary {
            duration: self.duration,
            start_time: self.start_time,
            end_time: self.end_time,
            scheduled_start,
            scheduled_end,
            transfers: self.transfers,
            legs,
        }
        .without_implausible_estimates()
    }
}

impl RawLeg {
    fn into_leg(self) -> Leg {
        let line = is_transit_mode(&self.mode).then(|| Line {
            name: self.route_short_name,
            color: self.route_color.as_deref().and_then(hex_color),
            text_color: self.route_text_color.as_deref().and_then(hex_color),
            headsign: self.headsign,
            agency: self.agency_name,
        });

        Leg {
            mode: self.mode,
            from: self.from.into_stop(),
            to: self.to.into_stop(),
            start_time: self.start_time,
            end_time: self.end_time,
            scheduled_start: self.scheduled_start_time.unwrap_or(self.start_time),
            scheduled_end: self.scheduled_end_time.unwrap_or(self.end_time),
            duration: self.duration,
            distance: self.distance,
            line,
            stops: self
                .intermediate_stops
                .into_iter()
                .map(|stop| PassedStop {
                    name: stop.name,
                    arrival: stop.arrival,
                    scheduled_arrival: stop.scheduled_arrival,
                    position: LatLon::new(stop.lat, stop.lon).to_world(),
                })
                .collect(),
            path: self
                .leg_geometry
                .map(|geometry| {
                    decode_polyline(&geometry.points, geometry.precision)
                        .into_iter()
                        .map(LatLon::to_world)
                        .collect()
                })
                .unwrap_or_default(),
            steps: self
                .steps
                .into_iter()
                .map(|step| Step {
                    direction: step.relative_direction,
                    street: step.street_name,
                    distance: step.distance,
                })
                .collect(),
        }
    }
}

impl RawPlace {
    fn into_stop(self) -> Stop {
        Stop {
            name: self.name,
            position: LatLon::new(self.lat, self.lon).to_world(),
        }
    }
}

/// A colour written as `rrggbb`, as in GTFS feeds.
fn hex_color(hex: &str) -> Option<Color> {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    (hex.trim_start_matches('#').len() == 6)
        .then(|| Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

/// Decodes an encoded polyline (Google's format) into latitudes and longitudes.
/// `precision` is the number of decimals kept, 5 in Google's format, 6 in MOTIS.
fn decode_polyline(encoded: &str, precision: u32) -> Vec<LatLon> {
    let factor = 10f64.powi(precision as i32);
    let mut bytes = encoded.bytes();
    let mut next_delta = || -> Option<i64> {
        let mut result = 0i64;
        let mut shift = 0;
        loop {
            let chunk = i64::from(bytes.next()?) - 63;
            result |= (chunk & 0x1f) << shift;
            shift += 5;
            if chunk < 0x20 {
                break;
            }
        }
        // Zigzag encoding: the lowest bit is the sign.
        Some(if result & 1 == 1 {
            !(result >> 1)
        } else {
            result >> 1
        })
    };

    let mut points = Vec::new();
    let (mut lat, mut lon) = (0i64, 0i64);
    while let (Some(dlat), Some(dlon)) = (next_delta(), next_delta()) {
        lat += dlat;
        lon += dlon;
        points.push(LatLon::new(lat as f64 / factor, lon as f64 / factor));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_polylines() {
        // The example from Google's polyline format documentation.
        let points = decode_polyline("_p~iF~ps|U_ulLnnqC_mqNvxq`@", 5);
        let expected = [(38.5, -120.2), (40.7, -120.95), (43.252, -126.453)];

        assert_eq!(points.len(), expected.len());
        for (point, (expected_lat, expected_lon)) in points.into_iter().zip(expected) {
            assert!(
                (point.lat - expected_lat).abs() < 1e-9 && (point.lon - expected_lon).abs() < 1e-9
            );
        }
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            hex_color("ffbe00"),
            Some(Color::from_rgb8(0xff, 0xbe, 0x00))
        );
        assert_eq!(hex_color("#ffffff"), Some(Color::WHITE));
        assert_eq!(hex_color("fff"), None);
        assert_eq!(hex_color("not a colour"), None);
    }

    #[test]
    fn parses_walk_and_transit_routes() {
        // Trimmed response for Musée du Louvre → Hôtel de Ville, Paris; the
        // geometries are shortened to Google's example polyline, at precision 5.
        let json = r#"{
            "direct": [{
                "duration": 1159, "startTime": "2026-10-25T14:56:00Z",
                "endTime": "2026-10-25T15:15:00Z", "transfers": 0,
                "legs": [{
                    "mode": "WALK", "duration": 1159, "distance": 1358.0,
                    "startTime": "2026-10-25T14:56:00Z", "endTime": "2026-10-25T15:15:00Z",
                    "from": {"name": "START", "lat": 48.8606, "lon": 2.3376},
                    "to": {"name": "END", "lat": 48.8564, "lon": 2.3524},
                    "legGeometry": {"points": "_p~iF~ps|U_ulLnnqC_mqNvxq`@", "precision": 5, "length": 3},
                    "steps": [{"relativeDirection": "CONTINUE", "distance": 15.0, "streetName": ""}]
                }]
            }],
            "itineraries": [{
                "duration": 600, "startTime": "2026-10-25T14:57:00Z",
                "endTime": "2026-10-25T15:07:00Z", "transfers": 0,
                "legs": [{
                    "mode": "SUBWAY", "duration": 120,
                    "startTime": "2026-10-25T15:01:00Z", "endTime": "2026-10-25T15:03:00Z",
                    "from": {"name": "Louvre - Rivoli", "lat": 48.861073, "lon": 2.3401628},
                    "to": {"name": "Hôtel de Ville", "lat": 48.857105, "lon": 2.3529568},
                    "routeShortName": "1", "routeColor": "ffbe00", "routeTextColor": "000000",
                    "headsign": "Château de Vincennes", "agencyName": "RATP",
                    "intermediateStops": [{"name": "Châtelet", "lat": 48.858, "lon": 2.347,
                                           "arrival": "2026-10-25T15:02:00Z"}],
                    "legGeometry": {"points": "_p~iF~ps|U_ulLnnqC_mqNvxq`@", "precision": 5, "length": 3}
                }]
            }],
            "previousPageCursor": "EARLIER|1792940160",
            "nextPageCursor": "LATER|1792940820"
        }"#;

        let plan = serde_json::from_str::<Response>(json).unwrap().into_plan();

        assert!(!plan.is_empty(RouteMode::Transit));
        assert_eq!(plan.later.as_deref(), Some("LATER|1792940820"));

        let walk = plan.direct.unwrap();
        assert_eq!(walk.duration, 1159);
        assert_eq!(
            walk.start_time,
            "2026-10-25T14:56:00Z".parse::<Timestamp>().unwrap()
        );
        let leg = &walk.legs[0];
        assert!(leg.is_walk());
        assert_eq!(leg.distance, Some(1358.0));
        assert_eq!(leg.line, None);
        assert_eq!(leg.path.len(), 3);
        assert_eq!(leg.steps[0].direction, "CONTINUE");
        assert_eq!(leg.steps[0].distance, 15.0);

        let metro = &plan.transit[0].legs[0];
        assert!(!metro.is_walk() && metro.is_transit());
        assert_eq!(metro.from.name, "Louvre - Rivoli");
        let line = metro.line.as_ref().unwrap();
        assert_eq!(line.name.as_deref(), Some("1"));
        assert_eq!(line.color, Some(Color::from_rgb8(0xff, 0xbe, 0x00)));
        assert_eq!(line.headsign.as_deref(), Some("Château de Vincennes"));
        assert_eq!(metro.stops.len(), 1);
        assert_eq!(metro.stops[0].name, "Châtelet");
        assert_eq!(
            metro.stops[0].position,
            LatLon::new(48.858, 2.347).to_world()
        );
        assert_eq!(
            metro.stops[0].arrival,
            Some("2026-10-25T15:02:00Z".parse::<Timestamp>().unwrap())
        );
        assert_eq!(metro.steps, Vec::new());
    }

    /// A walk to Louvre - Rivoli and a Metro 1 ride to Hôtel de Ville, Paris, scheduled
    /// from 14:56 to 15:03 with the ride leaving at 15:01. The ride is estimated `late`
    /// seconds after its schedule (before it if negative).
    fn metro_trip(late: i64) -> Itinerary {
        let time = |seconds: i64| {
            let scheduled: Timestamp = "2026-10-25T15:01:00Z".parse().unwrap();
            Timestamp::from_second(scheduled.as_second() + seconds)
                .unwrap()
                .to_string()
        };
        let json = format!(
            r#"{{
            "duration": {duration}, "startTime": "2026-10-25T14:56:00Z", "endTime": "{end}",
            "transfers": 0,
            "legs": [
                {{
                    "mode": "WALK", "duration": 300, "distance": 380.0,
                    "startTime": "2026-10-25T14:56:00Z", "endTime": "{start}",
                    "scheduledStartTime": "2026-10-25T14:56:00Z",
                    "scheduledEndTime": "2026-10-25T15:01:00Z",
                    "from": {{"name": "START", "lat": 48.8606, "lon": 2.3376}},
                    "to": {{"name": "Louvre - Rivoli", "lat": 48.861073, "lon": 2.3401628}}
                }},
                {{
                    "mode": "SUBWAY", "duration": 120, "realTime": true,
                    "startTime": "{start}", "endTime": "{end}",
                    "scheduledStartTime": "2026-10-25T15:01:00Z",
                    "scheduledEndTime": "2026-10-25T15:03:00Z",
                    "from": {{"name": "Louvre - Rivoli", "lat": 48.861073, "lon": 2.3401628}},
                    "to": {{"name": "Hôtel de Ville", "lat": 48.857105, "lon": 2.3529568}},
                    "routeShortName": "1",
                    "intermediateStops": [{{
                        "name": "Châtelet", "lat": 48.858, "lon": 2.347,
                        "arrival": "{stop}", "scheduledArrival": "2026-10-25T15:02:00Z"
                    }}]
                }}
            ]
        }}"#,
            duration = (420 + late).max(0),
            start = time(late),
            end = time(late + 120),
            stop = time(late + 60),
        );
        serde_json::from_str::<RawItinerary>(&json)
            .unwrap()
            .into_itinerary()
    }

    fn parse(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    #[test]
    fn real_time_estimates_are_kept_beside_the_schedule() {
        // Four minutes late.
        let trip = metro_trip(240);

        assert_eq!(trip.start().estimated, parse("2026-10-25T14:56:00Z"));
        assert_eq!(trip.start().scheduled, parse("2026-10-25T14:56:00Z"));
        assert_eq!(trip.end().estimated, parse("2026-10-25T15:07:00Z"));
        assert_eq!(trip.end().scheduled, parse("2026-10-25T15:03:00Z"));
        // Seven minutes scheduled, four more estimated.
        assert_eq!(trip.durations(), (660, 420));

        let metro = &trip.legs[1];
        assert_eq!(metro.start().estimated, parse("2026-10-25T15:05:00Z"));
        assert_eq!(metro.start().scheduled, parse("2026-10-25T15:01:00Z"));
        assert_eq!(metro.stops[0].arrival, Some(parse("2026-10-25T15:06:00Z")));
        assert_eq!(
            metro.stops[0].scheduled_arrival,
            Some(parse("2026-10-25T15:02:00Z"))
        );
    }

    #[test]
    fn the_schedule_alone_when_there_is_no_real_time_data() {
        // Older fixtures have no scheduled times: they default to the estimates.
        let plan: Plan = serde_json::from_str::<Response>(
            r#"{"direct": [{"duration": 60, "startTime": "2026-10-25T15:01:00Z",
                "endTime": "2026-10-25T15:02:00Z", "legs": [{
                "mode": "WALK", "duration": 60, "startTime": "2026-10-25T15:01:00Z",
                "endTime": "2026-10-25T15:02:00Z",
                "from": {"name": "START", "lat": 48.8606, "lon": 2.3376},
                "to": {"name": "END", "lat": 48.8564, "lon": 2.3524}}]}]}"#,
        )
        .unwrap()
        .into_plan();

        let walk = plan.direct.unwrap();
        assert_eq!(walk.start().estimated, walk.start().scheduled);
        assert_eq!(walk.durations(), (60, 60));
    }

    #[test]
    fn early_estimates_are_kept_within_five_minutes_and_late_ones_always() {
        // Three minutes early.
        assert_eq!(
            metro_trip(-180).legs[1].start().estimated,
            parse("2026-10-25T14:58:00Z")
        );
        // Half an hour late.
        let late = metro_trip(1800);
        assert_eq!(
            late.legs[1].start().estimated,
            parse("2026-10-25T15:31:00Z")
        );
        assert_eq!(late.durations(), (2220, 420));
    }

    #[test]
    fn implausibly_early_estimates_fall_back_to_the_schedule() {
        // An hour early, as one Metro 1 trip was from Transitous.
        let trip = metro_trip(-3660);

        assert_eq!(trip.start_time, parse("2026-10-25T14:56:00Z"));
        assert_eq!(trip.end_time, parse("2026-10-25T15:03:00Z"));
        assert_eq!(trip.durations(), (420, 420));
        for leg in &trip.legs {
            assert_eq!(leg.start_time, leg.scheduled_start);
            assert_eq!(leg.end_time, leg.scheduled_end);
        }
        assert_eq!(
            trip.legs[1].stops[0].arrival,
            Some(parse("2026-10-25T15:02:00Z"))
        );
    }

    #[test]
    fn missing_routes_give_an_empty_plan() {
        let plan = serde_json::from_str::<Response>(r#"{"direct": [], "itineraries": []}"#)
            .unwrap()
            .into_plan();

        assert_eq!(plan, Plan::default());
        assert!(plan.is_empty(RouteMode::Walk) && plan.is_empty(RouteMode::Transit));
    }

    #[test]
    fn parses_bike_routes() {
        // Trimmed response for Musée du Louvre → Gare de Lyon, Paris, by bike.
        let json = r#"{
            "direct": [{
                "duration": 936, "startTime": "2026-10-25T16:00:00Z",
                "endTime": "2026-10-25T16:15:36Z",
                "legs": [{
                    "mode": "BIKE", "duration": 936, "distance": 3724.0,
                    "startTime": "2026-10-25T16:00:00Z", "endTime": "2026-10-25T16:15:36Z",
                    "from": {"name": "START", "lat": 48.8606, "lon": 2.3376},
                    "to": {"name": "END", "lat": 48.8443, "lon": 2.3744},
                    "legGeometry": {"points": "_p~iF~ps|U_ulLnnqC_mqNvxq`@", "precision": 5, "length": 3},
                    "steps": [{"relativeDirection": "CONTINUE", "distance": 14.0, "streetName": "Rue de Rivoli"}]
                }]
            }],
            "itineraries": []
        }"#;

        let plan = serde_json::from_str::<Response>(json).unwrap().into_plan();

        assert!(!plan.is_empty(RouteMode::Bike));
        let leg = &plan.direct.unwrap().legs[0];
        assert!(!leg.is_walk() && !leg.is_transit());
        assert_eq!(leg.line, None);
        assert_eq!(leg.distance, Some(3724.0));
        assert_eq!(leg.steps[0].street, "Rue de Rivoli");
    }

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
