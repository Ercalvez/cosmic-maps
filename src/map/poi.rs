use geozero::mvt::{Message, Tile, tile};

use super::camera::TileId;

/// OpenMapTiles layer holding points of interest.
const POI_LAYER: &str = "poi";
/// Coordinate range of tile geometries when the layer does not say otherwise.
const DEFAULT_EXTENT: u32 = 4096;

#[derive(Debug, Clone)]
pub struct Poi {
    pub name: String,
    #[expect(dead_code, reason = "used to fetch extra details later")]
    pub osm_id: Option<OsmId>,
    /// Web Mercator position, with the world spanning 0..1 on both axes.
    pub position: (f64, f64),
}

/// The OpenStreetMap object a POI comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsmId {
    Node(u64),
    Way(u64),
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

/// Reads the named POIs of a vector tile.
pub fn parse_pois(data: &[u8], tile: TileId) -> Vec<Poi> {
    let Ok(decoded) = Tile::decode(data) else {
        return Vec::new();
    };
    let Some(layer) = decoded.layers.iter().find(|layer| layer.name == POI_LAYER) else {
        return Vec::new();
    };
    let Some(name_key) = layer.keys.iter().position(|key| key == "name") else {
        return Vec::new();
    };

    let extent = f64::from(layer.extent.unwrap_or(DEFAULT_EXTENT));
    let tiles_per_side = f64::from(1u32 << tile.z);

    layer
        .features
        .iter()
        .filter_map(|feature| {
            let name = string_tag(layer, feature, name_key)?;
            let (x, y) = first_point(&feature.geometry)?;

            Some(Poi {
                name,
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
        .chunks_exact(2)
        .find(|pair| pair[0] as usize == key)?;

    layer.values.get(pair[1] as usize)?.string_value.clone()
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
