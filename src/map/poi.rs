// SPDX-License-Identifier: MPL-2.0

//! Points of interest read from the vector tiles.

use geozero::mvt::{Message, Tile, tile};
use i18n_embed::DesktopLanguageRequester;

use super::camera::TileId;

/// OpenMapTiles layer holding points of interest.
const POI_LAYER: &str = "poi";
/// Coordinate range of tile geometries when the layer does not say otherwise.
const DEFAULT_EXTENT: u32 = 4096;

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
    /// Web Mercator position, with the world spanning 0..1 on both axes.
    pub position: (f64, f64),
}

impl Poi {
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

impl OsmId {
    /// OpenMapTiles encodes feature ids as `osm_id * 10 + kind`, where kind 1 is a
    /// node and 2 a way.
    fn from_feature_id(id: u64) -> Option<Self> {
        match id % 10 {
            1 => Some(Self::Node(id / 10)),
            2 => Some(Self::Way(id / 10)),
            _ => None,
        }
    }
}

/// Language subtags of the system's preferred languages, most preferred first,
/// such as `["fr", "en"]` for `fr-FR` then `en-US`.
pub fn system_languages() -> Vec<String> {
    let mut languages: Vec<String> = Vec::new();
    for requested in DesktopLanguageRequester::requested_languages() {
        let language = requested.language.as_str().to_owned();
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    languages
}

/// Reads the named POIs of a vector tile, with names translated into the first of
/// `languages` the tile has.
pub fn parse_pois(data: &[u8], tile: TileId, languages: &[String]) -> Vec<Poi> {
    let Ok(decoded) = Tile::decode(data) else {
        return Vec::new();
    };
    let Some(layer) = decoded.layers.iter().find(|layer| layer.name == POI_LAYER) else {
        return Vec::new();
    };
    let Some(name_key) = layer.keys.iter().position(|key| key == "name") else {
        return Vec::new();
    };

    // OpenMapTiles stores translations as `name:<language>`.
    let translated_keys: Vec<usize> = languages
        .iter()
        .filter_map(|language| {
            let key = format!("name:{language}");
            layer.keys.iter().position(|k| *k == key)
        })
        .collect();

    let class_key = layer.keys.iter().position(|key| key == "class");
    let subclass_key = layer.keys.iter().position(|key| key == "subclass");
    let rank_key = layer.keys.iter().position(|key| key == "rank");

    let extent = f64::from(layer.extent.unwrap_or(DEFAULT_EXTENT));
    let tiles_per_side = f64::from(1u32 << tile.z);

    layer
        .features
        .iter()
        .filter_map(|feature| {
            let name = string_tag(layer, feature, name_key)?;
            let (x, y) = first_point(&feature.geometry)?;
            let translated_name = translated_keys
                .iter()
                .find_map(|&key| string_tag(layer, feature, key))
                .filter(|translated| *translated != name);

            Some(Poi {
                name,
                translated_name,
                class: class_key.and_then(|key| string_tag(layer, feature, key)),
                subclass: subclass_key.and_then(|key| string_tag(layer, feature, key)),
                rank: rank_key
                    .and_then(|key| integer_tag(layer, feature, key))
                    .unwrap_or(u32::MAX),
                osm_id: feature.id.and_then(OsmId::from_feature_id),
                position: (
                    (f64::from(tile.x) + f64::from(x) / extent) / tiles_per_side,
                    (f64::from(tile.y) + f64::from(y) / extent) / tiles_per_side,
                ),
            })
        })
        .collect()
}

/// Tags are stored as pairs of indices into the layer's keys and values.
fn string_tag(layer: &tile::Layer, feature: &tile::Feature, key: usize) -> Option<String> {
    let pair = feature
        .tags
        .as_chunks::<2>()
        .0
        .iter()
        .find(|pair| pair[0] as usize == key)?;

    layer.values.get(pair[1] as usize)?.string_value.clone()
}

fn integer_tag(layer: &tile::Layer, feature: &tile::Feature, key: usize) -> Option<u32> {
    let pair = feature
        .tags
        .as_chunks::<2>()
        .0
        .iter()
        .find(|pair| pair[0] as usize == key)?;
    let value = layer.values.get(pair[1] as usize)?;

    // Encoders may use any of the integer types.
    let value = value
        .sint_value
        .or(value.int_value)
        .or(value.uint_value.and_then(|v| i64::try_from(v).ok()))?;
    u32::try_from(value).ok()
}

/// Decodes the first point of a geometry: a MoveTo command followed by
/// zigzag-encoded coordinates.
fn first_point(geometry: &[u32]) -> Option<(i32, i32)> {
    const MOVE_TO: u32 = 1;

    let [command, x, y, ..] = *geometry else {
        return None;
    };
    if command & 0x7 != MOVE_TO {
        return None;
    }

    Some((zigzag(x), zigzag(y)))
}

fn zigzag(value: u32) -> i32 {
    (value >> 1) as i32 ^ -((value & 1) as i32)
}
