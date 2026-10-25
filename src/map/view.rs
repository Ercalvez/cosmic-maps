use std::collections::HashMap;
use std::sync::Arc;

use cosmic::iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path};
use cosmic::iced::futures::Stream;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Color, Length, Point, Rectangle, Size, Vector, mouse};
use cosmic::prelude::*;
use cosmic::widget::{self, icon, popover};

use crate::fl;

use super::camera::{Camera, MAX_ZOOM, MIN_ZOOM, TileId};
use super::details::{self, PoiDetails, Wheelchair};
use super::poi::{OsmId, Poi};
use super::state::{MapState, RenderedTile};

/// Initial view: Berlin.
const INITIAL_CENTER: (f64, f64) = (52.52, 13.405);
const INITIAL_ZOOM: f64 = 12.0;

/// Zoom levels per click on the zoom buttons.
const ZOOM_PER_CLICK: f64 = 1.0;
/// Zoom levels per mouse wheel notch, and per pixel of touchpad scrolling.
const ZOOM_PER_LINE: f64 = 0.5;
const ZOOM_PER_PIXEL: f64 = 0.01;

/// Farthest a press may move, in pixels, and still count as a click.
const CLICK_TOLERANCE: f32 = 4.0;
/// Farthest a click may land from a POI, in pixels, to select it.
const POI_HIT_RADIUS: f32 = 20.0;
const MARKER_RADIUS: f32 = 6.0;
/// Gap between the selected POI and the corner of its popover.
const POPOVER_OFFSET: f32 = 8.0;
/// Keeps long addresses and opening hours from making the popover too wide.
const POPOVER_MAX_WIDTH: f32 = 360.0;

/// How many levels up to look for a loaded tile to show while one is missing.
const MAX_FALLBACK_LEVELS: u8 = 6;

const BACKGROUND: Color = Color::from_rgb(0.85, 0.85, 0.85);
const PLACEHOLDER: Color = Color::from_rgb(0.78, 0.78, 0.78);

/// Interactions reported by the map widget.
#[derive(Debug, Clone)]
pub enum MapEvent {
    /// Move the map content by this many screen pixels.
    Pan { dx: f64, dy: f64 },
    /// Zoom by `delta` levels around `anchor`, or around the center if `None`.
    Zoom { delta: f64, anchor: Option<Point> },
    Resized(Size),
    /// A click without dragging, at this position in the map.
    Click(Point),
    ClosePopover,
    /// A link in the popover, such as a website or a `tel:` URI.
    OpenUrl(String),
}

/// Work the map needs the app to do, returned from [`MapView::on_event`].
#[derive(Debug, Clone)]
pub enum MapAction {
    /// Fetch details with [`details::fetch`] and pass them to
    /// [`MapView::insert_details`].
    FetchDetails(OsmId),
    OpenUrl(String),
}

/// Extra information about a POI, by OpenStreetMap object.
enum DetailsState {
    Loading,
    Loaded(PoiDetails),
    /// Nominatim does not know the object; not asked again.
    Unavailable,
}

pub struct MapView {
    state: MapState,
    camera: Camera,
    viewport: Size,
    /// Rendered tiles; `None` marks tiles that failed to load.
    tiles: HashMap<TileId, Option<Handle>>,
    pois: HashMap<TileId, Arc<[Poi]>>,
    /// POI whose popover is shown.
    selected: Option<Poi>,
    details: HashMap<OsmId, DetailsState>,
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
            pois: HashMap::new(),
            selected: None,
            details: HashMap::new(),
            wanted: Vec::new(),
        };

        (map, tiles)
    }

    pub fn on_event(&mut self, event: MapEvent) -> Option<MapAction> {
        let mut action = None;

        match event {
            MapEvent::Pan { dx, dy } => self.camera.pan(dx, dy),
            MapEvent::Zoom { delta, anchor } => {
                let anchor = anchor.unwrap_or(Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0));
                self.camera.zoom_at(delta, anchor, self.viewport);
            }
            MapEvent::Resized(size) => self.viewport = size,
            MapEvent::Click(point) => {
                self.selected = self.poi_at(point);
                action = self.request_details();
            }
            MapEvent::ClosePopover => self.selected = None,
            MapEvent::OpenUrl(url) => action = Some(MapAction::OpenUrl(url)),
        }

        self.request_tiles();
        action
    }

    pub fn insert_details(&mut self, osm_id: OsmId, result: Result<PoiDetails, details::Error>) {
        match result {
            Ok(details) => {
                self.details.insert(osm_id, DetailsState::Loaded(details));
            }
            Err(details::Error::NotFound) => {
                self.details.insert(osm_id, DetailsState::Unavailable);
            }
            Err(details::Error::Network(err)) => {
                // Forget the request, so selecting the POI again retries it.
                eprintln!("failed to fetch details of {osm_id:?}: {err}");
                self.details.remove(&osm_id);
            }
        }
    }

    /// Asks for the selected POI's details, unless they are known or on their way.
    fn request_details(&mut self) -> Option<MapAction> {
        let osm_id = self.selected.as_ref()?.osm_id?;
        if self.details.contains_key(&osm_id) {
            return None;
        }

        self.details.insert(osm_id, DetailsState::Loading);
        Some(MapAction::FetchDetails(osm_id))
    }

    pub fn insert_tile(&mut self, rendered: RenderedTile) {
        self.tiles.insert(rendered.tile, rendered.image);
        self.pois.insert(rendered.tile, rendered.pois);
    }

    /// The POI closest to `point`, if one is within `POI_HIT_RADIUS`.
    fn poi_at(&self, point: Point) -> Option<Poi> {
        self.camera
            .visible_tiles(self.viewport)
            .iter()
            .filter_map(|tile| self.pois.get(&tile.id))
            .flat_map(|pois| pois.iter())
            .map(|poi| {
                let distance = self.camera.to_screen(poi.position, self.viewport).distance(point);
                (poi, distance)
            })
            .filter(|(_, distance)| *distance <= POI_HIT_RADIUS)
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(poi, _)| poi.clone())
    }

    /// Where the selected POI is on screen, if there is one and it is in view.
    fn selected_position(&self) -> Option<Point> {
        let position = self.camera.to_screen(self.selected.as_ref()?.position, self.viewport);
        Rectangle::with_size(self.viewport)
            .contains(position)
            .then_some(position)
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

        let close = on_event(MapEvent::ClosePopover);
        let poi_popup = self
            .selected
            .as_ref()
            .zip(self.selected_position())
            .map(|(poi, position)| {
                let details = poi.osm_id.and_then(|osm_id| self.details.get(&osm_id));
                (poi_popup(poi, details, &on_event), position)
            });

        let map = widget::canvas(MapCanvas {
            map: self,
            on_event,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        // Clicks outside the map, such as on the nav bar, also close the popover.
        let mut map = popover(cosmic::iced::widget::stack![map, controls]).on_close(close);
        if let Some((popup, position)) = poi_popup {
            let corner = position + Vector::new(POPOVER_OFFSET, POPOVER_OFFSET);
            map = map.popup(popup).position(popover::Position::Point(corner));
        }

        map.into()
    }
}

fn poi_popup<'a, Message: Clone + 'static>(
    poi: &'a Poi,
    details: Option<&'a DetailsState>,
    on_event: &impl Fn(MapEvent) -> Message,
) -> Element<'a, Message> {
    let spacing = cosmic::theme::spacing();

    // Translated name first, with the local name below as a caption, the way
    // libcosmic's settings items pair a title with its description.
    let names = match &poi.translated_name {
        Some(translated) => widget::column::with_capacity(3)
            .push(widget::text::heading(translated))
            .push(widget::text::caption(&poi.name)),
        None => widget::column::with_capacity(2).push(widget::text::heading(&poi.name)),
    };
    let names = names
        .push_maybe(poi.category().map(widget::text::caption))
        .spacing(2);

    let close = widget::button::icon(icon::from_name("window-close-symbolic"))
        .on_press(on_event(MapEvent::ClosePopover));
    let header = widget::row::with_capacity(2)
        .push(names.width(Length::Fill))
        .push(close)
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(spacing.space_xs);

    let details: Option<Element<'a, Message>> = match details {
        Some(DetailsState::Loading) => {
            Some(widget::text::caption(fl!("poi-details-loading")).into())
        }
        Some(DetailsState::Loaded(details)) => details_rows(details, on_event),
        Some(DetailsState::Unavailable) | None => None,
    };

    widget::container(
        widget::column::with_capacity(3)
            .push(header)
            .push_maybe(details.is_some().then(widget::divider::horizontal::default))
            .push_maybe(details)
            .spacing(spacing.space_xs),
    )
    .max_width(POPOVER_MAX_WIDTH)
    .padding([spacing.space_xxs, spacing.space_xxs, spacing.space_xs, spacing.space_s])
    .class(cosmic::theme::Container::Dropdown)
    .into()
}

/// One row per known detail, or `None` if there are none.
fn details_rows<'a, Message: Clone + 'static>(
    details: &'a PoiDetails,
    on_event: &impl Fn(MapEvent) -> Message,
) -> Option<Element<'a, Message>> {
    let spacing = cosmic::theme::spacing();
    let row = |icon_name: &'static str, content: Element<'a, Message>| -> Element<'a, Message> {
        widget::row::with_capacity(2)
            .push(icon::from_name(icon_name).size(16).icon())
            .push(content)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(spacing.space_xs)
            .into()
    };
    let link = |label: &'a str, url: String| -> Element<'a, Message> {
        // `link` needs an owned label.
        widget::button::link(label.to_owned())
            .padding(0)
            .on_press(on_event(MapEvent::OpenUrl(url)))
            .into()
    };

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if let Some(address) = &details.address {
        rows.push(row("mark-location-symbolic", widget::text::body(address).into()));
    }
    if let Some(hours) = &details.opening_hours {
        rows.push(row("document-open-recent-symbolic", widget::text::body(hours).into()));
    }
    if let Some(website) = &details.website {
        rows.push(row("web-browser-symbolic", link(website, website.clone())));
    }
    if let Some(phone) = &details.phone {
        let uri = format!("tel:{}", phone.replace(' ', ""));
        rows.push(row("call-start-symbolic", link(phone, uri)));
    }
    if let Some(wheelchair) = details.wheelchair {
        let label = match wheelchair {
            Wheelchair::Yes => fl!("poi-wheelchair-yes"),
            Wheelchair::Limited => fl!("poi-wheelchair-limited"),
            Wheelchair::No => fl!("poi-wheelchair-no"),
        };
        rows.push(row(
            "preferences-desktop-accessibility-symbolic",
            widget::text::body(label).into(),
        ));
    }

    (!rows.is_empty()).then(|| {
        widget::column::with_children(rows)
            .spacing(spacing.space_xxs)
            .into()
    })
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
        theme: &cosmic::Theme,
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
                    PLACEHOLDER,
                );
            }
        }

        if let Some(position) = self.map.selected_position() {
            let marker = Path::circle(position, MARKER_RADIUS);
            frame.fill(&marker, Color::from(theme.cosmic().accent_color()));
            frame.stroke(
                &marker,
                canvas::Stroke::default().with_color(Color::WHITE).with_width(2.0),
            );
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
