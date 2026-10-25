// SPDX-License-Identifier: MPL-2.0

//! The directions form: transport mode, start and destination. No route is
//! computed yet.

use cosmic::prelude::*;
use cosmic::widget::{self, icon, segmented_button};

use crate::app::Message;
use crate::fl;
use crate::map::location::Position;
use crate::map::poi::Poi;
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
}

/// One end of the trip.
#[derive(Debug, Clone)]
pub enum Endpoint {
    Unset,
    #[expect(dead_code, reason = "read once routes are computed")]
    UserLocation(Position),
    Place(Poi),
}

pub struct Directions {
    modes: segmented_button::SingleSelectModel,
    from: Endpoint,
    to: Endpoint,
    /// The user changed the start, so a position arriving must not replace it.
    from_edited: bool,
    from_search: SearchBox,
    to_search: SearchBox,
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

    /// The user's position became known. It becomes the start unless the user
    /// already chose or cleared one.
    pub fn user_position_known(&mut self, position: Position) {
        match self.from {
            Endpoint::Unset if !self.from_edited => {
                self.set(SearchTarget::From, Endpoint::UserLocation(position));
            }
            // Keep following the user while the start is their location.
            Endpoint::UserLocation(_) => self.from = Endpoint::UserLocation(position),
            _ => {}
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

    pub fn view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();

        let modes = widget::segmented_control::horizontal(&self.modes)
            .on_activate(Message::DirectionsMode);

        let swap = widget::tooltip(
            widget::button::icon(icon::from_name("object-flip-vertical-symbolic"))
                .on_press(Message::DirectionsSwap),
            widget::text::body(fl!("directions-swap")),
            widget::tooltip::Position::Bottom,
        );

        widget::column::with_capacity(4)
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
            .spacing(spacing.space_s)
            .into()
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

    fn here() -> Position {
        Position {
            latitude: 48.39,
            longitude: -4.48,
            accuracy: 50.0,
        }
    }

    #[test]
    fn defaults_with_and_without_location() {
        let directions = Directions::new(&place("Hôtel Oceania"), Some(here()));
        assert!(matches!(directions.from, Endpoint::UserLocation(_)));
        assert_eq!(directions.from_search.input, fl!("your-location"));
        assert!(matches!(&directions.to, Endpoint::Place(poi) if poi.name == "Hôtel Oceania"));
        assert_eq!(directions.to_search.input, "Hôtel Oceania");

        let directions = Directions::new(&place("Hôtel Oceania"), None);
        assert!(matches!(directions.from, Endpoint::Unset));
        assert!(directions.from_search.input.is_empty());
    }

    #[test]
    fn location_fills_an_untouched_empty_start_only() {
        let mut directions = Directions::new(&place("Port"), None);
        directions.user_position_known(here());
        assert!(matches!(directions.from, Endpoint::UserLocation(_)));

        let mut directions = Directions::new(&place("Port"), None);
        directions.on_input(SearchTarget::From, "Gare".to_owned());
        directions.on_input(SearchTarget::From, String::new());
        directions.user_position_known(here());
        assert!(matches!(directions.from, Endpoint::Unset));
    }

    #[test]
    fn swap_exchanges_places_and_texts() {
        let mut directions = Directions::new(&place("Port"), Some(here()));
        directions.swap();
        assert!(matches!(&directions.from, Endpoint::Place(poi) if poi.name == "Port"));
        assert!(matches!(directions.to, Endpoint::UserLocation(_)));
        assert_eq!(directions.from_search.input, "Port");
        assert_eq!(directions.to_search.input, fl!("your-location"));
    }
}
