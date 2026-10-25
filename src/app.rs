// SPDX-License-Identifier: MPL-2.0

use crate::config::Config;
use crate::fl;
use crate::map::details::{self, PoiDetails};
use crate::map::location::{self, Position};
use crate::map::poi::{OsmId, Poi};
use crate::map::search::{self, SearchResult};
use crate::map::{MapAction, MapEvent, MapView, RenderedTile};
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::futures::StreamExt;
use cosmic::iced::{Length, Subscription, event, keyboard};
use cosmic::prelude::*;
use cosmic::widget::{self, about::About, icon, menu};
use std::collections::HashMap;
use std::time::Duration;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");
/// Distance the map moves per arrow key press, in pixels.
const PAN_STEP: f64 = 128.0;
/// Pause after typing before searching, and the shortest query searched, as in
/// GNOME Maps.
const SEARCH_DELAY: Duration = Duration::from_millis(500);
const SEARCH_MIN_CHARS: usize = 3;
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
    search_input: String,
    search_results: Vec<SearchResult>,
    /// Result chosen with the arrow keys.
    search_highlight: Option<usize>,
    /// Enter was pressed before results arrived: pick the first one when they do.
    search_submitted: bool,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    Map(MapEvent),
    TileReady(RenderedTile),
    PoiDetails(OsmId, Result<PoiDetails, details::Error>),
    /// The user's position, or why it cannot be known.
    Location(Result<Position, String>),
    /// What a click that hit no POI found, for the lookup with this number.
    Reverse(u64, Result<Option<(Poi, PoiDetails)>, details::Error>),
    SearchActivate,
    SearchClear,
    SearchInput(String),
    /// Sent `SEARCH_DELAY` after typing; searches if the text is still the same.
    SearchDebounced(String),
    SearchSubmit,
    SearchResults(String, Result<Vec<SearchResult>, details::Error>),
    SearchSelect(usize),
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

        let (map, tiles) = MapView::new();

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
                    Err((_errors, config)) => {
                        // for why in errors {
                        //     tracing::error!(%why, "error loading app config");
                        // }

                        config
                    }
                })
                .unwrap_or_default(),
            map,
            search_id: widget::Id::unique(),
            search_active: false,
            search_input: String::new(),
            search_results: Vec::new(),
            search_highlight: None,
            search_submitted: false,
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
            widget::text_input::search_input(fl!("search-placeholder"), &self.search_input)
                .width(Length::Fixed(SEARCH_INPUT_WIDTH))
                .id(self.search_id.clone())
                .on_clear(Message::SearchClear)
                .on_input(Message::SearchInput)
                .on_submit(|_| Message::SearchSubmit)
                .into()
        } else {
            widget::button::icon(icon::from_name("system-search-symbolic"))
                .padding(8)
                .on_press(Message::SearchActivate)
                .into()
        };

        vec![search]
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
                self.map.place_panel(Message::Map)?,
                Message::Map(MapEvent::ClosePanel),
            ),
        })
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<'_, Self::Message> {
        // The map fills the window, with search results over it while shown.
        let map = self.map.view(Message::Map);
        if !self.search_active || self.search_results.is_empty() {
            return map;
        }
        cosmic::iced::widget::stack![map, self.search_results_view()].into()
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
                    // for why in update.errors {
                    //     tracing::error!(?why, "app config error");
                    // }

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
                let changes_place = matches!(event, MapEvent::Click(_) | MapEvent::ClosePanel);
                let action = self.map.on_event(event);
                if changes_place {
                    self.sync_place_drawer();
                }
                return self.run_map_action(action);
            }

            Message::SearchActivate => {
                self.search_active = true;
                return widget::text_input::focus(self.search_id.clone());
            }

            Message::SearchClear => {
                self.search_active = false;
                self.search_input.clear();
                self.search_results.clear();
                self.search_highlight = None;
                self.search_submitted = false;
            }

            Message::SearchInput(input) => {
                self.search_input = input.clone();
                self.search_highlight = None;
                if input.trim().chars().count() < SEARCH_MIN_CHARS {
                    self.search_results.clear();
                    self.search_submitted = false;
                    return Task::none();
                }
                return cosmic::task::future(async move {
                    tokio::time::sleep(SEARCH_DELAY).await;
                    Message::SearchDebounced(input)
                });
            }

            Message::SearchDebounced(input) => {
                // Typing continued: a later message will search.
                if input == self.search_input {
                    return self.search(input);
                }
            }

            Message::SearchSubmit => {
                let chosen = self
                    .search_highlight
                    .or((!self.search_results.is_empty()).then_some(0));
                if let Some(index) = chosen {
                    return self.update(Message::SearchSelect(index));
                }
                if !self.search_input.trim().is_empty() {
                    self.search_submitted = true;
                    return self.search(self.search_input.clone());
                }
            }

            Message::SearchResults(input, result) => {
                // Results for text that has changed since are stale.
                if input != self.search_input {
                    return Task::none();
                }
                match result {
                    Ok(results) => {
                        self.search_results = results;
                        self.search_highlight = None;
                        if std::mem::take(&mut self.search_submitted)
                            && !self.search_results.is_empty()
                        {
                            return self.update(Message::SearchSelect(0));
                        }
                    }
                    Err(err) => eprintln!("search for {input:?} failed: {err:?}"),
                }
            }

            Message::SearchSelect(index) => {
                let Some(result) = self.search_results.get(index).cloned() else {
                    return Task::none();
                };
                // Keep the query, so it can be refined.
                self.search_results.clear();
                self.search_highlight = None;
                let action = self.map.show_place(result.poi);
                self.sync_place_drawer();
                return self.run_map_action(action);
            }

            Message::Arrow(arrow) => {
                // Up and down move through search results while they are shown.
                let results = self.search_results.len();
                if self.search_active && results > 0 {
                    match arrow {
                        Arrow::Down => {
                            self.search_highlight =
                                Some(self.search_highlight.map_or(0, |i| (i + 1).min(results - 1)));
                            return Task::none();
                        }
                        Arrow::Up => {
                            self.search_highlight = self.search_highlight.map(|i| i.saturating_sub(1));
                            return Task::none();
                        }
                        Arrow::Left | Arrow::Right => {}
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
            }

            Message::Location(Err(err)) => {
                eprintln!("location unavailable: {err}");
            }

            Message::Reverse(request, result) => {
                self.map.insert_reverse(request, result);
                self.sync_place_drawer();
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
                    eprintln!("failed to open {url:?}: {err}");
                }
            },
        }
        Task::none()
    }
}

impl AppModel {
    /// Shows the place drawer while a place is selected, and hides it otherwise.
    /// Called when the selection may have changed, so that panning or the About
    /// drawer do not bring it back.
    fn sync_place_drawer(&mut self) {
        if self.map.has_place() {
            self.context_page = ContextPage::Place;
            self.core.window.show_context = true;
        } else if self.context_page == ContextPage::Place {
            self.core.window.show_context = false;
        }
    }

    /// Carries out work the map asked for.
    fn run_map_action(&mut self, action: Option<MapAction>) -> Task<cosmic::Action<Message>> {
        match action {
            Some(MapAction::FetchDetails(osm_id)) => cosmic::task::future(async move {
                Message::PoiDetails(osm_id, details::fetch(osm_id).await)
            }),
            Some(MapAction::Reverse {
                request,
                lat,
                lon,
                zoom,
            }) => cosmic::task::future(async move {
                Message::Reverse(request, details::reverse(lat, lon, zoom).await)
            }),
            Some(MapAction::OpenUrl(url)) => {
                cosmic::Application::update(self, Message::LaunchUrl(url))
            }
            None => Task::none(),
        }
    }

    /// Searches places for `query`, preferring those near the center of the map.
    fn search(&self, query: String) -> Task<cosmic::Action<Message>> {
        let near = self.map.center();
        cosmic::task::future(async move {
            let result = search::search(query.clone(), near).await;
            Message::SearchResults(query, result)
        })
    }

    /// Search results, floating over the top-left corner of the map.
    fn search_results_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let radius = cosmic::theme::active().cosmic().corner_radii.radius_s;

        let rows: Vec<Element<'_, Message>> = self
            .search_results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let caption = [result.poi.category(), result.area.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                let content = widget::column::with_capacity(2)
                    .push(widget::text::body(&result.poi.name))
                    .push_maybe((!caption.is_empty()).then(|| widget::text::caption(caption)))
                    .spacing(2);

                // `ListItem`, unlike `MenuItem`, shows the row chosen with the arrow keys.
                widget::button::custom(content)
                    .class(cosmic::theme::Button::ListItem(radius))
                    .selected(self.search_highlight == Some(index))
                    .width(Length::Fill)
                    .padding([spacing.space_xxs, spacing.space_s])
                    .on_press(Message::SearchSelect(index))
                    .into()
            })
            .collect();

        widget::container(widget::column::with_children(rows).spacing(spacing.space_xxxs))
            .padding(spacing.space_xxs)
            .width(Length::Fixed(SEARCH_RESULTS_WIDTH))
            .class(cosmic::theme::Container::Dropdown)
            .apply(widget::container)
            .padding(spacing.space_s)
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
