use std::collections::HashMap;
use std::sync::Arc;

use cosmic::iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path};
use cosmic::iced::futures::Stream;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Color, Length, Point, Rectangle, Size, Vector, mouse};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};

use crate::fl;

use super::camera::{Camera, MAX_ZOOM, MIN_ZOOM, TileId, lat_lon, world_position, zoom_showing};
use super::location::Position;
use super::details::{self, PoiDetails, Shape, Wheelchair};
use super::poi::{OsmId, Poi};
use super::routing::{Itinerary, Leg};
use super::state::{MapState, RenderedTile};
use super::style::MapPalette;

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
/// Zoom from which POIs are drawn and clickable, as in GNOME Maps. Below it, the
/// tiles in use only hold a few POIs.
const POI_MIN_ZOOM: f64 = 15.0;
const POI_MARKER_RADIUS: f32 = 3.5;
/// Farthest a click may land from a POI marker, in pixels, to select it.
const POI_HIT_RADIUS: f32 = 10.0;
/// The same for POIs without a marker, which have to be aimed at more precisely.
const HIDDEN_POI_HIT_RADIUS: f32 = 6.0;
const MARKER_RADIUS: f32 = 6.0;
/// Most zoomed in the map gets when showing the user's position.
const LOCATED_MAX_ZOOM: f64 = 16.0;
/// Viewport span assumed before the map knows its size.
const DEFAULT_SPAN: f32 = 800.0;
const USER_DOT_RADIUS: f32 = 7.0;
/// "You are here" blue, as in GNOME Maps.
const USER_COLOR: Color = Color::from_rgb(0.21, 0.52, 0.89);

/// Zoom used to show a place picked from search, high enough for POI markers.
const PLACE_ZOOM: f64 = 16.0;

/// How many levels up to look for a loaded tile to show while one is missing.
const MAX_FALLBACK_LEVELS: u8 = 6;

/// Width of the chosen route, of its outline on each side, and of the others.
const ROUTE_WIDTH: f32 = 6.0;
const ROUTE_OUTLINE: f32 = 2.0;
const FAINT_ROUTE_WIDTH: f32 = 4.0;
/// Space kept around a route when framing it.
const ROUTE_PADDING: f32 = 48.0;
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
    ClosePanel,
    /// Center the map on the user's position.
    Locate,
    /// A link in the place panel, such as a website or a `tel:` URI.
    OpenUrl(String),
    /// The place panel's Directions button.
    Directions,
}

/// Work the map needs the app to do, returned from [`MapView::on_event`].
#[derive(Debug, Clone)]
pub enum MapAction {
    /// Fetch details with [`details::fetch`] and pass them to
    /// [`MapView::insert_details`].
    FetchDetails(OsmId),
    /// Find what is at a place with [`details::reverse`] and pass it to
    /// [`MapView::insert_reverse`].
    Reverse {
        request: u64,
        lat: f64,
        lon: f64,
        zoom: u8,
    },
    OpenUrl(String),
    /// Show the directions form to the selected place.
    OpenDirections,
}

/// A click that hit no POI, waiting for [`details::reverse`] to say what is there.
#[derive(Clone, Copy)]
struct Lookup {
    request: u64,
    /// Where the click was, in Web Mercator coordinates spanning 0..1.
    position: (f64, f64),
}

/// Extra information about a POI, by OpenStreetMap object.
enum DetailsState {
    Loading,
    Loaded(PoiDetails),
    /// Nominatim does not know the object; not asked again.
    Unavailable,
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
    /// Directions results, while shown.
    routes: Option<RouteOverlay>,
}

impl MapView {
    /// Creates the map. The stream yields each tile when it is rendered; feed them
    /// back through [`MapView::insert_tile`].
    pub fn new(palette: MapPalette) -> (Self, impl Stream<Item = RenderedTile> + Send + 'static) {
        let (lat, lon) = INITIAL_CENTER;
        let (state, tiles) = MapState::new(palette);

        let map = Self {
            state,
            camera: Camera::from_lat_lon(lat, lon, INITIAL_ZOOM),
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
                let anchor = anchor.unwrap_or(Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0));
                self.camera.zoom_at(delta, anchor, self.viewport);
            }
            MapEvent::Resized(size) => self.viewport = size,
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
            MapEvent::OpenUrl(url) => action = Some(MapAction::OpenUrl(url)),
            MapEvent::Directions => action = Some(MapAction::OpenDirections),
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
    pub fn center(&self) -> (f64, f64) {
        self.camera.center_lat_lon()
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

    fn close_panel(&mut self) {
        self.selected = None;
        self.lookup = None;
    }

    /// Starts finding what is at `point`, which hit no POI.
    fn look_up(&mut self, point: Point) -> MapAction {
        let position = self.camera.to_world(point, self.viewport);
        let (lat, lon) = lat_lon(position);
        // The panel shows the lookup, not the previous place, until it answers.
        self.selected = None;
        self.next_lookup += 1;
        self.lookup = Some(Lookup {
            request: self.next_lookup,
            position,
        });

        MapAction::Reverse {
            request: self.next_lookup,
            lat,
            lon,
            zoom: lookup_detail(self.camera.zoom()),
        }
    }

    /// Selects what a lookup found, if it is still the latest one.
    pub fn insert_reverse(
        &mut self,
        request: u64,
        result: Result<Option<(Poi, PoiDetails)>, details::Error>,
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
            Err(err) => eprintln!("failed to look up what is at the click: {err:?}"),
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
                closest_poi(self.pois_in_view(self.viewport, u32::MAX), point, HIDDEN_POI_HIT_RADIUS)
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
            .filter(|id| self.tiles.get(id).is_none_or(|cached| cached.is_stale(&self.palette)))
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
        let zoom = |delta| on_event(MapEvent::Zoom { delta, anchor: None });

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
        let zoom = zoom_showing(user.accuracy, user.latitude, span).min(LOCATED_MAX_ZOOM);

        self.camera = Camera::centered_on(world_position(user.latitude, user.longitude), zoom);
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

    /// Whether a place is selected or being looked up, for the place panel.
    pub fn set_routes(&mut self, routes: Option<RouteOverlay>) {
        self.routes = routes;
    }

    /// Frames every route option, beside the drawer; the map stops following the user.
    pub fn fit_route(&mut self) {
        let Some(routes) = &self.routes else {
            return;
        };
        let Some((min, max)) = bounds(routes.itineraries.iter().flat_map(route_points)) else {
            return;
        };
        let viewport = if self.viewport.width > 0.0 {
            self.viewport
        } else {
            Size::new(DEFAULT_SPAN, DEFAULT_SPAN)
        };

        self.following_user = false;
        self.camera = Camera::fitting(min, max, viewport, ROUTE_PADDING);
        self.request_tiles();
    }

    pub fn has_place(&self) -> bool {
        self.selected.is_some() || self.lookup.is_some()
    }

    /// What the place panel shows: the selected place, a lookup in progress, or
    /// `None` when nothing is selected. The panel provides the close button and
    /// the scrolling.
    pub fn place_panel<'a, Message: Clone + 'static>(
        &'a self,
        on_event: impl Fn(MapEvent) -> Message + 'a,
    ) -> Option<Element<'a, Message>> {
        match (&self.selected, self.lookup) {
            (Some(poi), _) => {
                let details = poi.osm_id.and_then(|osm_id| self.details.get(&osm_id));
                Some(place_panel(poi, details, &on_event))
            }
            (None, Some(_)) => Some(widget::text::caption(fl!("poi-looking-up")).into()),
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
fn closest_poi<'a>(pois: Vec<(&'a Poi, Point)>, point: Point, radius: f32) -> Option<&'a Poi> {
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

fn place_panel<'a, Message: Clone + 'static>(
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

    let directions = widget::button::suggested(fl!("directions"))
        .on_press(on_event(MapEvent::Directions));

    let details: Option<Element<'a, Message>> = match details {
        Some(DetailsState::Loading) => {
            Some(widget::text::caption(fl!("poi-details-loading")).into())
        }
        Some(DetailsState::Loaded(details)) => details_rows(details, on_event),
        Some(DetailsState::Unavailable) | None => None,
    };

    widget::column::with_capacity(4)
        .push(names)
        .push(directions)
        .push_maybe(details.is_some().then(widget::divider::horizontal::default))
        .push_maybe(details)
        .spacing(spacing.space_xs)
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

/// POI markers and the selected POI, drawn above the tiles. It does not handle
/// events, so they reach `MapCanvas` below.
struct MarkerCanvas<'a> {
    map: &'a MapView,
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

/// Every point of a route: its paths and its stops.
fn route_points(itinerary: &Itinerary) -> impl Iterator<Item = (f64, f64)> + '_ {
    itinerary.legs.iter().flat_map(|leg| {
        leg.path
            .iter()
            .copied()
            .chain([leg.from.position, leg.to.position])
            .chain(leg.stops.iter().map(|stop| stop.position))
    })
}

/// The smallest box holding all `points`, or `None` if there are none.
fn bounds(points: impl Iterator<Item = (f64, f64)>) -> Option<((f64, f64), (f64, f64))> {
    points.fold(None, |bounds, (x, y)| {
        let ((min_x, min_y), (max_x, max_y)) = bounds.unwrap_or(((x, y), (x, y)));
        Some(((min_x.min(x), min_y.min(y)), (max_x.max(x), max_y.max(y))))
    })
}

/// A leg's path on screen, or a straight line between its ends without one.
fn leg_path(leg: &Leg, camera: &Camera, viewport: Size) -> Path {
    let fallback = [leg.from.position, leg.to.position];
    let points = if leg.path.is_empty() { &fallback[..] } else { &leg.path[..] };

    Path::new(|builder| {
        let mut points = points.iter().map(|point| camera.to_screen(*point, viewport));
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
/// parts dotted and its transit legs in their lines' colours.
fn draw_routes(
    frame: &mut Frame,
    camera: &Camera,
    routes: &RouteOverlay,
    viewport: Size,
    palette: &MapPalette,
    accent: Color,
) {
    let faint = Color { a: 0.45, ..palette.poi_marker };
    for (index, itinerary) in routes.itineraries.iter().enumerate() {
        if index != routes.chosen {
            for leg in &itinerary.legs {
                frame.stroke(&leg_path(leg, camera, viewport), round_stroke(faint, FAINT_ROUTE_WIDTH));
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
            frame.stroke(&path, canvas::Stroke { line_dash: dots, ..outline });
            frame.stroke(&path, canvas::Stroke { line_dash: dots, ..round_stroke(accent, ROUTE_WIDTH) });
        } else {
            let color = leg.line.as_ref().and_then(|line| line.color).unwrap_or(accent);
            frame.stroke(&path, outline);
            frame.stroke(&path, round_stroke(color, ROUTE_WIDTH));
        }
    }
}

/// Stops of the chosen route: those passed, where to board and get off, and the
/// trip's start (hollow) and destination (filled).
fn draw_route_stops(
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
        frame.stroke(&circle, canvas::Stroke::default().with_color(ring).with_width(2.0));
    };

    for leg in itinerary.legs.iter().filter(|leg| !leg.is_walk()) {
        let color = leg.line.as_ref().and_then(|line| line.color).unwrap_or(accent);
        for stop in &leg.stops {
            dot(frame, stop.position, ROUTE_STOP_RADIUS, palette.land, color);
        }
        for end in [leg.from.position, leg.to.position] {
            dot(frame, end, ROUTE_BOARD_RADIUS, Color::WHITE, color);
        }
    }

    if let (Some(first), Some(last)) = (itinerary.legs.first(), itinerary.legs.last()) {
        dot(frame, first.from.position, ROUTE_END_RADIUS, palette.land, accent);
        dot(frame, last.to.position, ROUTE_END_RADIUS, accent, Color::WHITE);
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
