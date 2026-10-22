use std::collections::HashMap;

use cosmic::iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Color, Length, Point, Rectangle, Size, Vector, mouse};
use cosmic::prelude::*;
use cosmic::widget;

use super::camera::{Camera, TileId};
use super::state::MapState;

/// Initial view: Berlin.
const INITIAL_CENTER: (f64, f64) = (52.52, 13.405);
const INITIAL_ZOOM: f64 = 12.0;

/// Zoom levels per mouse wheel notch, and per pixel of touchpad scrolling.
const ZOOM_PER_LINE: f64 = 0.5;
const ZOOM_PER_PIXEL: f64 = 0.01;

/// How many levels up to look for a loaded tile to show while one is missing.
const MAX_FALLBACK_LEVELS: u8 = 6;

const BACKGROUND: Color = Color::from_rgb(0.85, 0.85, 0.85);
const PLACEHOLDER: Color = Color::from_rgb(0.78, 0.78, 0.78);

/// Interactions reported by the map widget.
#[derive(Debug, Clone, Copy)]
pub enum MapEvent {
    /// Move the map content by this many screen pixels.
    Pan { dx: f64, dy: f64 },
    /// Zoom by `delta` levels around `anchor`, or around the center if `None`.
    Zoom { delta: f64, anchor: Option<Point> },
    Resized(Size),
}

pub struct MapView {
    state: MapState,
    camera: Camera,
    viewport: Size,
    /// Rendered tiles; `None` marks tiles that failed to load.
    tiles: HashMap<TileId, Option<Handle>>,
    /// Last request sent to the worker, to avoid resending it on every pan.
    wanted: Vec<TileId>,
}

impl MapView {
    pub fn new() -> Self {
        let (lat, lon) = INITIAL_CENTER;

        Self {
            state: MapState::new(),
            camera: Camera::from_lat_lon(lat, lon, INITIAL_ZOOM),
            viewport: Size::ZERO,
            tiles: HashMap::new(),
            wanted: Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: MapEvent) {
        match event {
            MapEvent::Pan { dx, dy } => self.camera.pan(dx, dy),
            MapEvent::Zoom { delta, anchor } => {
                let anchor = anchor.unwrap_or(Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0));
                self.camera.zoom_at(delta, anchor, self.viewport);
            }
            MapEvent::Resized(size) => self.viewport = size,
        }

        self.request_tiles();
    }

    /// Stores the tiles the worker finished since the last call.
    pub fn update_image(&mut self) {
        while let Some(rendered) = self.state.try_tile() {
            let handle = rendered
                .image
                .map(|image| Handle::from_rgba(image.width, image.height, image.pixels));
            self.tiles.insert(rendered.tile, handle);
        }
    }

    /// Asks the worker for visible tiles that are not rendered yet.
    fn request_tiles(&mut self) {
        let missing: Vec<TileId> = self
            .camera
            .visible_tiles(self.viewport)
            .into_iter()
            .map(|tile| tile.id)
            .filter(|id| !self.tiles.contains_key(id))
            .collect();

        if missing != self.wanted {
            self.state.want(missing.clone());
            self.wanted = missing;
        }
    }

    /// The closest loaded lower-zoom tile that covers `tile`.
    fn loaded_ancestor(&self, tile: TileId) -> Option<(TileId, &Handle)> {
        (1..=MAX_FALLBACK_LEVELS)
            .filter_map(|levels| tile.ancestor(levels))
            .find_map(|id| Some((id, self.tiles.get(&id)?.as_ref()?)))
    }

    pub fn view<'a, Message>(
        &'a self,
        on_event: impl Fn(MapEvent) -> Message + 'a,
    ) -> Element<'a, Message>
    where
        Message: 'static,
    {
        widget::canvas(MapCanvas {
            map: self,
            on_event,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

struct MapCanvas<'a, F> {
    map: &'a MapView,
    on_event: F,
}

#[derive(Default)]
struct CanvasState {
    size: Size,
    /// Cursor position at the last drag step, while the left button is held.
    drag_from: Option<Point>,
}

impl<Message, F> canvas::Program<Message, cosmic::Theme, cosmic::Renderer> for MapCanvas<'_, F>
where
    F: Fn(MapEvent) -> Message,
{
    type State = CanvasState;

    fn update(
        &self,
        state: &mut CanvasState,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if state.size != bounds.size() {
            state.size = bounds.size();
            return Some(Action::publish((self.on_event)(MapEvent::Resized(
                state.size,
            ))));
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // Window coordinates, like `CursorMoved` positions.
                state.drag_from = Some(cursor.position_over(bounds)?);
                Some(Action::capture())
            }

            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let from = state.drag_from?;
                state.drag_from = Some(*position);
                let delta = *position - from;

                Some(
                    Action::publish((self.on_event)(MapEvent::Pan {
                        dx: f64::from(delta.x),
                        dy: f64::from(delta.y),
                    }))
                    .and_capture(),
                )
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag_from.take()?;
                Some(Action::capture())
            }

            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let anchor = cursor.position_in(bounds)?;
                let delta = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => f64::from(*y) * ZOOM_PER_LINE,
                    mouse::ScrollDelta::Pixels { y, .. } => f64::from(*y) * ZOOM_PER_PIXEL,
                };

                Some(
                    Action::publish((self.on_event)(MapEvent::Zoom {
                        delta,
                        anchor: Some(anchor),
                    }))
                    .and_capture(),
                )
            }

            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &CanvasState,
        renderer: &cosmic::Renderer,
        _theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<cosmic::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), BACKGROUND);

        for tile in self.map.camera.visible_tiles(bounds.size()) {
            let rect = Rectangle::new(tile.position, Size::new(tile.size, tile.size));

            if let Some(Some(handle)) = self.map.tiles.get(&tile.id) {
                frame.draw_image(rect, handle);
            } else if let Some((ancestor, handle)) = self.map.loaded_ancestor(tile.id) {
                // Show the matching part of a lower-zoom tile, scaled up.
                let levels = tile.id.z - ancestor.z;
                let mask = (1 << levels) - 1;
                let offset = Vector::new(
                    (tile.id.x & mask) as f32 * tile.size,
                    (tile.id.y & mask) as f32 * tile.size,
                );
                let size = tile.size * f32::from(1u16 << levels);

                frame.with_clip(rect, |frame| {
                    frame.draw_image(
                        Rectangle::new(tile.position - offset, Size::new(size, size)),
                        handle,
                    );
                });
            } else {
                // Inset so missing tiles show up as a grid.
                frame.fill_rectangle(
                    rect.position() + Vector::new(1.0, 1.0),
                    Size::new(tile.size - 2.0, tile.size - 2.0),
                    PLACEHOLDER,
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &CanvasState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag_from.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}
