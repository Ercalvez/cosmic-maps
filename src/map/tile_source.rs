// SPDX-License-Identifier: MPL-2.0

//! Where the vector tiles come from: OpenFreeMap, whose public instance is free,
//! needs no key and allows apps, as long as it is credited (see
//! [`attribution::CREDITS`](super::attribution::CREDITS)).
//!
//! The tiles follow the OpenMapTiles schema, at zooms 0 to 14. `latest` always
//! points to the newest planet build, so the URL does not change.

use super::camera::TileId;

const TILE_URL: &str = "https://tiles.openfreemap.org/planet/latest/{z}/{x}/{y}.pbf";

/// The address of a tile, in the XYZ (slippy map) scheme.
fn tile_url(tile: TileId) -> String {
    TILE_URL
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Downloads a tile, as the protobuf MapLibre reads.
pub async fn fetch(client: &reqwest::Client, tile: TileId) -> Result<Box<[u8]>, reqwest::Error> {
    let tile = client
        .get(tile_url(tile))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(tile.to_vec().into_boxed_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_addressed_by_zoom_column_and_row() {
        // The tile holding the Musée du Louvre, Paris.
        let louvre = TileId {
            x: 8298,
            y: 5636,
            z: 14,
        };
        assert_eq!(
            tile_url(louvre),
            "https://tiles.openfreemap.org/planet/latest/14/8298/5636.pbf"
        );
    }
}
