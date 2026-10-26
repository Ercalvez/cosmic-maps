// SPDX-License-Identifier: MPL-2.0

//! Directions results drawn on the map.

use cosmic::iced::widget::canvas::{self, Frame, Path};
use cosmic::iced::{Color, Size};

use crate::map::camera::Camera;
use crate::map::style::MapPalette;
use crate::services::transitous::{Itinerary, Leg};

/// Width of the chosen route, of its outline on each side, and of the others.
const ROUTE_WIDTH: f32 = 6.0;
const ROUTE_OUTLINE: f32 = 2.0;
const FAINT_ROUTE_WIDTH: f32 = 4.0;
/// Space kept around a route when framing it.
pub(super) const ROUTE_PADDING: f32 = 48.0;
/// Walking parts as dots: zero-length dashes with round caps, this far apart.
const WALK_DOTS: [f32; 2] = [0.0, 11.0];
const ROUTE_STOP_RADIUS: f32 = 3.0;
const ROUTE_BOARD_RADIUS: f32 = 5.0;
const ROUTE_END_RADIUS: f32 = 7.0;

/// Routes drawn on the map: the options found, one of them chosen.
#[derive(Debug, Clone)]
pub struct RouteOverlay {
    /// In the order the directions list them.
    pub itineraries: Vec<Itinerary>,
    pub chosen: usize,
}

/// Every point of a route: its paths and its stops.
pub(super) fn route_points(itinerary: &Itinerary) -> impl Iterator<Item = (f64, f64)> + '_ {
    itinerary.legs.iter().flat_map(|leg| {
        leg.path
            .iter()
            .copied()
            .chain([leg.from.position, leg.to.position])
            .chain(leg.stops.iter().map(|stop| stop.position))
    })
}

/// The smallest box holding all `points`, or `None` if there are none.
pub(super) fn bounds(points: impl Iterator<Item = (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
    points.fold(None, |bounds, (x, y)| {
        let ((min_x, min_y), (max_x, max_y)) = bounds.unwrap_or(((x, y), (x, y)));
        Some(((min_x.min(x), min_y.min(y)), (max_x.max(x), max_y.max(y))))
    })
}

/// A leg's path on screen, or a straight line between its ends without one.
fn leg_path(leg: &Leg, camera: &Camera, viewport: Size) -> Path {
    let fallback = [leg.from.position, leg.to.position];
    let points = if leg.path.is_empty() {
        &fallback[..]
    } else {
        &leg.path[..]
    };

    Path::new(|builder| {
        let mut points = points
            .iter()
            .map(|point| camera.to_screen(*point, viewport));
        if let Some(first) = points.next() {
            builder.move_to(first);
            for point in points {
                builder.line_to(point);
            }
        }
    })
}

fn round_stroke<'a>(color: Color, width: f32) -> canvas::Stroke<'a> {
    canvas::Stroke::default()
        .with_color(color)
        .with_width(width)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round)
}

/// The routes found: the others faint, then the chosen one on top, its walking
/// parts dotted, cycling and driving solid, and its transit legs in their lines'
/// colours.
pub(super) fn draw_routes(
    frame: &mut Frame,
    camera: &Camera,
    routes: &RouteOverlay,
    viewport: Size,
    palette: &MapPalette,
    accent: Color,
) {
    let faint = Color {
        a: 0.45,
        ..palette.poi_marker
    };
    for (index, itinerary) in routes.itineraries.iter().enumerate() {
        if index != routes.chosen {
            for leg in &itinerary.legs {
                frame.stroke(
                    &leg_path(leg, camera, viewport),
                    round_stroke(faint, FAINT_ROUTE_WIDTH),
                );
            }
        }
    }

    let Some(chosen) = routes.itineraries.get(routes.chosen) else {
        return;
    };
    for leg in &chosen.legs {
        let path = leg_path(leg, camera, viewport);
        let dots = canvas::LineDash {
            segments: &WALK_DOTS,
            offset: 0,
        };
        // An outline in the land colour keeps light line colours visible.
        let outline = round_stroke(palette.land, ROUTE_WIDTH + 2.0 * ROUTE_OUTLINE);
        if leg.is_walk() {
            frame.stroke(
                &path,
                canvas::Stroke {
                    line_dash: dots,
                    ..outline
                },
            );
            frame.stroke(
                &path,
                canvas::Stroke {
                    line_dash: dots,
                    ..round_stroke(accent, ROUTE_WIDTH)
                },
            );
        } else {
            let color = leg
                .line
                .as_ref()
                .and_then(|line| line.color)
                .unwrap_or(accent);
            frame.stroke(&path, outline);
            frame.stroke(&path, round_stroke(color, ROUTE_WIDTH));
        }
    }
}

/// Stops of the chosen route: those passed, where to board and get off, and the
/// trip's start (hollow) and destination (filled).
pub(super) fn draw_route_stops(
    frame: &mut Frame,
    camera: &Camera,
    itinerary: &Itinerary,
    viewport: Size,
    palette: &MapPalette,
    accent: Color,
) {
    let dot = |frame: &mut Frame, position: (f64, f64), radius: f32, fill: Color, ring: Color| {
        let circle = Path::circle(camera.to_screen(position, viewport), radius);
        frame.fill(&circle, fill);
        frame.stroke(
            &circle,
            canvas::Stroke::default().with_color(ring).with_width(2.0),
        );
    };

    for leg in itinerary.legs.iter().filter(|leg| leg.is_transit()) {
        let color = leg
            .line
            .as_ref()
            .and_then(|line| line.color)
            .unwrap_or(accent);
        for stop in &leg.stops {
            dot(frame, stop.position, ROUTE_STOP_RADIUS, palette.land, color);
        }
        for end in [leg.from.position, leg.to.position] {
            dot(frame, end, ROUTE_BOARD_RADIUS, Color::WHITE, color);
        }
    }

    if let (Some(first), Some(last)) = (itinerary.legs.first(), itinerary.legs.last()) {
        dot(
            frame,
            first.from.position,
            ROUTE_END_RADIUS,
            palette.land,
            accent,
        );
        dot(
            frame,
            last.to.position,
            ROUTE_END_RADIUS,
            accent,
            Color::WHITE,
        );
    }
}
