//! Walking and public transit routes from Transitous, the community-run MOTIS
//! service GNOME Maps also uses for public transit.
//!
//! Usage policy (transitous.org/api): open-source, non-commercial apps sending a
//! `User-Agent` with a contact, with visible attribution of transitous.org/sources
//! and OpenStreetMap. Maintainers ask to be contacted before heavy `/plan` use.


use std::time::Duration;

use cosmic::iced::Color;
use jiff::Timestamp;
use serde::Deserialize;

use super::http::{CLIENT, Error, RateLimit};
use crate::map::camera::world_position;


const PLAN_URL: &str = "https://api.transitous.org/api/v6/plan";
/// Transitous gives no numbers; plans are only requested on user actions, so one
/// per second is plenty.
static TRANSITOUS: RateLimit = RateLimit::new(Duration::from_secs(1));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Walk,
    Transit,
}

/// The routes found between two places.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Walking all the way, if possible.
    pub walk: Option<Itinerary>,
    /// Public transit options, empty for walking requests.
    pub transit: Vec<Itinerary>,
    /// Cursors for transit options leaving earlier or later, passed back to [`plan`].
    pub earlier: Option<String>,
    pub later: Option<String>,
}

impl Plan {
    /// Whether nothing was found for `mode`.
    pub fn is_empty(&self, mode: RouteMode) -> bool {
        match mode {
            RouteMode::Walk => self.walk.is_none(),
            RouteMode::Transit => self.walk.is_none() && self.transit.is_empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Itinerary {
    /// In seconds.
    pub duration: u64,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub transfers: u32,
    pub legs: Vec<Leg>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    /// MOTIS mode, such as `WALK`, `BUS`, `TRAM` or `REGIONAL_RAIL`.
    pub mode: String,
    pub from: Stop,
    pub to: Stop,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    /// In seconds.
    pub duration: u64,
    /// In meters, for walking legs.
    pub distance: Option<f64>,
    /// The line taken, for transit legs.
    pub line: Option<Line>,
    /// Stops passed between boarding and getting off, for transit legs.
    pub stops: Vec<PassedStop>,
    /// The leg's path, in Web Mercator coordinates spanning 0..1.
    pub path: Vec<(f64, f64)>,
    /// Turn-by-turn instructions, for walking legs.
    pub steps: Vec<Step>,
}

impl Leg {
    pub fn is_walk(&self) -> bool {
        self.mode == "WALK"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    /// Stop name; MOTIS names the trip's ends `START` and `END`.
    pub name: String,
    /// Web Mercator coordinates spanning 0..1.
    pub position: (f64, f64),
}

/// A stop the vehicle calls at between boarding and getting off.
#[derive(Debug, Clone, PartialEq)]
pub struct PassedStop {
    pub name: String,
    pub arrival: Option<Timestamp>,
    /// Web Mercator coordinates spanning 0..1.
    pub position: (f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Such as "3" or "A".
    pub name: Option<String>,
    /// The line's colours, when the operator publishes them.
    pub color: Option<Color>,
    pub text_color: Option<Color>,
    /// Where the vehicle is heading.
    pub headsign: Option<String>,
    pub agency: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// MOTIS direction, such as `CONTINUE`, `LEFT` or `SLIGHTLY_RIGHT`.
    pub direction: String,
    /// Empty when the way has no name.
    pub street: String,
    /// In meters.
    pub distance: f64,
}

/// Routes from `from` to `to`, both latitude and longitude, leaving now, or the
/// earlier or later page given by a cursor from a previous [`Plan`].
pub async fn plan(
    from: (f64, f64),
    to: (f64, f64),
    mode: RouteMode,
    page: Option<String>,
) -> Result<Plan, Error> {
    TRANSITOUS.wait_turn().await;

    let from = format!("{},{}", from.0, from.1);
    let to = format!("{},{}", to.0, to.1);
    // No transit modes: only the direct walking route is computed.
    let transit_modes = match mode {
        RouteMode::Walk => "",
        RouteMode::Transit => "TRANSIT",
    };
    let mut query = vec![
        ("fromPlace", from.as_str()),
        ("toPlace", to.as_str()),
        ("directModes", "WALK"),
        ("transitModes", transit_modes),
    ];
    if let Some(page) = &page {
        query.push(("pageCursor", page.as_str()));
    }
    let response: Response = CLIENT
        .get(PLAN_URL)
        .query(&query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response.into_plan())
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
struct RawStop {
    name: String,
    arrival: Option<Timestamp>,
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
            walk: self.direct.into_iter().next().map(RawItinerary::into_itinerary),
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
        Itinerary {
            duration: self.duration,
            start_time: self.start_time,
            end_time: self.end_time,
            transfers: self.transfers,
            legs: self.legs.into_iter().map(RawLeg::into_leg).collect(),
        }
    }
}

impl RawLeg {
    fn into_leg(self) -> Leg {
        let is_transit = self.mode != "WALK";
        let line = is_transit.then(|| Line {
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
            duration: self.duration,
            distance: self.distance,
            line,
            stops: self
                .intermediate_stops
                .into_iter()
                .map(|stop| PassedStop {
                    name: stop.name,
                    arrival: stop.arrival,
                    position: world_position(stop.lat, stop.lon),
                })
                .collect(),
            path: self
                .leg_geometry
                .map(|geometry| {
                    decode_polyline(&geometry.points, geometry.precision)
                        .into_iter()
                        .map(|(lat, lon)| world_position(lat, lon))
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
            position: world_position(self.lat, self.lon),
        }
    }
}

/// A colour written as `rrggbb`, as in GTFS feeds.
fn hex_color(hex: &str) -> Option<Color> {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    (hex.trim_start_matches('#').len() == 6).then(|| {
        Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
    })
}

/// Decodes an encoded polyline (Google's format) into latitudes and longitudes.
/// `precision` is the number of decimals kept, 5 in Google's format, 6 in MOTIS.
fn decode_polyline(encoded: &str, precision: u32) -> Vec<(f64, f64)> {
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
        Some(if result & 1 == 1 { !(result >> 1) } else { result >> 1 })
    };

    let mut points = Vec::new();
    let (mut lat, mut lon) = (0i64, 0i64);
    while let (Some(dlat), Some(dlon)) = (next_delta(), next_delta()) {
        lat += dlat;
        lon += dlon;
        points.push((lat as f64 / factor, lon as f64 / factor));
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
        for ((lat, lon), (expected_lat, expected_lon)) in points.into_iter().zip(expected) {
            assert!((lat - expected_lat).abs() < 1e-9 && (lon - expected_lon).abs() < 1e-9);
        }
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(hex_color("ffbe00"), Some(Color::from_rgb8(0xff, 0xbe, 0x00)));
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

        let walk = plan.walk.unwrap();
        assert_eq!(walk.duration, 1159);
        assert_eq!(walk.start_time, "2026-10-25T14:56:00Z".parse::<Timestamp>().unwrap());
        let leg = &walk.legs[0];
        assert!(leg.is_walk());
        assert_eq!(leg.distance, Some(1358.0));
        assert_eq!(leg.line, None);
        assert_eq!(leg.path.len(), 3);
        assert_eq!(leg.steps[0].direction, "CONTINUE");
        assert_eq!(leg.steps[0].distance, 15.0);

        let metro = &plan.transit[0].legs[0];
        assert!(!metro.is_walk());
        assert_eq!(metro.from.name, "Louvre - Rivoli");
        let line = metro.line.as_ref().unwrap();
        assert_eq!(line.name.as_deref(), Some("1"));
        assert_eq!(line.color, Some(Color::from_rgb8(0xff, 0xbe, 0x00)));
        assert_eq!(line.headsign.as_deref(), Some("Château de Vincennes"));
        assert_eq!(metro.stops.len(), 1);
        assert_eq!(metro.stops[0].name, "Châtelet");
        assert_eq!(metro.stops[0].position, world_position(48.858, 2.347));
        assert_eq!(
            metro.stops[0].arrival,
            Some("2026-10-25T15:02:00Z".parse::<Timestamp>().unwrap())
        );
        assert!(metro.steps.is_empty());
    }

    #[test]
    fn missing_routes_give_an_empty_plan() {
        let plan = serde_json::from_str::<Response>(r#"{"direct": [], "itineraries": []}"#)
            .unwrap()
            .into_plan();

        assert_eq!(plan, Plan::default());
        assert!(plan.is_empty(RouteMode::Walk) && plan.is_empty(RouteMode::Transit));
    }
}
