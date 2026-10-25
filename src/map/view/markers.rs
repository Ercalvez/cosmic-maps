use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Color, Rectangle, Size, mouse};

use super::MapView;
use super::routes::{draw_route_stops, draw_routes};
use crate::map::camera::{Camera, world_position};
use crate::map::details::Shape;
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

        // The selected place's extent, under everything else.
        if let Some(shape) = self.map.selected_shape() {
            draw_shape(&mut frame, &self.map.camera, shape, bounds.size(), accent);
        }

        if let Some(routes) = &self.map.routes {
            draw_routes(&mut frame, &self.map.camera, routes, bounds.size(), &self.map.palette, accent);
        }

        // The area the user is in, under the POI markers.
        let user = self.map.user.map(|user| {
            let center = self
                .map
                .camera
                .to_screen(world_position(user.latitude, user.longitude), bounds.size());
            let radius = (user.accuracy / self.map.camera.meters_per_pixel(user.latitude)) as f32;
            (center, radius)
        });
        if let Some((center, radius)) = user {
            // Skip it when it is hidden by the dot, or when it covers the whole view.
            let diagonal = bounds.width.hypot(bounds.height);
            if radius > USER_DOT_RADIUS && radius < 4.0 * diagonal {
                let area = Path::circle(center, radius);
                frame.fill(&area, Color { a: 0.15, ..USER_COLOR });
                frame.stroke(
                    &area,
                    canvas::Stroke::default()
                        .with_color(Color { a: 0.5, ..USER_COLOR })
                        .with_width(1.0),
                );
            }
        }

        // One path for all markers: there can be hundreds.
        let pois = self.map.drawn_pois(bounds.size());
        if !pois.is_empty() {
            let markers = Path::new(|builder| {
                for (_, position) in &pois {
                    builder.circle(*position, POI_MARKER_RADIUS);
                }
            });
            let MapPalette { poi_marker, poi_outline, .. } = self.map.palette;
            frame.fill(&markers, poi_marker);
            frame.stroke(
                &markers,
                canvas::Stroke::default().with_color(poi_outline).with_width(1.5),
            );
        }

        // Route stops above the POI markers, so they stay visible.
        if let Some(itinerary) = self
            .map
            .routes
            .as_ref()
            .and_then(|routes| routes.itineraries.get(routes.chosen))
        {
            draw_route_stops(&mut frame, &self.map.camera, itinerary, bounds.size(), &self.map.palette, accent);
        }

        if let Some((center, _)) = user {
            let dot = Path::circle(center, USER_DOT_RADIUS);
            frame.fill(&dot, USER_COLOR);
            frame.stroke(
                &dot,
                canvas::Stroke::default().with_color(Color::WHITE).with_width(2.0),
            );
        }

        if let Some(position) = self.map.selected_position() {
            let marker = Path::circle(position, MARKER_RADIUS);
            frame.fill(&marker, accent);
            frame.stroke(
                &marker,
                canvas::Stroke::default().with_color(Color::WHITE).with_width(2.0),
            );
        }

        vec![frame.into_geometry()]
    }
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
