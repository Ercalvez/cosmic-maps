// SPDX-License-Identifier: MPL-2.0

//! Routes between two places, with the times they are scheduled and estimated
//! for.

use cosmic::iced::Color;
use jiff::Timestamp;

use crate::geo::WorldPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Walk,
    Bike,
    Car,
    /// Public transit, with walking to and between stops.
    Transit,
}

/// The routes found between two places.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The route all the way by street, in the requested mode (walking for
    /// transit), if possible.
    pub direct: Option<Itinerary>,
    /// Public transit options, empty for other modes.
    pub transit: Vec<Itinerary>,
    /// Cursors for transit options leaving earlier or later, passed back to [`plan`].
    pub earlier: Option<String>,
    pub later: Option<String>,
}

impl Plan {
    /// Whether nothing was found for `mode`.
    pub fn is_empty(&self, mode: RouteMode) -> bool {
        match mode {
            RouteMode::Walk | RouteMode::Bike | RouteMode::Car => self.direct.is_none(),
            RouteMode::Transit => self.direct.is_none() && self.transit.is_empty(),
        }
    }
}

/// Real-time estimates earlier than the schedule by more than this, in seconds,
/// are taken for errors in the feed: vehicles do not run an hour early.
const MAX_EARLY: i64 = 300;

/// A time as estimated with real-time data, and as scheduled. Both are the same
/// without real-time data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planned {
    pub estimated: Timestamp,
    pub scheduled: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Itinerary {
    /// Estimated, in seconds.
    pub duration: u64,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    /// Of the first and last legs, which MOTIS gives no itinerary times for.
    pub scheduled_start: Timestamp,
    pub scheduled_end: Timestamp,
    pub transfers: u32,
    pub legs: Vec<Leg>,
}

impl Itinerary {
    pub fn start(&self) -> Planned {
        Planned {
            estimated: self.start_time,
            scheduled: self.scheduled_start,
        }
    }

    pub fn end(&self) -> Planned {
        Planned {
            estimated: self.end_time,
            scheduled: self.scheduled_end,
        }
    }

    /// The duration as estimated and as scheduled, in seconds.
    pub fn durations(&self) -> (u64, u64) {
        let scheduled = self.scheduled_end.as_second() - self.scheduled_start.as_second();
        (self.duration, u64::try_from(scheduled).unwrap_or_default())
    }
}

/// How a leg is travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegMode {
    Walk,
    Bike,
    Car,
    Transit(TransitKind),
}

/// The kind of vehicle on a public transit line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitKind {
    Metro,
    Bus,
    Tram,
    Ferry,
    Train,
    /// Cable cars, funiculars, and whatever else feeds describe.
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    pub mode: LegMode,
    pub from: Stop,
    pub to: Stop,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub scheduled_start: Timestamp,
    pub scheduled_end: Timestamp,
    /// In seconds.
    pub duration: u64,
    /// In meters, for street legs.
    pub distance: Option<f64>,
    /// The line taken, for transit legs.
    pub line: Option<Line>,
    /// Stops passed between boarding and getting off, for transit legs.
    pub stops: Vec<PassedStop>,
    /// The leg's path, in Web Mercator coordinates spanning 0..1.
    pub path: Vec<WorldPoint>,
    /// Turn-by-turn instructions, for street legs.
    pub steps: Vec<Step>,
}

impl Leg {
    pub fn is_walk(&self) -> bool {
        self.mode == LegMode::Walk
    }

    pub fn start(&self) -> Planned {
        Planned {
            estimated: self.start_time,
            scheduled: self.scheduled_start,
        }
    }

    pub fn end(&self) -> Planned {
        Planned {
            estimated: self.end_time,
            scheduled: self.scheduled_end,
        }
    }

    /// On a public transit line, rather than walking, cycling or driving.
    pub fn is_transit(&self) -> bool {
        matches!(self.mode, LegMode::Transit(_))
    }
}

/// What a stop is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopName {
    /// Where the trip starts, which has no name of its own.
    Start,
    /// Where the trip ends.
    End,
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub name: StopName,
    pub position: WorldPoint,
}

/// A stop the vehicle calls at between boarding and getting off.
#[derive(Debug, Clone, PartialEq)]
pub struct PassedStop {
    pub name: String,
    pub arrival: Option<Timestamp>,
    pub scheduled_arrival: Option<Timestamp>,
    pub position: WorldPoint,
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

/// What to do at the start of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Depart,
    Continue,
    Left,
    Right,
    SlightlyLeft,
    SlightlyRight,
    HardLeft,
    HardRight,
    Roundabout,
    UTurn,
    Elevator,
    Stairs,
    EnterStation,
    ExitStation,
    FollowSigns,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub direction: Direction,
    /// Empty when the way has no name.
    pub street: String,
    /// In meters.
    pub distance: f64,
}

impl Itinerary {
    /// The itinerary with its times on schedule if some leg is estimated to run much
    /// earlier than scheduled, which feeds get wrong.
    pub fn without_implausible_estimates(mut self) -> Self {
        if self.has_implausible_estimates() {
            self.use_schedule();
        }
        self
    }

    /// Some leg is estimated to run much earlier than scheduled.
    fn has_implausible_estimates(&self) -> bool {
        let too_early = |planned: Planned| {
            planned.scheduled.as_second() - planned.estimated.as_second() > MAX_EARLY
        };
        self.legs
            .iter()
            .any(|leg| too_early(leg.start()) || too_early(leg.end()))
    }

    /// Drops all the real-time estimates, so the times shown stay consistent.
    fn use_schedule(&mut self) {
        self.start_time = self.scheduled_start;
        self.end_time = self.scheduled_end;
        self.duration = self.durations().1;
        for leg in &mut self.legs {
            leg.start_time = leg.scheduled_start;
            leg.end_time = leg.scheduled_end;
            for stop in &mut leg.stops {
                stop.arrival = stop.scheduled_arrival.or(stop.arrival);
            }
        }
    }
}
