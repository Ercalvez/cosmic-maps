// SPDX-License-Identifier: MPL-2.0

//! Points of interest read from the vector tiles.

use geozero::mvt::{Message, Tile, tile};

use super::camera::TileId;
use crate::domain::place::{OsmId, Poi};
use crate::geo::WorldPoint;

/// OpenMapTiles layer holding points of interest.
const POI_LAYER: &str = "poi";
/// Coordinate range of tile geometries when the layer does not say otherwise.
const DEFAULT_EXTENT: u32 = 4096;

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
                position: WorldPoint::new(
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
