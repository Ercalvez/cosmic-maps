// SPDX-License-Identifier: MPL-2.0

//! The map widget's state, events and view.

mod canvas;
mod markers;
mod routes;
mod selection;
mod tiles;

use cosmic::iced::futures::Stream;
use cosmic::iced::{Length, Point, Size};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};

use super::camera::{Camera, MAX_ZOOM, MIN_ZOOM, zoom_showing};
use super::state::RenderedTile;
use super::style::MapPalette;
use crate::domain::place::{OsmId, Poi, PoiDetails};
use crate::geo::{LatLon, Position, WorldBounds};
use canvas::MapCanvas;
use markers::MarkerCanvas;
pub use routes::RouteOverlay;
use routes::{ROUTE_PADDING, route_points};
use selection::Selection;
pub use selection::{DetailsAnswer, DetailsState, Place};
use tiles::Tiles;

/// View on the first run, until the user's position is known: Berlin.
const INITIAL_CENTER: LatLon = LatLon::new(52.52, 13.405);
const INITIAL_ZOOM: f64 = 12.0;

/// Zoom levels per click on the zoom buttons.
const ZOOM_PER_CLICK: f64 = 1.0;
/// Most zoomed in the map gets when showing the user's position.
const LOCATED_MAX_ZOOM: f64 = 16.0;
/// Viewport span assumed before the map knows its size.
const DEFAULT_SPAN: f32 = 800.0;

/// Zoom used to show a place picked from search, high enough for POI markers.
const PLACE_ZOOM: f64 = 16.0;

/// Interactions reported by the map widget.
#[derive(Debug, Clone, Copy)]
pub enum MapEvent {
    /// Move the map content by this many screen pixels.
    Pan {
        dx: f64,
        dy: f64,
    },
    /// Zoom by `delta` levels around `anchor`, or around the center if `None`.
    Zoom {
        delta: f64,
        anchor: Option<Point>,
    },
    Resized(Size),
    /// A click without dragging, at this position in the map.
    Click(Point),
    ClosePanel,
    /// Center the map on the user's position.
    Locate,
}

/// Work the map needs the app to do, returned from [`MapView::on_event`].
#[derive(Debug, Clone, Copy)]
pub enum MapAction {
    /// Fetch the details of this object, and pass the answer to
    /// [`MapView::insert_details`].
    FetchDetails(OsmId),
    /// Find what is at a place, and pass the answer to [`MapView::insert_reverse`].
    Reverse { request: u64, at: LatLon, zoom: u8 },
}

pub struct MapView {
    camera: Camera,
    viewport: Size,
    tiles: Tiles,
    selection: Selection,
    /// Where the user is, once known.
    user: Option<Position>,
    /// The map re-centres on each new position until the user moves it.
    following_user: bool,
    /// The routes are to be framed once the map knows its size.
    fit_pending: bool,
    /// Directions results, while shown.
    routes: Option<RouteOverlay>,
}

impl MapView {
    /// Creates the map, looking at `start` (a place and a zoom) if given. The
    /// stream yields each tile when it is rendered; feed them back through
    /// [`MapView::insert_tile`].
    pub fn new(
        palette: MapPalette,
        start: Option<(LatLon, f64)>,
    ) -> (Self, impl Stream<Item = RenderedTile> + Send + 'static) {
        let (center, zoom) = start.unwrap_or((INITIAL_CENTER, INITIAL_ZOOM));
        let (tiles, rendered) = Tiles::new(palette);

        let map = Self {
            camera: Camera::from_lat_lon(center, zoom),
            viewport: Size::ZERO,
            tiles,
            selection: Selection::default(),
            user: None,
            following_user: true,
            fit_pending: false,
            routes: None,
        };

        (map, rendered)
    }

    pub fn on_event(&mut self, event: MapEvent) -> Option<MapAction> {
        let mut action = None;

        match event {
            MapEvent::Pan { dx, dy } => {
                self.following_user = false;
                self.camera.pan(dx, dy);
            }
            MapEvent::Zoom { delta, anchor } => {
                self.following_user = false;
                let anchor = anchor.unwrap_or(Point::new(
                    self.viewport.width / 2.0,
                    self.viewport.height / 2.0,
                ));
                self.camera.zoom_at(delta, anchor, self.viewport);
            }
            MapEvent::Resized(size) => {
                self.viewport = size;
                if std::mem::take(&mut self.fit_pending) {
                    self.fit_route();
                }
            }
            // Every click shows what is there; the panel closes with its own button.
            MapEvent::Click(point) => {
                action = match self.tiles.poi_at(&self.camera, self.viewport, point) {
                    Some(poi) => self.selection.select(poi),
                    None => Some(self.look_up(point)),
                };
            }
            MapEvent::ClosePanel => self.selection.close(),
            MapEvent::Locate => {
                self.following_user = true;
                self.center_on_user();
            }
        }

        self.request_tiles();
        action
    }

    /// Centers the map on `poi` and shows it in the place panel, such as for a search
    /// result.
    pub fn show_place(&mut self, poi: Poi) -> Option<MapAction> {
        self.following_user = false;
        self.camera = Camera::centered_on(poi.position, PLACE_ZOOM);
        self.request_tiles();
        self.selection.select(poi)
    }

    /// Latitude and longitude of the center of the map.
    pub fn center(&self) -> LatLon {
        self.camera.center_lat_lon()
    }

    pub fn zoom(&self) -> f64 {
        self.camera.zoom()
    }

    /// Stores the answer to a [`MapAction::FetchDetails`].
    pub fn insert_details(&mut self, osm_id: OsmId, answer: DetailsAnswer) {
        self.selection.insert_details(osm_id, answer);
    }

    /// Selects what a lookup found, if there is anything and it is still the latest
    /// one.
    pub fn insert_reverse(&mut self, request: u64, found: Option<(Poi, PoiDetails)>) {
        self.selection.insert_reverse(request, found);
    }

    /// Starts finding what is at `point`, which hit no POI.
    fn look_up(&mut self, point: Point) -> MapAction {
        let position = self.camera.to_world(point, self.viewport);
        self.selection
            .look_up(position, lookup_detail(self.camera.zoom()))
    }

    /// Switches the map's colours. Tiles on screen stay until re-rendered.
    pub fn set_palette(&mut self, palette: MapPalette) {
        if self.tiles.set_palette(palette) {
            self.request_tiles();
        }
    }

    pub fn insert_tile(&mut self, rendered: RenderedTile) {
        self.tiles.insert(rendered);
    }

    /// Where the selected POI is on screen, if there is one and it is in view.
    fn selected_position(&self) -> Option<Point> {
        let position = self
            .camera
            .to_screen(self.selection.position()?, self.viewport);
        cosmic::iced::Rectangle::with_size(self.viewport)
            .contains(position)
            .then_some(position)
    }

    /// Asks the worker for visible tiles that are not rendered yet.
    fn request_tiles(&mut self) {
        self.tiles.request(&self.camera, self.viewport);
    }

    pub fn view<'a, Message>(
        &'a self,
        on_event: impl Fn(MapEvent) -> Message + 'a,
    ) -> Element<'a, Message>
    where
        Message: Clone + 'static,
    {
        let spacing = cosmic::theme::spacing();
        let zoom = |delta| {
            on_event(MapEvent::Zoom {
                delta,
                anchor: None,
            })
        };

        let zoom_in = widget::button::icon(icon::from_name("zoom-in-symbolic"))
            .on_press_maybe((self.camera.zoom() < MAX_ZOOM).then(|| zoom(ZOOM_PER_CLICK)));
        let zoom_out = widget::button::icon(icon::from_name("zoom-out-symbolic"))
            .on_press_maybe((self.camera.zoom() > MIN_ZOOM).then(|| zoom(-ZOOM_PER_CLICK)));

        let zoom_buttons = widget::container(
            widget::column::with_capacity(2)
                .push(zoom_in)
                .push(zoom_out)
                .spacing(spacing.space_xxxs),
        )
        .padding(spacing.space_xxxs)
        .class(cosmic::theme::Container::Card);

        // Disabled until the user's position is known.
        let locate = widget::button::icon(icon::from_name("find-location-symbolic"))
            .on_press_maybe(self.user.is_some().then(|| on_event(MapEvent::Locate)));
        let locate = widget::container(locate)
            .padding(spacing.space_xxxs)
            .class(cosmic::theme::Container::Card);

        let controls = widget::column::with_capacity(2)
            .push(zoom_buttons)
            .push(locate)
            .spacing(spacing.space_xs)
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
        // A separate canvas, so markers get their own layer: within a layer, iced
        // draws images such as tiles above shapes such as markers.
        let markers = widget::canvas(MarkerCanvas { map: self })
            .width(Length::Fill)
            .height(Length::Fill);

        cosmic::iced::widget::stack![map, markers, controls].into()
    }

    /// Records where the user is. Until the user moves the map, it re-centres there,
    /// so a better estimate replaces a rough first one.
    pub fn set_position(&mut self, position: Position) {
        self.user = Some(position);
        if self.following_user {
            self.center_on_user();
        }
    }

    /// Centers the map on the user, zoomed so the area they are in fits.
    fn center_on_user(&mut self) {
        let Some(user) = self.user else {
            return;
        };
        let span = match self.viewport.width.min(self.viewport.height) {
            span if span > 0.0 => span,
            _ => DEFAULT_SPAN,
        };
        let zoom = zoom_showing(user.accuracy, user.lat_lon.lat, span).min(LOCATED_MAX_ZOOM);

        self.camera = Camera::centered_on(user.lat_lon.to_world(), zoom);
        self.request_tiles();
    }

    /// The selected place, once known.
    pub fn selected(&self) -> Option<&Poi> {
        self.selection.selected()
    }

    /// Where the user is, once known.
    pub fn user_position(&self) -> Option<Position> {
        self.user
    }

    /// Draws directions results, or clears them with `None`.
    pub fn set_routes(&mut self, routes: Option<RouteOverlay>) {
        self.routes = routes;
    }

    /// Frames every route option, beside the drawer; the map stops following the user.
    pub fn fit_route(&mut self) {
        let Some(routes) = &self.routes else {
            return;
        };
        let Some(bounds) = WorldBounds::around(routes.itineraries.iter().flat_map(route_points))
        else {
            return;
        };

        self.following_user = false;
        // Before the first layout, a guessed size would frame them too closely.
        if self.viewport.width <= 0.0 || self.viewport.height <= 0.0 {
            self.fit_pending = true;
            return;
        }
        self.camera = Camera::fitting(bounds, self.viewport, ROUTE_PADDING);
        self.request_tiles();
    }

    /// Whether a place is selected or being looked up, for the place panel.
    pub fn has_place(&self) -> bool {
        self.selection.has_place()
    }

    /// What the place panel shows, or `None` when nothing is selected.
    pub fn place(&self) -> Option<Place<'_>> {
        self.selection.place()
    }
}

/// Nominatim detail level for a click at map `zoom`. Its levels are coarser than map
/// zooms (16 and 17 are streets, 18 buildings), so they are shifted: buildings from
/// map zoom 15, where they are clearly visible, down to counties around zoom 5.
fn lookup_detail(zoom: f64) -> u8 {
    const SHIFT: f64 = 3.0;
    (zoom.round() + SHIFT).clamp(3.0, 18.0) as u8
}
