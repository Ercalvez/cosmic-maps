// SPDX-License-Identifier: MPL-2.0

//! Encoded polylines, which MOTIS gives leg geometries in.

use crate::geo::LatLon;

/// Decodes an encoded polyline (Google's format) into latitudes and longitudes.
/// `precision` is the number of decimals kept, 5 in Google's format, 6 in MOTIS.
pub fn decode_polyline(encoded: &str, precision: u32) -> Vec<LatLon> {
    let factor = 10f64.powi(precision as i32);
    let mut bytes = encoded.bytes();
    let mut next_delta = || -> Option<i64> {
        let mut result = 0i64;
        let mut shift = 0;
        loop {
            let chunk = i64::from(bytes.next()?) - 63;
            result |= (chunk & 0x1f) << shift;
            shift += 5;
            if chunk < 0x20 {
                break;
            }
        }
        // Zigzag encoding: the lowest bit is the sign.
        Some(if result & 1 == 1 {
            !(result >> 1)
        } else {
            result >> 1
        })
    };

    let mut points = Vec::new();
    let (mut lat, mut lon) = (0i64, 0i64);
    while let (Some(dlat), Some(dlon)) = (next_delta(), next_delta()) {
        lat += dlat;
        lon += dlon;
        points.push(LatLon::new(lat as f64 / factor, lon as f64 / factor));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_polylines() {
        // The example from Google's polyline format documentation.
        let points = decode_polyline("_p~iF~ps|U_ulLnnqC_mqNvxq`@", 5);
        let expected = [(38.5, -120.2), (40.7, -120.95), (43.252, -126.453)];

        assert_eq!(points.len(), expected.len());
        for (point, (expected_lat, expected_lon)) in points.into_iter().zip(expected) {
            assert!(
                (point.lat - expected_lat).abs() < 1e-9 && (point.lon - expected_lon).abs() < 1e-9
            );
        }
    }
}
