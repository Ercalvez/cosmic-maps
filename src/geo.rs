// SPDX-License-Identifier: MPL-2.0

//! Geographic coordinates and their Web Mercator projection, which the tiles and
//! the map use.

use std::f64::consts::PI;

/// Latitude and longitude, in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Where this is in the Web Mercator world.
    pub fn to_world(self) -> WorldPoint {
        let lat = self.lat.to_radians();
        let x = (self.lon + 180.0) / 360.0;
        let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0;

        WorldPoint { x, y }
    }
}

/// Where the user is, and how well that is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub lat_lon: LatLon,
    /// Radius of the area the user is in, in meters.
    pub accuracy: f64,
}

/// A point of the Web Mercator world, which spans 0 to 1 on both axes, `y`
/// growing southwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint {
    pub x: f64,
    pub y: f64,
}

impl WorldPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn to_lat_lon(self) -> LatLon {
        let lat = (PI * (1.0 - 2.0 * self.y)).sinh().atan().to_degrees();

        LatLon {
            lat,
            lon: self.x * 360.0 - 180.0,
        }
    }
}

/// The smallest box holding some points of the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: WorldPoint,
    pub max: WorldPoint,
}

impl WorldBounds {
    /// The box around `points`, or `None` if there are none.
    pub fn around(points: impl IntoIterator<Item = WorldPoint>) -> Option<Self> {
        points.into_iter().fold(None, |bounds, point| {
            let Self { min, max } = bounds.unwrap_or(Self {
                min: point,
                max: point,
            });
            Some(Self {
                min: WorldPoint::new(min.x.min(point.x), min.y.min(point.y)),
                max: WorldPoint::new(max.x.max(point.x), max.y.max(point.y)),
            })
        })
    }

    pub fn center(&self) -> WorldPoint {
        WorldPoint::new(
            self.min.x.midpoint(self.max.x),
            self.min.y.midpoint(self.max.y),
        )
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Near the Musée du Louvre, Paris.
    const LOUVRE: LatLon = LatLon::new(48.8606, 2.3376);

    #[test]
    fn the_projection_round_trips() {
        let back = LOUVRE.to_world().to_lat_lon();
        assert!((back.lat - LOUVRE.lat).abs() < 1e-9);
        assert!((back.lon - LOUVRE.lon).abs() < 1e-9);
    }

    #[test]
    fn the_world_spans_zero_to_one() {
        let origin = LatLon::new(0.0, -180.0).to_world();
        assert_eq!((origin.x, origin.y), (0.0, 0.5));
        let paris = LOUVRE.to_world();
        assert!((0.0..=1.0).contains(&paris.x) && (0.0..=1.0).contains(&paris.y));
        // Greenwich is a little west of Paris.
        assert!(LatLon::new(48.8606, 0.0).to_world().x < paris.x);
    }

    #[test]
    fn bounds_surround_their_points() {
        let gare_de_lyon = LatLon::new(48.8443, 2.3744).to_world();
        let bounds = WorldBounds::around([LOUVRE.to_world(), gare_de_lyon]).unwrap();

        assert_eq!(bounds.min.x, LOUVRE.to_world().x);
        assert_eq!(bounds.max.x, gare_de_lyon.x);
        // North is smaller `y`.
        assert_eq!(bounds.min.y, LOUVRE.to_world().y);
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
        assert!(WorldBounds::around([]).is_none());
    }
}
