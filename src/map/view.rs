use std::collections::HashMap;

use cosmic::iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use cosmic::iced::futures::Stream;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Color, Length, Point, Rectangle, Size, Vector, mouse};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};

use super::camera::{Camera, MAX_ZOOM, MIN_ZOOM, TileId};
use super::state::{MapState, RenderedTile};

/// Initial view: Berlin.
const INITIAL_CENTER: (f64, f64) = (52.52, 13.405);
const INITIAL_ZOOM: f64 = 12.0;

/// Zoom levels per click on the zoom buttons.
const ZOOM_PER_CLICK: f64 = 1.0;
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
    /// Creates the map. The stream yields each tile when it is rendered; feed them
    /// back through [`MapView::insert_tile`].
    pub fn new() -> (Self, impl Stream<Item = RenderedTile> + Send + 'static) {
        let (lat, lon) = INITIAL_CENTER;
        let (state, tiles) = MapState::new();

        let map = Self {
            state,
            camera: Camera::from_lat_lon(lat, lon, INITIAL_ZOOM),
            viewport: Size::ZERO,
            tiles: HashMap::new(),
            wanted: Vec::new(),
        };

        (map, tiles)
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

    pub fn insert_tile(&mut self, rendered: RenderedTile) {
        self.tiles.insert(rendered.tile, rendered.image);
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
        Message: Clone + 'static,
    {
        let spacing = cosmic::theme::spacing();
        let zoom = |delta| on_event(MapEvent::Zoom { delta, anchor: None });

        let zoom_in = widget::button::icon(icon::from_name("zoom-in-symbolic"))
            .on_press_maybe((self.camera.zoom() < MAX_ZOOM).then(|| zoom(ZOOM_PER_CLICK)));
        let zoom_out = widget::button::icon(icon::from_name("zoom-out-symbolic"))
            .on_press_maybe((self.camera.zoom() > MIN_ZOOM).then(|| zoom(-ZOOM_PER_CLICK)));

        let controls = widget::container(
            widget::column::with_capacity(2)
                .push(zoom_in)
                .push(zoom_out)
                .spacing(spacing.space_xxxs),
        )
        .padding(spacing.space_xxxs)
        .class(cosmic::theme::Container::Card)
        .apply(widget::container)
        .padding(spacing.space_s)
        .align_right(Length::Fill)
        .align_top(Length::Fill);

        let map = widget::canvas(MapCanvas {
            map: self,
            on_event,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        cosmic::iced::widget::stack![map, controls].into()
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
