// SPDX-License-Identifier: MPL-2.0

//! Places: what the map shows, what searches find, and what is known about them.

use crate::geo::WorldPoint;

#[derive(Debug, Clone)]
pub struct Poi {
    /// Name in the local language, as written on site.
    pub name: String,
    /// Name in the user's language, when the tile has one that differs from `name`.
    pub translated_name: Option<String>,
    /// OpenMapTiles category, such as `railway`.
    pub class: Option<String>,
    /// More precise category, usually the OpenStreetMap tag value, such as `tram_stop`.
    pub subclass: Option<String>,
    /// Importance within the tile, lower is more important. Missing ranks are
    /// treated as least important.
    pub rank: u32,
    pub osm_id: Option<OsmId>,
    pub position: WorldPoint,
}

impl Poi {
    /// A place with just a name and a position.
    pub fn new(name: impl Into<String>, position: WorldPoint) -> Self {
        Self {
            name: name.into(),
            translated_name: None,
            class: None,
            subclass: None,
            rank: 0,
            osm_id: None,
            position,
        }
    }

    /// Readable kind of place, such as "Tram stop".
    pub fn category(&self) -> Option<String> {
        // `subclass` first: `class` can be misleading, such as `art_gallery` for
        // street artwork.
        // `yes` says nothing, as in `building=yes`.
        let values = [self.subclass.as_deref(), self.class.as_deref()]
            .map(|value| value.filter(|v| *v != "yes"));

        values
            .iter()
            .flatten()
            .find_map(|value| crate::i18n::get_dynamic(&format!("poi-type-{value}")))
            .or_else(|| values.iter().flatten().next().map(|value| readable(value)))
    }
}

/// Turns a tag value such as `escape_game` into "Escape game".
fn readable(value: &str) -> String {
    let text = value.replace('_', " ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The OpenStreetMap object a POI comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsmId {
    Node(u64),
    Way(u64),
    Relation(u64),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoiDetails {
    /// Street and city, such as "Schillingstraße 12, 10179 Berlin".
    pub address: Option<String>,
    /// Raw `opening_hours` tag, such as "Mo-Fr 09:00-19:00".
    pub opening_hours: Option<String>,
    pub website: Option<String>,
    pub phone: Option<String>,
    pub wheelchair: Option<Wheelchair>,
    /// The place's outline or line, to highlight it on the map.
    pub shape: Option<Shape>,
}

/// The extent of a place, in Web Mercator coordinates spanning 0..1.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Streets and rivers: one or more lines.
    Lines(Vec<Vec<WorldPoint>>),
    /// Buildings and areas: the rings of one or more polygons, holes included.
    Area(Vec<Vec<WorldPoint>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wheelchair {
    Yes,
    Limited,
    No,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub poi: Poi,
    /// Where the place is, such as "Dircksenstraße 2, Berlin, Allemagne".
    pub area: Option<String>,
}
