// SPDX-License-Identifier: MPL-2.0

//! The place the user selected: a POI they clicked or searched for, or what a
//! click on bare map turns out to be, with what is known about it.

use std::collections::HashMap;

use super::MapAction;
use crate::domain::place::{OsmId, Poi, PoiDetails, Shape};
use crate::geo::WorldPoint;

/// What asking for a place's details came to.
pub enum DetailsAnswer {
    Found(PoiDetails),
    /// The service does not know the place.
    Unknown,
    /// The request failed: it can be tried again.
    Failed,
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

/// A click that hit no POI, waiting for Nominatim to say what is there.
#[derive(Clone, Copy)]
struct Lookup {
    request: u64,
    position: WorldPoint,
}

#[derive(Default)]
pub(super) struct Selection {
    /// POI shown in the place panel.
    selected: Option<Poi>,
    /// Shown instead of `selected` until the lookup answers.
    lookup: Option<Lookup>,
    next_lookup: u64,
    details: HashMap<OsmId, DetailsState>,
}

impl Selection {
    /// Selects `poi`. Returns the request for its details, unless they are known or
    /// on their way.
    pub(super) fn select(&mut self, poi: Poi) -> Option<MapAction> {
        self.lookup = None;
        self.selected = Some(poi);
        self.request_details()
    }

    /// Clears the selection and any lookup under way.
    pub(super) fn close(&mut self) {
        self.selected = None;
        self.lookup = None;
    }

    /// Starts finding what is at `position`, where a click hit no POI. `zoom` is the
    /// detail the answer should have.
    pub(super) fn look_up(&mut self, position: WorldPoint, zoom: u8) -> MapAction {
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
            zoom,
        }
    }

    /// Selects what a lookup found, if there is anything and it is still the latest
    /// one.
    pub(super) fn insert_reverse(&mut self, request: u64, found: Option<(Poi, PoiDetails)>) {
        // A newer click, or closing the panel, replaced it.
        if self.lookup.map(|lookup| lookup.request) != Some(request) {
            return;
        }
        self.lookup = None;

        if let Some((poi, details)) = found {
            // The details came with the answer: no need to fetch them.
            if let Some(osm_id) = poi.osm_id {
                self.details.insert(osm_id, DetailsState::Loaded(details));
            }
            self.selected = Some(poi);
        }
    }

    /// Stores the answer to a [`MapAction::FetchDetails`].
    pub(super) fn insert_details(&mut self, osm_id: OsmId, answer: DetailsAnswer) {
        match answer {
            DetailsAnswer::Found(details) => {
                self.details.insert(osm_id, DetailsState::Loaded(details));
            }
            DetailsAnswer::Unknown => {
                self.details.insert(osm_id, DetailsState::Unavailable);
            }
            // Forget the request, so selecting the POI again retries it.
            DetailsAnswer::Failed => {
                self.details.remove(&osm_id);
            }
        }
    }

    fn request_details(&mut self) -> Option<MapAction> {
        let osm_id = self.selected.as_ref()?.osm_id?;
        if self.details.contains_key(&osm_id) {
            return None;
        }

        self.details.insert(osm_id, DetailsState::Loading);
        Some(MapAction::FetchDetails(osm_id))
    }

    /// The selected place, once known.
    pub(super) fn selected(&self) -> Option<&Poi> {
        self.selected.as_ref()
    }

    /// Whether a place is selected or being looked up, for the place panel.
    pub(super) fn has_place(&self) -> bool {
        self.selected.is_some() || self.lookup.is_some()
    }

    /// What the place panel shows, or `None` when nothing is selected.
    pub(super) fn place(&self) -> Option<Place<'_>> {
        match (&self.selected, self.lookup) {
            (Some(poi), _) => Some(Place::Selected {
                poi,
                details: poi.osm_id.and_then(|osm_id| self.details.get(&osm_id)),
            }),
            (None, Some(_)) => Some(Place::LookingUp),
            (None, None) => None,
        }
    }

    /// Where the selected place, or the click being looked up, is in the world.
    pub(super) fn position(&self) -> Option<WorldPoint> {
        match (&self.selected, self.lookup) {
            (Some(poi), _) => Some(poi.position),
            (None, Some(lookup)) => Some(lookup.position),
            (None, None) => None,
        }
    }

    /// The outline or line of the selected place, once its details have arrived.
    pub(super) fn shape(&self) -> Option<&Shape> {
        let osm_id = self.selected.as_ref()?.osm_id?;
        match self.details.get(&osm_id)? {
            DetailsState::Loaded(details) => details.shape.as_ref(),
            DetailsState::Loading | DetailsState::Unavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn louvre() -> Poi {
        let mut poi = Poi::new("Musée du Louvre", WorldPoint::new(0.5, 0.5));
        poi.osm_id = Some(OsmId::Way(1));
        poi
    }

    #[test]
    fn details_are_asked_for_once() {
        let mut selection = Selection::default();
        assert!(matches!(
            selection.select(louvre()),
            Some(MapAction::FetchDetails(OsmId::Way(1)))
        ));
        // Selecting it again while they are on their way asks nothing.
        assert!(selection.select(louvre()).is_none());

        // A failed request is forgotten, so the next selection retries it.
        selection.insert_details(OsmId::Way(1), DetailsAnswer::Failed);
        assert!(selection.select(louvre()).is_some());
        selection.insert_details(OsmId::Way(1), DetailsAnswer::Unknown);
        assert!(selection.select(louvre()).is_none());
    }

    #[test]
    fn only_the_latest_lookup_selects_what_it_found() {
        let mut selection = Selection::default();
        let at = WorldPoint::new(0.5, 0.5);
        let Some(MapAction::Reverse { request: first, .. }) = Some(selection.look_up(at, 16))
        else {
            panic!("expected a reverse lookup");
        };
        assert!(selection.has_place() && selection.selected().is_none());

        let MapAction::Reverse {
            request: second, ..
        } = selection.look_up(at, 16)
        else {
            panic!("expected a reverse lookup");
        };
        // The first answer arrives late: it is ignored.
        selection.insert_reverse(first, Some((louvre(), PoiDetails::default())));
        assert!(selection.selected().is_none());

        selection.insert_reverse(second, Some((louvre(), PoiDetails::default())));
        assert_eq!(
            selection.selected().map(|poi| poi.name.as_str()),
            Some("Musée du Louvre")
        );
        // The details came with the answer.
        assert!(matches!(
            selection.place(),
            Some(Place::Selected {
                details: Some(DetailsState::Loaded(_)),
                ..
            })
        ));

        selection.close();
        assert!(!selection.has_place());
    }
}
