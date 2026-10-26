// SPDX-License-Identifier: MPL-2.0

//! Type-ahead place search state, shared by the header search and the directions
//! fields.

use std::time::Duration;

use cosmic::iced::Length;
use cosmic::prelude::*;
use cosmic::widget;

use crate::services::{self, photon::SearchResult};

/// Pause after typing before searching, and the shortest query searched, as in
/// GNOME Maps.
pub const SEARCH_DELAY: Duration = Duration::from_millis(500);
const SEARCH_MIN_CHARS: usize = 3;

/// Messages of a search field.
#[derive(Debug, Clone)]
pub enum Message {
    Input(String),
    /// Sent `SEARCH_DELAY` after typing; searches if the text is still the same.
    Debounced(String),
    /// Enter was pressed.
    Submit,
    /// Places found for this text.
    Results(String, Result<Vec<SearchResult>, services::Error>),
    /// A result was clicked.
    Select(usize),
}

/// A place search to run, sending its answer back as [`Message::Results`].
#[derive(Debug, PartialEq, Eq)]
pub enum Query {
    /// After `SEARCH_DELAY`, as [`Message::Debounced`].
    Later(String),
    Now(String),
}

/// What a message to a search field leads to, from [`SearchBox::update`].
#[derive(Debug)]
pub enum Action {
    Query(Query),
    /// The user picked this place.
    Pick(SearchResult),
}

/// What pressing Enter in a search field leads to.
#[derive(Debug, PartialEq, Eq)]
enum Submit {
    /// Pick the result at this index.
    Pick(usize),
    /// Search this text now, and pick the first result when it arrives.
    Search(String),
    Nothing,
}

#[derive(Debug, Default)]
pub struct SearchBox {
    /// The field's text.
    pub input: String,
    pub results: Vec<SearchResult>,
    /// Result chosen with the arrow keys.
    pub highlight: Option<usize>,
    /// Enter was pressed before results arrived: pick the first one when they do.
    submitted: bool,
}

impl SearchBox {
    pub fn update(&mut self, message: Message) -> Option<Action> {
        match message {
            Message::Input(text) => self
                .on_input(text)
                .map(|query| Action::Query(Query::Later(query))),
            // Typing continued: a later message will search.
            Message::Debounced(query) => self
                .is_current(&query)
                .then_some(Action::Query(Query::Now(query))),
            Message::Submit => match self.submit() {
                Submit::Pick(index) => self.take(index).map(Action::Pick),
                Submit::Search(query) => Some(Action::Query(Query::Now(query))),
                Submit::Nothing => None,
            },
            Message::Results(query, result) => {
                // Results for text that has changed since are stale.
                if !self.is_current(&query) {
                    return None;
                }
                match result {
                    Ok(results) => {
                        let index = self.set_results(results)?;
                        self.take(index).map(Action::Pick)
                    }
                    Err(err) => {
                        tracing::warn!("search for {query:?} failed: {err:?}");
                        None
                    }
                }
            }
            Message::Select(index) => self.take(index).map(Action::Pick),
        }
    }

    /// Records typed text. Returns the query to search after `SEARCH_DELAY`, if it
    /// is long enough.
    fn on_input(&mut self, input: String) -> Option<String> {
        self.input.clone_from(&input);
        self.highlight = None;
        if input.trim().chars().count() < SEARCH_MIN_CHARS {
            self.hide_results();
            self.submitted = false;
            return None;
        }
        Some(input)
    }

    /// Whether `query` is still the field's text: results for older text are stale.
    fn is_current(&self, query: &str) -> bool {
        self.input == query
    }

    fn submit(&mut self) -> Submit {
        if let Some(index) = self.highlight.or((!self.results.is_empty()).then_some(0)) {
            return Submit::Pick(index);
        }
        if self.input.trim().is_empty() {
            return Submit::Nothing;
        }
        self.submitted = true;
        Submit::Search(self.input.clone())
    }

    /// Stores results for the current text. Returns the result to pick, if Enter was
    /// pressed before they arrived.
    fn set_results(&mut self, results: Vec<SearchResult>) -> Option<usize> {
        self.results = results;
        self.highlight = None;
        (std::mem::take(&mut self.submitted) && !self.results.is_empty()).then_some(0)
    }

    /// Takes a result, hiding the list and keeping the text so it can be refined.
    fn take(&mut self, index: usize) -> Option<SearchResult> {
        let result = self.results.get(index).cloned()?;
        self.hide_results();
        Some(result)
    }

    /// Replaces the text, such as with the name of a picked place.
    pub fn set_text(&mut self, text: String) {
        self.input = text;
        self.hide_results();
        self.submitted = false;
    }

    pub fn hide_results(&mut self) {
        self.results.clear();
        self.highlight = None;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Moves the highlight through the results. Returns `false` when none are shown.
    pub fn move_highlight(&mut self, down: bool) -> bool {
        let count = self.results.len();
        if count == 0 {
            return false;
        }
        self.highlight = if down {
            Some(self.highlight.map_or(0, |i| (i + 1).min(count - 1)))
        } else {
            self.highlight.map(|i| i.saturating_sub(1))
        };
        true
    }

    /// One row per result: name, then category and area.
    pub fn results_view<'a, Message: Clone + 'static>(
        &'a self,
        on_pick: impl Fn(usize) -> Message,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let radius = cosmic::theme::active().cosmic().corner_radii.radius_s;

        let rows: Vec<Element<'a, Message>> = self
            .results
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
                    .selected(self.highlight == Some(index))
                    .width(Length::Fill)
                    .padding([spacing.space_xxs, spacing.space_s])
                    .on_press(on_pick(index))
                    .into()
            })
            .collect();

        widget::column::with_children(rows)
            .spacing(spacing.space_xxxs)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::WorldPoint;
    use crate::map::poi::Poi;

    fn result(name: &str) -> SearchResult {
        SearchResult {
            poi: Poi {
                name: name.to_owned(),
                translated_name: None,
                class: None,
                subclass: None,
                rank: 0,
                osm_id: None,
                position: WorldPoint::new(0.5, 0.5),
            },
            area: None,
        }
    }

    #[test]
    fn short_text_does_not_search() {
        let mut search = SearchBox::default();
        assert_eq!(search.on_input("ab".to_owned()), None);
        assert_eq!(search.on_input("abc".to_owned()), Some("abc".to_owned()));
    }

    #[test]
    fn older_results_are_stale() {
        let mut search = SearchBox::default();
        search.on_input("alex".to_owned());
        search.on_input("alexander".to_owned());
        assert!(!search.is_current("alex"));
        assert!(search.is_current("alexander"));
    }

    #[test]
    fn highlight_stays_within_results() {
        let mut search = SearchBox::default();
        assert!(!search.move_highlight(true));

        search.set_results(vec![result("a"), result("b")]);
        search.move_highlight(true);
        search.move_highlight(true);
        search.move_highlight(true);
        assert_eq!(search.highlight, Some(1));
        search.move_highlight(false);
        search.move_highlight(false);
        assert_eq!(search.highlight, Some(0));
    }

    #[test]
    fn enter_before_results_picks_the_first() {
        let mut search = SearchBox::default();
        search.on_input("berlin".to_owned());
        assert_eq!(search.submit(), Submit::Search("berlin".to_owned()));
        assert_eq!(search.set_results(vec![result("Berlin")]), Some(0));

        // Later results are not picked automatically.
        assert_eq!(search.set_results(vec![result("Berlin")]), None);
        assert_eq!(search.submit(), Submit::Pick(0));
    }

    #[test]
    fn typing_searches_after_the_pause_only_if_unchanged() {
        let mut search = SearchBox::default();
        let update = |search: &mut SearchBox, message| match search.update(message) {
            Some(Action::Query(query)) => Some(query),
            _ => None,
        };
        let later = update(&mut search, Message::Input("Châtelet".to_owned()));
        assert_eq!(later, Some(Query::Later("Châtelet".to_owned())));
        search.update(Message::Input("Châtelet-Les Halles".to_owned()));

        assert_eq!(
            update(&mut search, Message::Debounced("Châtelet".to_owned())),
            None
        );
        let now = update(
            &mut search,
            Message::Debounced("Châtelet-Les Halles".to_owned()),
        );
        assert_eq!(now, Some(Query::Now("Châtelet-Les Halles".to_owned())));
    }

    #[test]
    fn enter_then_results_picks_the_first() {
        let mut search = SearchBox::default();
        search.update(Message::Input("Châtelet".to_owned()));
        search.update(Message::Submit);
        let picked = search.update(Message::Results(
            "Châtelet".to_owned(),
            Ok(vec![result("Châtelet")]),
        ));
        assert!(matches!(picked, Some(Action::Pick(result)) if result.poi.name == "Châtelet"));
        // The text stays, without the results.
        assert_eq!(search.input, "Châtelet");
        assert!(search.results.is_empty());
    }
}
