// SPDX-License-Identifier: MPL-2.0

use crate::config::Config;
use crate::directions::{self, Directions, RouteRequest};
use crate::fl;
use crate::map::location::{self, Position};
use crate::map::style::MapPalette;
use crate::map::poi::{OsmId, Poi};
use crate::map::{MapAction, MapEvent, MapView, RenderedTile};
use crate::place_view;
use crate::route_view;
use crate::services::nominatim::{self, PoiDetails};
use crate::services::photon;
use crate::services::transitous;
use crate::services;
use crate::search_box::{self, Query, SEARCH_DELAY, SearchBox};
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::StreamExt;
use cosmic::iced::{Length, Subscription, event, keyboard};
use cosmic::prelude::*;
use cosmic::widget::{self, about::About, icon, menu, popover};
use std::collections::HashMap;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");
/// Distance the map moves per arrow key press, in pixels.
const PAN_STEP: f64 = 128.0;
/// Width of the search field, as in other COSMIC apps.
const SEARCH_INPUT_WIDTH: f32 = 240.0;
const SEARCH_RESULTS_WIDTH: f32 = 360.0;

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct AppModel {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    /// Display a context drawer with the designated page if defined.
    context_page: ContextPage,
    /// The about page for this app.
    about: About,

    map: MapView,
    /// Key bindings for the application's menu bar.
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    /// Configuration data that persists between application runs.
    config: Config,
    /// Search field in the header bar, shown with its button or Ctrl+F.
    search_id: widget::Id,
    search_active: bool,
    search: SearchBox,
    /// The directions form, while shown in the context drawer.
    directions: Option<Directions>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    Map(MapEvent),
    TileReady(RenderedTile),
    PoiDetails(OsmId, Result<PoiDetails, services::Error>),
    /// The user's position, or why it cannot be known.
    Location(Result<Position, String>),
    /// What a click that hit no POI found, for the lookup with this number.
    Reverse(u64, Result<Option<(Poi, PoiDetails)>, services::Error>),
    SearchActivate,
    SearchClear,
    /// The header search field.
    Search(search_box::Message),
    /// The place panel's Directions button.
    OpenDirections,
    DirectionsBack,
    Directions(directions::Message),
    Arrow(Arrow),
}

#[derive(Debug, Clone, Copy)]
pub enum Arrow {
    Left,
    Right,
    Up,
    Down,
}

/// Create a COSMIC application from the app model
impl cosmic::Application for AppModel {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.ercalvez.cosmic-maps";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        mut core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Lay the context drawer out beside the map rather than over it. As an
        // overlay, it covers the whole map area and the map stops receiving clicks.
        core.window.context_is_overlay = false;

        // Create the about widget
        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .links([(fl!("repository"), REPOSITORY)])
            .license(env!("CARGO_PKG_LICENSE"));

        let (map, tiles) = MapView::new(MapPalette::from_theme(cosmic::theme::active().cosmic()));

        // Construct the app model with the runtime's core.
        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            about,
            key_binds: HashMap::new(),
            // Optional configuration file for an application.
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((errors, config)) => {
                        for why in errors {
                            tracing::error!(%why, "error loading app config");
                        }
                        config
                    }
                })
                .unwrap_or_default(),
            map,
            search_id: widget::Id::unique(),
            search_active: false,
            search: SearchBox::default(),
            directions: None,
        };

        // Set the window title, and turn each rendered tile into a message for as
        // long as the map worker runs.
        let command = Task::batch([
            app.update_title(),
            cosmic::task::stream(tiles.map(Message::TileReady)),
        ]);

        (app, command)
    }

    /// Elements to pack at the start of the header bar.
    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            menu::root(fl!("view")).apply(Element::from),
            menu::items(
                &self.key_binds,
                vec![menu::Item::Button(fl!("about"), None, MenuAction::About)],
            ),
        )]);

        vec![menu_bar.into()]
    }

    /// Search, at the end of the header bar as in COSMIC Files: a button that turns
    /// into a search field.
    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        let search = if self.search_active {
            let input =
                widget::text_input::search_input(fl!("search-placeholder"), &self.search.input)
                    .width(Length::Fixed(SEARCH_INPUT_WIDTH))
                    .id(self.search_id.clone())
                    .on_clear(Message::SearchClear)
                    .on_input(|text| Message::Search(search_box::Message::Input(text)))
                    .on_submit(|_| Message::Search(search_box::Message::Submit));

            // Results drop down under the field, kept inside the window. No
            // `on_close`: it also fires for presses on the results themselves.
            let mut search = popover(input);
            if !self.search.results.is_empty() {
                search = search
                    .popup(self.search_results_view())
                    .position(popover::Position::Bottom);
            }
            search.into()
        } else {
            widget::button::icon(icon::from_name("system-search-symbolic"))
                .padding(8)
                .on_press(Message::SearchActivate)
                .into()
        };

        vec![search]
    }

    /// The system theme changed, including between light and dark mode. Changes
    /// that leave the map colours as they are, such as a new accent, re-render nothing.
    fn system_theme_update(
        &mut self,
        _keys: &[&'static str],
        new_theme: &cosmic::cosmic_theme::Theme,
    ) -> Task<cosmic::Action<Self::Message>> {
        self.map.set_palette(MapPalette::from_theme(new_theme));
        Task::none()
    }

    /// Ctrl+F, handled by libcosmic.
    fn on_search(&mut self) -> Task<cosmic::Action<Self::Message>> {
        self.update(Message::SearchActivate)
    }

    /// Esc closes the search first, then the place panel.
    fn on_escape(&mut self) -> Task<cosmic::Action<Self::Message>> {
        if self.search_active {
            self.update(Message::SearchClear)
        } else {
            self.update(Message::Map(MapEvent::ClosePanel))
        }
    }

    /// Display a context drawer if the context page is requested.
    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        Some(match self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_string()),
                Message::ToggleContextPage(ContextPage::About),
            ),
            ContextPage::Place => context_drawer::context_drawer(
                place_view::view(self.map.place()?),
                Message::Map(MapEvent::ClosePanel),
            ),
            ContextPage::Directions => {
                let back = widget::tooltip(
                    widget::button::icon(icon::from_name("go-previous-symbolic"))
                        .on_press(Message::DirectionsBack),
                    widget::text::body(fl!("directions-back")),
                    widget::tooltip::Position::Bottom,
                );
                // The form stays in the header, above the scrolling results.
                let directions = self.directions.as_ref()?;
                context_drawer::context_drawer(
                    directions.results_view().map(Message::Directions),
                    Message::Map(MapEvent::ClosePanel),
                )
                .title(fl!("directions"))
                .actions(back)
                .header(directions.form_view().map(Message::Directions))
                .footer(route_view::attribution(Message::LaunchUrl))
            }
        })
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        // The map fills the window; search results drop down from the header.
        self.map.view(Message::Map)
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They can be dynamically
    /// stopped and started conditionally based on application state, or persist
    /// indefinitely.
    fn subscription(&self) -> Subscription<Self::Message> {
        // Add subscriptions which are always active.
        let mut subscriptions = vec![
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    for why in update.errors {
                        tracing::error!(?why, "app config error");
                    }
                    Message::UpdateConfig(update.config)
                }),
        ];

        // The user's position, for as long as the app runs.
        subscriptions.push(Subscription::run_with(Self::APP_ID, |app_id| {
            location::updates(app_id).map(Message::Location)
        }));

        subscriptions.push(event::listen_with(|event, status, _window| {
            // Leave keys alone when a widget, such as a text input, used them.
            if status == event::Status::Captured {
                return None;
            }

            let event::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event else {
                return None;
            };

            // Esc goes through `on_escape`.
            let zoom = |delta| Some(Message::Map(MapEvent::Zoom { delta, anchor: None }));
            match key.as_ref() {
                keyboard::Key::Character("+" | "=") => zoom(1.0),
                keyboard::Key::Character("-") => zoom(-1.0),
                keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => Some(Message::Arrow(Arrow::Left)),
                keyboard::Key::Named(keyboard::key::Named::ArrowRight) => Some(Message::Arrow(Arrow::Right)),
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(Message::Arrow(Arrow::Up)),
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Some(Message::Arrow(Arrow::Down)),
                _ => None,
            }
        }));

        Subscription::batch(subscriptions)
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::Map(event) => {
                // Clicking the map hides search results, keeping the queries.
                if matches!(event, MapEvent::Click(_)) {
                    self.search.hide_results();
                    if let Some(directions) = &mut self.directions {
                        directions.hide_results();
                    }
                }
                // Closing the drawer also closes the directions form.
                if matches!(event, MapEvent::ClosePanel) {
                    self.directions = None;
                    self.show_routes(false);
                }
                let changes_place = matches!(event, MapEvent::Click(_) | MapEvent::ClosePanel);
                let action = self.map.on_event(event);
                let route = if changes_place {
                    self.sync_place_drawer()
                } else {
                    Task::none()
                };
                return Task::batch([route, self.run_map_action(action)]);
            }

            Message::SearchActivate => {
                self.search_active = true;
                return widget::text_input::focus(self.search_id.clone());
            }

            Message::SearchClear => {
                self.search_active = false;
                self.search.clear();
            }

            Message::Search(message) => match self.search.update(message) {
                Some(search_box::Action::Query(query)) => {
                    return self.search_places(query, Message::Search);
                }
                Some(search_box::Action::Pick(result)) => {
                    let action = self.map.show_place(result.poi);
                    let route = self.sync_place_drawer();
                    return Task::batch([route, self.run_map_action(action)]);
                }
                None => {}
            },

            Message::OpenDirections => {
                if let Some(place) = self.map.selected() {
                    let mut directions = Directions::new(place, self.map.user_position());
                    let actions = directions.refresh();
                    self.directions = Some(directions);
                    self.context_page = ContextPage::Directions;
                    self.core.window.show_context = true;
                    return self.run_directions(actions);
                }
            }

            Message::Directions(message) => {
                if let Some(directions) = &mut self.directions {
                    let actions = directions.update(message);
                    return self.run_directions(actions);
                }
            }

            Message::DirectionsBack => {
                self.directions = None;
                self.show_routes(false);
                self.context_page = ContextPage::Place;
                self.core.window.show_context = self.map.has_place();
            }

            Message::Arrow(arrow) => {
                // Up and down move through the search results shown, if any.
                if let Arrow::Up | Arrow::Down = arrow {
                    let down = matches!(arrow, Arrow::Down);
                    let header = self.search_active && self.search.move_highlight(down);
                    let directions = !header
                        && self
                            .directions
                            .as_mut()
                            .is_some_and(|directions| directions.move_highlight(down));
                    if header || directions {
                        return Task::none();
                    }
                }

                // Arrow keys move the view, so the content moves the other way.
                let (dx, dy) = match arrow {
                    Arrow::Left => (PAN_STEP, 0.0),
                    Arrow::Right => (-PAN_STEP, 0.0),
                    Arrow::Up => (0.0, PAN_STEP),
                    Arrow::Down => (0.0, -PAN_STEP),
                };
                return self.update(Message::Map(MapEvent::Pan { dx, dy }));
            }

            Message::PoiDetails(osm_id, result) => {
                self.map.insert_details(osm_id, result);
            }

            Message::Location(Ok(position)) => {
                self.map.set_position(position);
                if let Some(directions) = &mut self.directions {
                    let actions = directions.set_user_position(position);
                    return self.run_directions(actions);
                }
            }

            Message::Location(Err(err)) => {
                tracing::warn!("location unavailable: {err}");
            }

            Message::Reverse(request, result) => {
                self.map.insert_reverse(request, result);
                return self.sync_place_drawer();
            }

            Message::TileReady(tile) => {
                self.map.insert_tile(tile);
            }

            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    // Close the context drawer if the toggled context page is the same.
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    // Open the context drawer to display the requested context page.
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }

            Message::UpdateConfig(config) => {
                self.config = config;
            }

            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    tracing::error!("failed to open {url:?}: {err}");
                }
            },
        }
        Task::none()
    }
}

impl AppModel {
    /// Shows the place drawer while a place is selected, and hides it otherwise.
    /// Called when the selection may have changed, so that panning or the About
    /// drawer do not bring it back. While the directions form is open, a newly
    /// selected place becomes its destination instead, and routes are searched.
    fn sync_place_drawer(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(directions) = &mut self.directions {
            let actions = match self.map.selected() {
                Some(place) => directions.set_destination(place.clone()),
                None => Vec::new(),
            };
            return self.run_directions(actions);
        }

        if self.map.has_place() {
            self.context_page = ContextPage::Place;
            self.core.window.show_context = true;
        } else if self.context_page == ContextPage::Place {
            self.core.window.show_context = false;
        }
        Task::none()
    }

    /// Carries out work the directions asked for.
    fn run_directions(&mut self, actions: Vec<directions::Action>) -> Task<cosmic::Action<Message>> {
        let mut tasks = Vec::with_capacity(actions.len());
        for action in actions {
            match action {
                directions::Action::Search(field, query) => {
                    tasks.push(self.search_places(query, move |message| {
                        Message::Directions(directions::Message::Field(field, message))
                    }));
                }
                directions::Action::FindRoute(request) => tasks.push(Self::find_route(request)),
                directions::Action::ShowRoutes { fit } => self.show_routes(fit),
            }
        }
        Task::batch(tasks)
    }

    /// Draws the directions' routes on the map, or clears them, framing the
    /// options when `fit` is set.
    fn show_routes(&mut self, fit: bool) {
        let routes = self.directions.as_ref().and_then(Directions::route_overlay);
        let found = routes.is_some();
        self.map.set_routes(routes);
        if fit && found {
            self.map.fit_route();
        }
    }

    fn find_route(request: RouteRequest) -> Task<cosmic::Action<Message>> {
        cosmic::task::future(async move {
            let result = transitous::plan(request.from, request.to, request.mode, request.page).await;
            Message::Directions(directions::Message::RouteFound(request.id, result))
        })
    }

    /// Carries out work the map asked for.
    fn run_map_action(&self, action: Option<MapAction>) -> Task<cosmic::Action<Message>> {
        match action {
            Some(MapAction::FetchDetails(osm_id)) => cosmic::task::future(async move {
                Message::PoiDetails(osm_id, nominatim::fetch(osm_id).await)
            }),
            Some(MapAction::Reverse {
                request,
                lat,
                lon,
                zoom,
            }) => cosmic::task::future(async move {
                Message::Reverse(request, nominatim::reverse(lat, lon, zoom).await)
            }),
            None => Task::none(),
        }
    }

    /// Runs a search field's place search, preferring places near the center of
    /// the map, and sends the answer back through `wrap`.
    fn search_places(
        &self,
        query: Query,
        wrap: impl Fn(search_box::Message) -> Message + Send + 'static,
    ) -> Task<cosmic::Action<Message>> {
        match query {
            Query::Later(text) => cosmic::task::future(async move {
                tokio::time::sleep(SEARCH_DELAY).await;
                wrap(search_box::Message::Debounced(text))
            }),
            Query::Now(text) => {
                let near = self.map.center();
                cosmic::task::future(async move {
                    let result = photon::search(text.clone(), near).await;
                    wrap(search_box::Message::Results(text, result))
                })
            }
        }
    }

    /// Header search results, shown under the search field.
    fn search_results_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        widget::container(
            self.search
                .results_view(|index| Message::Search(search_box::Message::Select(index))),
        )
        .padding(spacing.space_xxs)
        .width(Length::Fixed(SEARCH_RESULTS_WIDTH))
        .class(cosmic::theme::Container::Dropdown)
        .apply(widget::container)
        // A small gap, so the list does not touch the field.
        .padding([spacing.space_xxs, 0, 0, 0])
        .into()
    }

    /// Updates the header and window titles.
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let window_title = fl!("app-title");

        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(window_title, id)
        } else {
            Task::none()
        }
    }
}

/// The context page to display in the context drawer.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
    /// The place selected on the map.
    Place,
    /// The directions form, to the selected place.
    Directions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
        }
    }
}
