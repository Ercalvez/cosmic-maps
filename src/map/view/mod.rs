// SPDX-License-Identifier: MPL-2.0

//! The map widget's state, events and view.

use std::collections::HashMap;
use std::sync::Arc;

mod canvas;
mod markers;
mod routes;

use cosmic::iced::futures::Stream;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Length, Point, Rectangle, Size};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};

use super::camera::{Camera, MAX_ZOOM, MIN_ZOOM, TileId, zoom_showing};
use super::location::Position;
use super::poi::{OsmId, Poi};
use super::state::{MapState, RenderedTile};
use super::style::MapPalette;
use crate::geo::{LatLon, WorldBounds, WorldPoint};
use crate::services::{
    self,
    nominatim::{PoiDetails, Shape},
};
use canvas::MapCanvas;
use markers::MarkerCanvas;
pub use routes::RouteOverlay;
use routes::{ROUTE_PADDING, route_points};

/// View on the first run, until the user's position is known: Berlin.
const INITIAL_CENTER: LatLon = LatLon::new(52.52, 13.405);
const INITIAL_ZOOM: f64 = 12.0;

/// Zoom levels per click on the zoom buttons.
const ZOOM_PER_CLICK: f64 = 1.0;
/// Zoom from which POIs are drawn and clickable, as in GNOME Maps. Below it, the
/// tiles in use only hold a few POIs.
const POI_MIN_ZOOM: f64 = 15.0;
/// Farthest a click may land from a POI marker, in pixels, to select it.
const POI_HIT_RADIUS: f32 = 10.0;
/// The same for POIs without a marker, which have to be aimed at more precisely.
const HIDDEN_POI_HIT_RADIUS: f32 = 6.0;
/// Most zoomed in the map gets when showing the user's position.
const LOCATED_MAX_ZOOM: f64 = 16.0;
/// Viewport span assumed before the map knows its size.
const DEFAULT_SPAN: f32 = 800.0;

/// Zoom used to show a place picked from search, high enough for POI markers.
const PLACE_ZOOM: f64 = 16.0;

/// How many levels up to look for a loaded tile to show while one is missing.
const MAX_FALLBACK_LEVELS: u8 = 6;

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
    /// Fetch details with [`nominatim::fetch`](crate::services::nominatim::fetch) and pass them to
    /// [`MapView::insert_details`].
    FetchDetails(OsmId),
    /// Find what is at a place with [`nominatim::reverse`](crate::services::nominatim::reverse) and pass it to
    /// [`MapView::insert_reverse`].
    Reverse { request: u64, at: LatLon, zoom: u8 },
}

/// A click that hit no POI, waiting for Nominatim to say what is there.
#[derive(Clone, Copy)]
struct Lookup {
    request: u64,
    /// Where the click was, in Web Mercator coordinates spanning 0..1.
    position: WorldPoint,
}

/// Extra information about a POI, by OpenStreetMap object.
pub enum DetailsState {
    Loading,
    Loaded(PoiDetails),
    /// Nominatim does not know the object; not asked again.
    Unavailable,
}

/// What the map has selected, for the place panel.
#[derive(Clone, Copy)]
pub enum Place<'a> {
    Selected {
        poi: &'a Poi,
        /// `None` when the place has no OpenStreetMap object to ask about.
        details: Option<&'a DetailsState>,
    },
    /// A click that hit no POI, until the lookup answers.
    LookingUp,
}

/// A rendered tile, or `None` if it failed to load.
struct CachedTile {
    image: Option<Handle>,
    palette: MapPalette,
}

impl CachedTile {
    /// Rendered with other colours. Failed tiles are not retried for a new palette.
    fn is_stale(&self, palette: &MapPalette) -> bool {
        self.image.is_some() && self.palette != *palette
    }
}

pub struct MapView {
    state: MapState,
    camera: Camera,
    viewport: Size,
    /// Rendered tiles, possibly in previous colours until re-rendered.
    tiles: HashMap<TileId, CachedTile>,
    /// Colours tiles are rendered with; tiles in other colours are re-rendered.
    palette: MapPalette,
    pois: HashMap<TileId, Arc<[Poi]>>,
    /// POI shown in the place panel.
    selected: Option<Poi>,
    /// Shown instead of `selected` until the lookup answers.
    lookup: Option<Lookup>,
    next_lookup: u64,
    details: HashMap<OsmId, DetailsState>,
    /// Last request sent to the worker, to avoid resending it on every pan.
    wanted: Vec<TileId>,
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
        let (state, tiles) = MapState::new(palette);

        let map = Self {
            state,
            camera: Camera::from_lat_lon(center, zoom),
            viewport: Size::ZERO,
            tiles: HashMap::new(),
            palette,
            pois: HashMap::new(),
            selected: None,
            lookup: None,
            next_lookup: 0,
            details: HashMap::new(),
            wanted: Vec::new(),
            user: None,
            following_user: true,
            fit_pending: false,
            routes: None,
        };

        (map, tiles)
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
                if let Some(poi) = self.poi_at(point) {
                    self.lookup = None;
                    self.selected = Some(poi);
                    action = self.request_details();
                } else {
                    action = Some(self.look_up(point));
                }
            }
            MapEvent::ClosePanel => self.close_panel(),
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
        self.lookup = None;
        self.selected = Some(poi);
        self.request_tiles();
        self.request_details()
    }

    /// Latitude and longitude of the center of the map.
    pub fn center(&self) -> LatLon {
        self.camera.center_lat_lon()
    }

    pub fn zoom(&self) -> f64 {
        self.camera.zoom()
    }

    pub fn insert_details(&mut self, osm_id: OsmId, result: Result<PoiDetails, services::Error>) {
        match result {
            Ok(details) => {
                self.details.insert(osm_id, DetailsState::Loaded(details));
            }
            Err(services::Error::NotFound) => {
                self.details.insert(osm_id, DetailsState::Unavailable);
            }
            Err(services::Error::Network(err)) => {
                // Forget the request, so selecting the POI again retries it.
                tracing::warn!("failed to fetch details of {osm_id:?}: {err}");
                self.details.remove(&osm_id);
            }
        }
    }

    fn close_panel(&mut self) {
        self.selected = None;
        self.lookup = None;
    }

    /// Starts finding what is at `point`, which hit no POI.
    fn look_up(&mut self, point: Point) -> MapAction {
        let position = self.camera.to_world(point, self.viewport);
        // The panel shows the lookup, not the previous place, until it answers.
        self.selected = None;
        self.next_lookup += 1;
        self.lookup = Some(Lookup {
            request: self.next_lookup,
            position,
        });

        MapAction::Reverse {
            request: self.next_lookup,
            at: position.to_lat_lon(),
            zoom: lookup_detail(self.camera.zoom()),
        }
    }

    /// Selects what a lookup found, if it is still the latest one.
    pub fn insert_reverse(
        &mut self,
        request: u64,
        result: Result<Option<(Poi, PoiDetails)>, services::Error>,
    ) {
        // A newer click, or closing the panel, replaced it.
        if self.lookup.map(|lookup| lookup.request) != Some(request) {
            return;
        }
        self.lookup = None;

        match result {
            Ok(Some((poi, details))) => {
                // The details came with the answer: no need to fetch them.
                if let Some(osm_id) = poi.osm_id {
                    self.details.insert(osm_id, DetailsState::Loaded(details));
                }
                self.selected = Some(poi);
            }
            Ok(None) => {}
            Err(err) => tracing::warn!("failed to look up what is at the click: {err:?}"),
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

    /// Switches the map's colours. Tiles on screen stay until re-rendered.
    pub fn set_palette(&mut self, palette: MapPalette) {
        if palette == self.palette {
            return;
        }
        self.palette = palette;
        // Before the request below, which the worker then renders with it.
        self.state.set_palette(palette);
        self.request_tiles();
    }

    pub fn insert_tile(&mut self, rendered: RenderedTile) {
        let cached = CachedTile {
            image: rendered.image,
            palette: rendered.palette,
        };
        self.tiles.insert(rendered.tile, cached);
        self.pois.insert(rendered.tile, rendered.pois);
    }

    /// The POI under `point`: the closest marker within `POI_HIT_RADIUS`, or else the
    /// closest POI without one within `HIDDEN_POI_HIT_RADIUS`.
    ///
    /// Markers win, so important POIs stay easy to pick where POIs are dense. POIs
    /// thinned out of the markers can still be picked from `POI_MIN_ZOOM`, since the
    /// tiles in use then hold all of them.
    fn poi_at(&self, point: Point) -> Option<Poi> {
        if self.camera.zoom() < POI_MIN_ZOOM {
            return None;
        }

        closest_poi(self.drawn_pois(self.viewport), point, POI_HIT_RADIUS)
            .or_else(|| {
                closest_poi(
                    self.pois_in_view(self.viewport, u32::MAX),
                    point,
                    HIDDEN_POI_HIT_RADIUS,
                )
            })
            .cloned()
    }

    /// POIs to draw as markers, with their position in the viewport: none below
    /// `POI_MIN_ZOOM`, then the most important first.
    fn drawn_pois(&self, viewport: Size) -> Vec<(&Poi, Point)> {
        match max_poi_rank(self.camera.zoom()) {
            Some(max_rank) => self.pois_in_view(viewport, max_rank),
            None => Vec::new(),
        }
    }

    /// Loaded POIs of at most `max_rank`, with their position in the viewport.
    fn pois_in_view(&self, viewport: Size, max_rank: u32) -> Vec<(&Poi, Point)> {
        let bounds = Rectangle::with_size(viewport);

        self.camera
            .visible_tiles(viewport)
            .iter()
            .filter_map(|tile| self.pois.get(&tile.id))
            .flat_map(|pois| pois.iter())
            .filter(|poi| poi.rank <= max_rank)
            .map(|poi| (poi, self.camera.to_screen(poi.position, viewport)))
            .filter(|(_, position)| bounds.contains(*position))
            .collect()
    }

    /// Where the selected POI is on screen, if there is one and it is in view.
    fn selected_position(&self) -> Option<Point> {
        let position = match (&self.selected, self.lookup) {
            (Some(poi), _) => poi.position,
            (None, Some(lookup)) => lookup.position,
            (None, None) => return None,
        };
        let position = self.camera.to_screen(position, self.viewport);
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
            .filter(|id| {
                self.tiles
                    .get(id)
                    .is_none_or(|cached| cached.is_stale(&self.palette))
            })
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
            .find_map(|id| Some((id, self.tiles.get(&id)?.image.as_ref()?)))
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

    /// The outline or line of the selected place, once its details have arrived.
    fn selected_shape(&self) -> Option<&Shape> {
        let osm_id = self.selected.as_ref()?.osm_id?;
        match self.details.get(&osm_id)? {
            DetailsState::Loaded(details) => details.shape.as_ref(),
            DetailsState::Loading | DetailsState::Unavailable => None,
        }
    }

    /// The selected place, once known.
    pub fn selected(&self) -> Option<&Poi> {
        self.selected.as_ref()
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
        self.selected.is_some() || self.lookup.is_some()
    }

    /// What the place panel shows, or `None` when nothing is selected.
    pub fn place(&self) -> Option<Place<'_>> {
        match (&self.selected, self.lookup) {
            (Some(poi), _) => Some(Place::Selected {
                poi,
                details: poi.osm_id.and_then(|osm_id| self.details.get(&osm_id)),
            }),
            (None, Some(_)) => Some(Place::LookingUp),
            (None, None) => None,
        }
    }
}

/// Nominatim detail level for a click at map `zoom`. Its levels are coarser than map
/// zooms (16 and 17 are streets, 18 buildings), so they are shifted: buildings from
/// map zoom 15, where they are clearly visible, down to counties around zoom 5.
fn lookup_detail(zoom: f64) -> u8 {
    const SHIFT: f64 = 3.0;
    (zoom.round() + SHIFT).clamp(3.0, 18.0) as u8
}

/// The POI closest to `point` within `radius` pixels.
fn closest_poi(pois: Vec<(&Poi, Point)>, point: Point, radius: f32) -> Option<&Poi> {
    pois.into_iter()
        .map(|(poi, position)| (poi, position.distance(point)))
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(poi, _)| poi)
}

/// Highest POI rank drawn at `zoom`, following GNOME Maps' density limits, or
/// `None` when no POI is drawn.
fn max_poi_rank(zoom: f64) -> Option<u32> {
    if zoom < POI_MIN_ZOOM {
        None
    } else if zoom < 17.0 {
        Some(50)
    } else {
        Some(100)
    }
}
