use cosmic::iced::widget::canvas::{self, Action, Event, Frame, Geometry};
use cosmic::iced::{Point, Rectangle, Size, Vector, mouse};

use super::{MapEvent, MapView};

/// Zoom levels per mouse wheel notch, and per pixel of touchpad scrolling.
const ZOOM_PER_LINE: f64 = 0.5;
const ZOOM_PER_PIXEL: f64 = 0.01;

/// Farthest a press may move, in pixels, and still count as a click.
const CLICK_TOLERANCE: f32 = 4.0;

/// The map's tiles. It turns drags, wheel turns and clicks into map events.
pub(super) struct MapCanvas<'a, F> {
    pub map: &'a MapView,
    pub on_event: F,
}

#[derive(Default)]
pub(super) struct CanvasState {
    size: Size,
    /// Cursor position at the last drag step, while the left button is held.
    drag_from: Option<Point>,
    /// Where the left button was pressed, until the cursor moves too far for a click.
    click_from: Option<Point>,
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
                let position = cursor.position_over(bounds)?;
                state.drag_from = Some(position);
                state.click_from = Some(position);
                Some(Action::capture())
            }

            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let from = state.drag_from?;
                state.drag_from = Some(*position);
                let delta = *position - from;

                if state
                    .click_from
                    .is_some_and(|start| start.distance(*position) > CLICK_TOLERANCE)
                {
                    state.click_from = None;
                }

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

                let Some(start) = state.click_from.take() else {
                    return Some(Action::capture());
                };
                let point = Point::ORIGIN + (start - bounds.position());
                Some(Action::publish((self.on_event)(MapEvent::Click(point))).and_capture())
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
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.map.palette.land);

        for tile in self.map.camera.visible_tiles(bounds.size()) {
            let rect = Rectangle::new(tile.position, Size::new(tile.size, tile.size));

            if let Some(handle) = self.map.tiles.get(&tile.id).and_then(|cached| cached.image.as_ref()) {
                frame.draw_image(rect, handle);
            } else if let Some((ancestor, handle)) = self.map.loaded_ancestor(tile.id) {
                // `with_clip` replaces the frame's clip instead of narrowing it, so keep
                // the region inside the canvas or the scaled-up tile spills over the window.
                let Some(clip) = rect.intersection(&Rectangle::with_size(bounds.size())) else {
                    continue;
                };

                // Show the matching part of a lower-zoom tile, scaled up.
                let levels = tile.id.z - ancestor.z;
                let mask = (1 << levels) - 1;
                let offset = Vector::new(
                    (tile.id.x & mask) as f32 * tile.size,
                    (tile.id.y & mask) as f32 * tile.size,
                );
                let size = tile.size * f32::from(1u16 << levels);

                frame.with_clip(clip, |frame| {
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
                    self.map.palette.placeholder,
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
