// SPDX-License-Identifier: MPL-2.0

//! What is drawn above the tiles: the selection, POI markers, routes and the
//! user's position.

use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Color, Point, Rectangle, Size, mouse};

use super::MapView;
use super::routes::{draw_route_stops, draw_routes};
use crate::domain::place::{Poi, Shape};
use crate::geo::Position;
use crate::map::camera::Camera;
use crate::map::style::MapPalette;

const POI_MARKER_RADIUS: f32 = 3.5;
const MARKER_RADIUS: f32 = 6.0;
const USER_DOT_RADIUS: f32 = 7.0;
/// "You are here" blue, as in GNOME Maps.
const USER_COLOR: Color = Color::from_rgb(0.21, 0.52, 0.89);

/// POI markers and the selected POI, drawn above the tiles. It does not handle
/// events, so they reach `MapCanvas` below.
pub(super) struct MarkerCanvas<'a> {
    pub map: &'a MapView,
}

impl<Message> canvas::Program<Message, cosmic::Theme, cosmic::Renderer> for MarkerCanvas<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::Renderer,
        theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<cosmic::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        let accent = Color::from(theme.cosmic().accent_color());
        let (map, size) = (self.map, bounds.size());
        let palette = map.tiles.palette();

        // The selected place's extent, under everything else.
        if let Some(shape) = map.selection.shape() {
            draw_shape(&mut frame, &map.camera, shape, size, accent);
        }

        if let Some(routes) = &map.routes {
            draw_routes(&mut frame, &map.camera, routes, size, &palette, accent);
        }

        // The area the user is in, under the POI markers.
        let user = map.user.map(|user| user_area(&map.camera, user, size));
        if let Some((center, radius)) = user {
            draw_user_area(&mut frame, center, radius, size);
        }

        draw_pois(
            &mut frame,
            &map.tiles.drawn_pois(&map.camera, size),
            &palette,
        );

        // Route stops above the POI markers, so they stay visible.
        if let Some(itinerary) = map
            .routes
            .as_ref()
            .and_then(|routes| routes.itineraries.get(routes.chosen))
        {
            draw_route_stops(&mut frame, &map.camera, itinerary, size, &palette, accent);
        }

        if let Some((center, _)) = user {
            draw_user_dot(&mut frame, center);
        }

        if let Some(position) = map.selected_position() {
            draw_selection_marker(&mut frame, position, accent);
        }

        vec![frame.into_geometry()]
    }
}

/// Where the user is on screen, and the radius of the area they are in, in pixels.
fn user_area(camera: &Camera, user: Position, viewport: Size) -> (Point, f32) {
    let center = camera.to_screen(user.lat_lon.to_world(), viewport);
    let radius = (user.accuracy / camera.meters_per_pixel(user.lat_lon.lat)) as f32;
    (center, radius)
}

/// The area the user is in, unless it is hidden by the dot or covers the whole view.
fn draw_user_area(frame: &mut Frame, center: Point, radius: f32, viewport: Size) {
    let diagonal = viewport.width.hypot(viewport.height);
    if radius <= USER_DOT_RADIUS || radius >= 4.0 * diagonal {
        return;
    }

    let area = Path::circle(center, radius);
    frame.fill(
        &area,
        Color {
            a: 0.15,
            ..USER_COLOR
        },
    );
    frame.stroke(
        &area,
        canvas::Stroke::default()
            .with_color(Color {
                a: 0.5,
                ..USER_COLOR
            })
            .with_width(1.0),
    );
}

/// The POI markers: one path for all of them, as there can be hundreds.
fn draw_pois(frame: &mut Frame, pois: &[(&Poi, Point)], palette: &MapPalette) {
    if pois.is_empty() {
        return;
    }

    let markers = Path::new(|builder| {
        for (_, position) in pois {
            builder.circle(*position, POI_MARKER_RADIUS);
        }
    });
    frame.fill(&markers, palette.poi_marker);
    frame.stroke(
        &markers,
        canvas::Stroke::default()
            .with_color(palette.poi_outline)
            .with_width(1.5),
    );
}

fn draw_user_dot(frame: &mut Frame, center: Point) {
    let dot = Path::circle(center, USER_DOT_RADIUS);
    frame.fill(&dot, USER_COLOR);
    frame.stroke(
        &dot,
        canvas::Stroke::default()
            .with_color(Color::WHITE)
            .with_width(2.0),
    );
}

fn draw_selection_marker(frame: &mut Frame, position: Point, accent: Color) {
    let marker = Path::circle(position, MARKER_RADIUS);
    frame.fill(&marker, accent);
    frame.stroke(
        &marker,
        canvas::Stroke::default()
            .with_color(Color::WHITE)
            .with_width(2.0),
    );
}

/// Highlights a place: areas get a light fill and an outline, lines a thick stroke.
fn draw_shape(frame: &mut Frame, camera: &Camera, shape: &Shape, viewport: Size, color: Color) {
    let (Shape::Lines(parts) | Shape::Area(parts)) = shape;
    let closed = matches!(shape, Shape::Area(_));

    let path = Path::new(|builder| {
        for part in parts {
            let mut points = part.iter().map(|point| camera.to_screen(*point, viewport));
            let Some(first) = points.next() else {
                continue;
            };
            builder.move_to(first);
            for point in points {
                builder.line_to(point);
            }
            if closed {
                builder.close();
            }
        }
    });

    if closed {
        // Even-odd, so holes such as courtyards stay unfilled.
        frame.fill(
            &path,
            canvas::Fill {
                style: canvas::Style::Solid(Color { a: 0.2, ..color }),
                rule: canvas::fill::Rule::EvenOdd,
            },
        );
        frame.stroke(
            &path,
            canvas::Stroke::default().with_color(color).with_width(2.0),
        );
    } else {
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(Color { a: 0.7, ..color })
                .with_width(6.0)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round),
        );
    }
}
