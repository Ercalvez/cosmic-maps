// SPDX-License-Identifier: MPL-2.0

//! The parts of MOTIS responses that are used, and their conversion to routes.

use cosmic::iced::Color;
use jiff::Timestamp;
use serde::Deserialize;

use super::polyline::decode_polyline;
use crate::domain::route::{
    Direction, Itinerary, Leg, LegMode, Line, PassedStop, Plan, Step, Stop, StopName, TransitKind,
};
use crate::geo::LatLon;

/// The parts of a MOTIS `/api/v6/plan` response that are used.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Response {
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
    mode: RawMode,
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
    relative_direction: RawDirection,
    #[serde(default)]
    street_name: String,
    distance: f64,
}

/// A MOTIS leg mode. Ones this does not know are taken for public transit, which
/// is what most of them are.
#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RawMode {
    Walk,
    Bike,
    Car,
    Subway,
    Metro,
    Bus,
    Coach,
    Tram,
    Ferry,
    Rail,
    HighspeedRail,
    LongDistance,
    NightRail,
    RegionalFastRail,
    RegionalRail,
    Suburban,
    #[serde(other)]
    Other,
}

impl From<RawMode> for LegMode {
    fn from(mode: RawMode) -> Self {
        match mode {
            RawMode::Walk => Self::Walk,
            RawMode::Bike => Self::Bike,
            RawMode::Car => Self::Car,
            RawMode::Subway | RawMode::Metro => Self::Transit(TransitKind::Metro),
            RawMode::Bus | RawMode::Coach => Self::Transit(TransitKind::Bus),
            RawMode::Tram => Self::Transit(TransitKind::Tram),
            RawMode::Ferry => Self::Transit(TransitKind::Ferry),
            RawMode::Rail
            | RawMode::HighspeedRail
            | RawMode::LongDistance
            | RawMode::NightRail
            | RawMode::RegionalFastRail
            | RawMode::RegionalRail
            | RawMode::Suburban => Self::Transit(TransitKind::Train),
            RawMode::Other => Self::Transit(TransitKind::Other),
        }
    }
}

/// A MOTIS relative direction, with the spellings it also uses. Ones this does not
/// know read as "continue".
#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RawDirection {
    Depart,
    Continue,
    Left,
    Right,
    SlightlyLeft,
    #[serde(alias = "SLIGHT_RIGHT")]
    SlightlyRight,
    HardLeft,
    HardRight,
    CircleClockwise,
    CircleCounterclockwise,
    #[serde(alias = "UTRUN_LEFT")]
    UturnLeft,
    UturnRight,
    Elevator,
    Stairs,
    EnterStation,
    ExitStation,
    FollowSigns,
    #[serde(other)]
    Other,
}

impl From<RawDirection> for Direction {
    fn from(direction: RawDirection) -> Self {
        match direction {
            RawDirection::Depart => Self::Depart,
            RawDirection::Continue | RawDirection::Other => Self::Continue,
            RawDirection::Left => Self::Left,
            RawDirection::Right => Self::Right,
            RawDirection::SlightlyLeft => Self::SlightlyLeft,
            RawDirection::SlightlyRight => Self::SlightlyRight,
            RawDirection::HardLeft => Self::HardLeft,
            RawDirection::HardRight => Self::HardRight,
            RawDirection::CircleClockwise | RawDirection::CircleCounterclockwise => {
                Self::Roundabout
            }
            RawDirection::UturnLeft | RawDirection::UturnRight => Self::UTurn,
            RawDirection::Elevator => Self::Elevator,
            RawDirection::Stairs => Self::Stairs,
            RawDirection::EnterStation => Self::EnterStation,
            RawDirection::ExitStation => Self::ExitStation,
            RawDirection::FollowSigns => Self::FollowSigns,
        }
    }
}

impl Response {
    pub(super) fn into_plan(self) -> Plan {
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
        let mode = LegMode::from(self.mode);
        let line = matches!(mode, LegMode::Transit(_)).then(|| Line {
            name: self.route_short_name,
            color: self.route_color.as_deref().and_then(hex_color),
            text_color: self.route_text_color.as_deref().and_then(hex_color),
            headsign: self.headsign,
            agency: self.agency_name,
        });

        Leg {
            mode,
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
                    direction: step.relative_direction.into(),
                    street: step.street_name,
                    distance: step.distance,
                })
                .collect(),
        }
    }
}

impl RawPlace {
    fn into_stop(self) -> Stop {
        // MOTIS names the trip's ends `START` and `END`.
        let name = match self.name.as_str() {
            "START" => StopName::Start,
            "END" => StopName::End,
            _ => StopName::Named(self.name),
        };
        Stop {
            name,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::route::RouteMode;

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
        assert_eq!(leg.steps[0].direction, Direction::Continue);
        assert_eq!(leg.mode, LegMode::Walk);
        assert_eq!(walk.legs[0].from.name, StopName::Start);
        assert_eq!(walk.legs[0].to.name, StopName::End);
        assert_eq!(leg.steps[0].distance, 15.0);

        let metro = &plan.transit[0].legs[0];
        assert!(!metro.is_walk() && metro.is_transit());
        assert_eq!(
            metro.from.name,
            StopName::Named("Louvre - Rivoli".to_owned())
        );
        assert_eq!(metro.mode, LegMode::Transit(TransitKind::Metro));
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
    fn modes_and_directions_are_parsed_once() {
        let mode = |name: &str| LegMode::from(serde_json::from_str::<RawMode>(name).unwrap());
        assert_eq!(mode(r#""WALK""#), LegMode::Walk);
        assert_eq!(mode(r#""SUBWAY""#), LegMode::Transit(TransitKind::Metro));
        assert_eq!(
            mode(r#""REGIONAL_RAIL""#),
            LegMode::Transit(TransitKind::Train)
        );
        // Modes feeds add later are public transit of an unusual kind.
        assert_eq!(mode(r#""CABLE_CAR""#), LegMode::Transit(TransitKind::Other));

        let direction =
            |name: &str| Direction::from(serde_json::from_str::<RawDirection>(name).unwrap());
        assert_eq!(direction(r#""LEFT""#), Direction::Left);
        // MOTIS spells some of them in more than one way.
        assert_eq!(direction(r#""SLIGHT_RIGHT""#), Direction::SlightlyRight);
        assert_eq!(direction(r#""SLIGHTLY_RIGHT""#), Direction::SlightlyRight);
        assert_eq!(direction(r#""UTRUN_LEFT""#), Direction::UTurn);
        assert_eq!(direction(r#""CIRCLE_CLOCKWISE""#), Direction::Roundabout);
        // Unknown ones read as "continue".
        assert_eq!(direction(r#""SOMETHING_NEW""#), Direction::Continue);
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
}
