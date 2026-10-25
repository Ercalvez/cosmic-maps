// SPDX-License-Identifier: MPL-2.0

//! The directions form: transport mode, start and destination, and the routes
//! found between them.

use std::collections::HashSet;

use cosmic::iced::Length;
use cosmic::prelude::*;
use cosmic::widget::{self, icon, segmented_button};

use crate::app::Message;
use crate::fl;
use crate::map::camera::lat_lon;
use crate::map::location::Position;
use crate::map::poi::Poi;
use crate::map::RouteOverlay;
use crate::map::routing::{self, Plan, RouteMode};
use crate::route_view::{self, Ends, RouteChoice};
use crate::search_box::{SearchBox, SearchTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelMode {
    Foot,
    Bike,
    Transit,
    Car,
}

impl TravelMode {
    const ALL: [Self; 4] = [Self::Foot, Self::Bike, Self::Transit, Self::Car];

    fn label(self) -> String {
        match self {
            Self::Foot => fl!("travel-mode-foot"),
            Self::Bike => fl!("travel-mode-bike"),
            Self::Transit => fl!("travel-mode-transit"),
            Self::Car => fl!("travel-mode-car"),
        }
    }

    fn icon(self) -> icon::Icon {
        let svg: &'static [u8] = match self {
            Self::Foot => include_bytes!("../resources/icons/transport/walking-symbolic.svg"),
            Self::Bike => include_bytes!("../resources/icons/transport/cycling-symbolic.svg"),
            Self::Transit => include_bytes!("../resources/icons/transport/transit-symbolic.svg"),
            Self::Car => include_bytes!("../resources/icons/transport/driving-symbolic.svg"),
        };
        let mut handle = icon::from_svg_bytes(svg);
        // Drawn in one colour, so the theme can recolour it like its own icons.
        handle.symbolic = true;
        handle.icon()
    }

    /// How routes are found for this mode, or `None` when not supported yet.
    fn route_mode(self) -> Option<RouteMode> {
        match self {
            Self::Foot => Some(RouteMode::Walk),
            Self::Transit => Some(RouteMode::Transit),
            Self::Bike | Self::Car => None,
        }
    }
}

/// One end of the trip.
#[derive(Debug, Clone)]
pub enum Endpoint {
    Unset,
    UserLocation(Position),
    Place(Poi),
}

impl Endpoint {
    fn lat_lon(&self) -> Option<(f64, f64)> {
        match self {
            Self::Unset => None,
            Self::UserLocation(position) => Some((position.latitude, position.longitude)),
            Self::Place(poi) => Some(lat_lon(poi.position)),
        }
    }
}

/// What is known about routes for the current form.
#[derive(Debug, Clone)]
pub enum RouteState {
    /// The start or destination is missing.
    Idle,
    /// The selected mode cannot be routed yet.
    Unavailable,
    Loading,
    Found(Plan),
    NotFound,
    Failed,
}

/// The trip a route request is for.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RouteQuery {
    from: (f64, f64),
    to: (f64, f64),
    mode: RouteMode,
}

/// A route search to run with [`routing::plan`], then pass to
/// [`Directions::insert_route`].
#[derive(Debug, Clone)]
pub struct RouteRequest {
    pub id: u64,
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: RouteMode,
    /// Earlier or later options, as a cursor from a previous plan.
    pub page: Option<String>,
}

pub struct Directions {
    modes: segmented_button::SingleSelectModel,
    from: Endpoint,
    to: Endpoint,
    /// The user changed the start, so a position arriving must not replace it.
    from_edited: bool,
    from_search: SearchBox,
    to_search: SearchBox,
    route: RouteState,
    /// The trip shown or being searched, so an unchanged form is not searched again.
    query: Option<RouteQuery>,
    /// Number of the latest request: answers to older ones are stale.
    latest_request: u64,
    /// The route option whose details are shown.
    chosen: Option<RouteChoice>,
    /// Legs of the chosen option whose stops are listed.
    open_stops: HashSet<usize>,
}

impl Directions {
    /// From the user's position, if known, to `destination`.
    pub fn new(destination: &Poi, user: Option<Position>) -> Self {
        let mut modes = segmented_button::SingleSelectModel::default();
        for mode in TravelMode::ALL {
            let entity = modes.insert().text(mode.label()).icon(mode.icon()).data(mode);
            if mode == TravelMode::Foot {
                entity.activate();
            }
        }

        let mut directions = Self {
            modes,
            from: Endpoint::Unset,
            to: Endpoint::Unset,
            from_edited: false,
            from_search: SearchBox::default(),
            to_search: SearchBox::default(),
            route: RouteState::Idle,
            query: None,
            latest_request: 0,
            chosen: None,
            open_stops: HashSet::new(),
        };
        if let Some(position) = user {
            directions.set(SearchTarget::From, Endpoint::UserLocation(position));
        }
        directions.set_destination(destination.clone());
        directions
    }

    pub fn select_mode(&mut self, mode: segmented_button::Entity) {
        self.modes.activate(mode);
    }

    fn mode(&self) -> TravelMode {
        self.modes
            .active_data::<TravelMode>()
            .copied()
            .unwrap_or(TravelMode::Foot)
    }

    /// The user's position became known. It becomes the start unless the user
    /// already chose or cleared one. Returns whether the start changed this way.
    pub fn user_position_known(&mut self, position: Position) -> bool {
        match self.from {
            Endpoint::Unset if !self.from_edited => {
                self.set(SearchTarget::From, Endpoint::UserLocation(position));
                true
            }
            // Keep the latest position, without searching again for every move.
            Endpoint::UserLocation(_) => {
                self.from = Endpoint::UserLocation(position);
                false
            }
            _ => false,
        }
    }

    pub fn set_destination(&mut self, place: Poi) {
        self.set(SearchTarget::To, Endpoint::Place(place));
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.from, &mut self.to);
        std::mem::swap(&mut self.from_search, &mut self.to_search);
        self.from_edited = true;
    }

    /// The search box of a field, or `None` for the header search.
    pub fn search_mut(&mut self, target: SearchTarget) -> Option<&mut SearchBox> {
        match target {
            SearchTarget::Header => None,
            SearchTarget::From => Some(&mut self.from_search),
            SearchTarget::To => Some(&mut self.to_search),
        }
    }

    /// The field whose results are shown, if any.
    pub fn searching_mut(&mut self) -> Option<&mut SearchBox> {
        [&mut self.from_search, &mut self.to_search]
            .into_iter()
            .find(|search| !search.results.is_empty())
    }

    /// Text typed in a field: its place is no longer set until one is picked.
    /// Returns the query to search after a pause, if any.
    pub fn on_input(&mut self, target: SearchTarget, text: String) -> Option<String> {
        match target {
            SearchTarget::Header => return None,
            SearchTarget::From => {
                self.from = Endpoint::Unset;
                self.from_edited = true;
            }
            SearchTarget::To => self.to = Endpoint::Unset,
        }
        self.search_mut(target)?.on_input(text)
    }

    /// A search result was picked for a field.
    pub fn pick(&mut self, target: SearchTarget, place: Poi) {
        if target == SearchTarget::From {
            self.from_edited = true;
        }
        self.set(target, Endpoint::Place(place));
    }

    pub fn hide_results(&mut self) {
        self.from_search.hide_results();
        self.to_search.hide_results();
    }

    /// Updates the routes after a change to the form. Returns the search to run
    /// when the trip changed and can be routed.
    pub fn route_request(&mut self) -> Option<RouteRequest> {
        let Some(mode) = self.mode().route_mode() else {
            self.query = None;
            self.route = RouteState::Unavailable;
            return None;
        };
        let (Some(from), Some(to)) = (self.from.lat_lon(), self.to.lat_lon()) else {
            self.query = None;
            self.route = RouteState::Idle;
            return None;
        };

        let query = RouteQuery { from, to, mode };
        if self.query == Some(query) {
            return None;
        }
        self.query = Some(query);
        Some(self.start(query, None))
    }

    /// The search for transit options leaving earlier or later than those shown.
    pub fn page_request(&mut self, later: bool) -> Option<RouteRequest> {
        let RouteState::Found(plan) = &self.route else {
            return None;
        };
        let cursor = if later { plan.later.clone() } else { plan.earlier.clone() }?;
        let query = self.query?;
        Some(self.start(query, Some(cursor)))
    }

    /// Stores the answer to a search, unless a newer one was started since.
    pub fn insert_route(&mut self, id: u64, result: Result<Plan, routing::Error>) {
        if id != self.latest_request {
            return;
        }
        let Some(query) = self.query else {
            return;
        };
        self.chosen = None;
        self.open_stops.clear();
        self.route = match result {
            Ok(plan) if plan.is_empty(query.mode) => RouteState::NotFound,
            Ok(plan) => RouteState::Found(plan),
            Err(err) => {
                eprintln!("failed to find routes: {err:?}");
                RouteState::Failed
            }
        };
    }

    /// Shows the details of a route option, or hides them if already shown.
    /// The routes to draw on the map, in the order listed: the walking route
    /// first, then transit options. The chosen one is highlighted; before any
    /// choice, the first transit option, or the walking route in Walk mode.
    pub fn route_overlay(&self) -> Option<RouteOverlay> {
        let RouteState::Found(plan) = &self.route else {
            return None;
        };
        let mode = self.query?.mode;

        let mut itineraries: Vec<_> = plan.walk.iter().cloned().collect();
        if mode == RouteMode::Transit {
            itineraries.extend(plan.transit.iter().cloned());
        }
        let walks = usize::from(plan.walk.is_some());
        let chosen = match self.chosen {
            Some(RouteChoice::Walk) => 0,
            Some(RouteChoice::Transit(index)) => walks + index,
            None if mode == RouteMode::Transit && !plan.transit.is_empty() => walks,
            None => 0,
        };

        Some(RouteOverlay {
            itineraries,
            chosen,
        })
    }

    pub fn choose(&mut self, choice: RouteChoice) {
        self.chosen = (self.chosen != Some(choice)).then_some(choice);
        self.open_stops.clear();
    }

    /// Lists the stops of a leg of the chosen option, or hides them.
    pub fn toggle_stops(&mut self, leg: usize) {
        if !self.open_stops.remove(&leg) {
            self.open_stops.insert(leg);
        }
    }

    fn start(&mut self, query: RouteQuery, page: Option<String>) -> RouteRequest {
        self.latest_request += 1;
        self.route = RouteState::Loading;
        RouteRequest {
            id: self.latest_request,
            from: query.from,
            to: query.to,
            mode: query.mode,
            page,
        }
    }

    /// Sets a field's place and shows its name in the field.
    fn set(&mut self, target: SearchTarget, endpoint: Endpoint) {
        let name = match &endpoint {
            Endpoint::Unset => String::new(),
            Endpoint::UserLocation(_) => fl!("your-location"),
            Endpoint::Place(poi) => poi.translated_name.clone().unwrap_or_else(|| poi.name.clone()),
        };
        match target {
            SearchTarget::Header => return,
            SearchTarget::From => self.from = endpoint,
            SearchTarget::To => self.to = endpoint,
        }
        if let Some(search) = self.search_mut(target) {
            search.set_text(name);
        }
    }

    /// The form, kept above the scrolling results: modes, start and destination,
    /// and earlier/later buttons for transit.
    pub fn form_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        let modes = widget::segmented_control::horizontal(&self.modes)
            .on_activate(Message::DirectionsMode);

        let swap = widget::tooltip(
            widget::button::icon(icon::from_name("object-flip-vertical-symbolic"))
                .on_press(Message::DirectionsSwap),
            widget::text::body(fl!("directions-swap")),
            widget::tooltip::Position::Bottom,
        );

        widget::column::with_capacity(5)
            .width(Length::Fill)
            .push(modes)
            .push(field(
                SearchTarget::From,
                fl!("directions-from"),
                fl!("directions-choose-start"),
                &self.from_search,
            ))
            .push(widget::row::with_capacity(1).push(widget::space::horizontal()).push(swap))
            .push(field(
                SearchTarget::To,
                fl!("directions-to"),
                fl!("directions-choose-destination"),
                &self.to_search,
            ))
            .push_maybe(self.pages_view())
            .spacing(spacing.space_s)
            .into()
    }

    /// Earlier and later transit options, when routes were found by transit.
    fn pages_view(&self) -> Option<Element<'_, Message>> {
        let RouteState::Found(plan) = &self.route else {
            return None;
        };
        let is_transit = self.query.is_some_and(|query| query.mode == RouteMode::Transit);
        if !is_transit || plan.transit.is_empty() {
            return None;
        }

        let pages = widget::row::with_capacity(3)
            .push(
                widget::button::text(fl!("route-earlier"))
                    .on_press_maybe(plan.earlier.is_some().then_some(Message::RoutePage(false))),
            )
            .push(widget::space::horizontal())
            .push(
                widget::button::text(fl!("route-later"))
                    .on_press_maybe(plan.later.is_some().then_some(Message::RoutePage(true))),
            );
        Some(pages.into())
    }

    /// The routes found, or the search's status: the part that scrolls.
    pub fn results_view(&self) -> Element<'_, Message> {
        let status = |text: String| widget::text::caption(text).into();
        let plan = match &self.route {
            RouteState::Idle => return widget::column::with_capacity(0).into(),
            RouteState::Unavailable => return status(fl!("route-unavailable")),
            RouteState::Loading => return status(fl!("route-loading")),
            RouteState::NotFound => return status(fl!("route-not-found")),
            RouteState::Failed => return status(fl!("route-failed")),
            RouteState::Found(plan) => plan,
        };

        let Some(query) = self.query else {
            return widget::column::with_capacity(0).into();
        };
        let ends = Ends {
            start: &self.from_search.input,
            destination: &self.to_search.input,
        };
        route_view::plan_view(plan, query.mode, self.chosen, &self.open_stops, ends)
    }
}

/// A labelled place field, with its search results under it while shown.
fn field<'a>(
    target: SearchTarget,
    label: String,
    hint: String,
    search: &'a SearchBox,
) -> Element<'a, Message> {
    let input = widget::text_input::text_input(hint, &search.input)
        .on_input(move |text| Message::SearchInput(target, text))
        .on_submit(move |_| Message::SearchSubmit(target));

    widget::column::with_capacity(3)
        .push(widget::text::caption_heading(label))
        .push(input)
        .push_maybe(
            (!search.results.is_empty())
                .then(|| search.results_view(move |index| Message::SearchSelect(target, index))),
        )
        .spacing(cosmic::theme::spacing().space_xxs)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(name: &str) -> Poi {
        Poi {
            name: name.to_owned(),
            translated_name: None,
            class: None,
            subclass: None,
            rank: 0,
            osm_id: None,
            position: (0.5, 0.5),
        }
    }

    /// Near the Musée du Louvre, Paris.
    fn here() -> Position {
        Position {
            latitude: 48.8606,
            longitude: 2.3376,
            accuracy: 50.0,
        }
    }

    fn select(directions: &mut Directions, mode: TravelMode) {
        let entity = directions
            .modes
            .iter()
            .find(|&entity| directions.modes.data::<TravelMode>(entity) == Some(&mode))
            .unwrap();
        directions.select_mode(entity);
    }

    #[test]
    fn defaults_with_and_without_location() {
        let directions = Directions::new(&place("Hôtel de Ville"), Some(here()));
        assert!(matches!(directions.from, Endpoint::UserLocation(_)));
        assert_eq!(directions.from_search.input, fl!("your-location"));
        assert!(matches!(&directions.to, Endpoint::Place(poi) if poi.name == "Hôtel de Ville"));
        assert_eq!(directions.to_search.input, "Hôtel de Ville");

        let directions = Directions::new(&place("Hôtel de Ville"), None);
        assert!(matches!(directions.from, Endpoint::Unset));
        assert!(directions.from_search.input.is_empty());
    }

    #[test]
    fn location_fills_an_untouched_empty_start_only() {
        let mut directions = Directions::new(&place("Gare de Lyon"), None);
        assert!(directions.user_position_known(here()));
        assert!(matches!(directions.from, Endpoint::UserLocation(_)));
        // Later positions update the start without asking for a new search.
        assert!(!directions.user_position_known(here()));

        let mut directions = Directions::new(&place("Gare de Lyon"), None);
        directions.on_input(SearchTarget::From, "Châtelet".to_owned());
        directions.on_input(SearchTarget::From, String::new());
        assert!(!directions.user_position_known(here()));
        assert!(matches!(directions.from, Endpoint::Unset));
    }

    #[test]
    fn swap_exchanges_places_and_texts() {
        let mut directions = Directions::new(&place("Gare de Lyon"), Some(here()));
        directions.swap();
        assert!(matches!(&directions.from, Endpoint::Place(poi) if poi.name == "Gare de Lyon"));
        assert!(matches!(directions.to, Endpoint::UserLocation(_)));
        assert_eq!(directions.from_search.input, "Gare de Lyon");
        assert_eq!(directions.to_search.input, fl!("your-location"));
    }

    #[test]
    fn requests_follow_the_form() {
        let mut directions = Directions::new(&place("Hôtel de Ville"), None);
        // No start yet.
        assert!(directions.route_request().is_none());
        assert!(matches!(directions.route, RouteState::Idle));

        directions.user_position_known(here());
        let first = directions.route_request().unwrap();
        assert_eq!(first.mode, RouteMode::Walk);
        assert!(matches!(directions.route, RouteState::Loading));
        // An unchanged form is not searched again.
        assert!(directions.route_request().is_none());

        select(&mut directions, TravelMode::Transit);
        let second = directions.route_request().unwrap();
        assert_eq!(second.mode, RouteMode::Transit);
        assert!(second.id > first.id);

        select(&mut directions, TravelMode::Bike);
        assert!(directions.route_request().is_none());
        assert!(matches!(directions.route, RouteState::Unavailable));
    }

    #[test]
    fn stop_lists_open_and_reset() {
        let mut directions = Directions::new(&place("Hôtel de Ville"), Some(here()));
        directions.choose(RouteChoice::Transit(0));
        directions.toggle_stops(1);
        assert!(directions.open_stops.contains(&1));
        directions.toggle_stops(1);
        assert!(!directions.open_stops.contains(&1));

        // Another option starts with its stop lists closed.
        directions.toggle_stops(1);
        directions.choose(RouteChoice::Transit(1));
        assert!(directions.open_stops.is_empty());
    }

    fn itinerary() -> routing::Itinerary {
        routing::Itinerary {
            duration: 600,
            start_time: jiff::Timestamp::UNIX_EPOCH,
            end_time: jiff::Timestamp::UNIX_EPOCH,
            transfers: 0,
            legs: Vec::new(),
        }
    }

    #[test]
    fn overlay_highlights_the_chosen_route() {
        let mut directions = Directions::new(&place("Gare de Lyon"), Some(here()));
        select(&mut directions, TravelMode::Transit);
        let request = directions.route_request().unwrap();
        let plan = Plan {
            walk: Some(itinerary()),
            transit: vec![itinerary(), itinerary()],
            ..Plan::default()
        };
        directions.insert_route(request.id, Ok(plan));

        // The walking route first, then the transit options; the first of those
        // is highlighted until one is chosen.
        let overlay = directions.route_overlay().unwrap();
        assert_eq!(overlay.itineraries.len(), 3);
        assert_eq!(overlay.chosen, 1);

        directions.choose(RouteChoice::Transit(1));
        assert_eq!(directions.route_overlay().unwrap().chosen, 2);
        directions.choose(RouteChoice::Walk);
        assert_eq!(directions.route_overlay().unwrap().chosen, 0);

        // Nothing to draw while searching again.
        select(&mut directions, TravelMode::Foot);
        directions.route_request();
        assert!(directions.route_overlay().is_none());
    }

    #[test]
    fn stale_answers_are_dropped() {
        let mut directions = Directions::new(&place("Hôtel de Ville"), Some(here()));
        let first = directions.route_request().unwrap();
        select(&mut directions, TravelMode::Transit);
        let second = directions.route_request().unwrap();

        directions.insert_route(first.id, Ok(Plan::default()));
        assert!(matches!(directions.route, RouteState::Loading));

        directions.insert_route(second.id, Ok(Plan::default()));
        assert!(matches!(directions.route, RouteState::NotFound));
    }
}
