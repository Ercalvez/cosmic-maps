use cosmic::iced::{Point, Size};

/// Edge length of a rendered tile, in logical pixels.
pub const TILE_SIZE: f32 = 512.0;

pub const MIN_ZOOM: f64 = 0.0;
pub const MAX_ZOOM: f64 = 17.0;
/// Length of the equator, in meters.
const EARTH_CIRCUMFERENCE: f64 = 40_075_016.686;

/// Highest zoom the tile source serves; beyond it, tiles are scaled up.
const MAX_TILE_ZOOM: u8 = 14;

/// A tile in the XYZ (slippy map) scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub x: i32,
    pub y: i32,
    pub z: u8,
}

impl TileId {
    /// The tile `levels` zoom levels up that contains this one.
    pub fn ancestor(self, levels: u8) -> Option<TileId> {
        Some(TileId {
            x: self.x >> levels,
            y: self.y >> levels,
            z: self.z.checked_sub(levels)?,
        })
    }
}

/// A tile to draw, with the position of its top-left corner in the viewport and its
/// edge length on screen.
pub struct VisibleTile {
    pub id: TileId,
    pub position: Point,
    pub size: f32,
}

/// What part of the world is shown. Positions are in world pixels at the current
/// zoom, where the whole world is `TILE_SIZE * 2^zoom` pixels wide.
pub struct Camera {
    center: (f64, f64),
    zoom: f64,
}

impl Camera {
    pub fn from_lat_lon(lat: f64, lon: f64, zoom: f64) -> Self {
        Self::centered_on(world_position(lat, lon), zoom)
    }

    /// A camera showing `position`, in Web Mercator coordinates spanning 0..1.
    pub fn centered_on(position: (f64, f64), zoom: f64) -> Self {
        let world = world_size(zoom);

        Self {
            center: (position.0 * world, position.1 * world),
            zoom,
        }
    }

    /// A camera showing the box from `min` to `max` (Web Mercator, spanning 0..1)
    /// in `viewport`, leaving `padding` pixels on every side.
    pub fn fitting(min: (f64, f64), max: (f64, f64), viewport: Size, padding: f32) -> Self {
        let available = |span: f32| f64::from((span - 2.0 * padding).max(1.0));
        let tile = f64::from(TILE_SIZE);
        // Zoom at which each side fills the space; a point fits at any zoom.
        let fit = |extent: f64, space: f64| (space / (extent * tile)).log2();
        let zoom = fit(max.0 - min.0, available(viewport.width))
            .min(fit(max.1 - min.1, available(viewport.height)))
            .clamp(MIN_ZOOM, MAX_ZOOM);

        Self::centered_on(((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0), zoom)
    }

    /// Latitude and longitude of the center of the view.
    pub fn center_lat_lon(&self) -> (f64, f64) {
        let world = world_size(self.zoom);
        lat_lon((self.center.0 / world, self.center.1 / world))
    }

    /// Web Mercator position, spanning 0..1, of a point in the viewport.
    pub fn to_world(&self, point: Point, viewport: Size) -> (f64, f64) {
        let world = world_size(self.zoom);
        let x = self.center.0 + f64::from(point.x - viewport.width / 2.0);
        let y = self.center.1 + f64::from(point.y - viewport.height / 2.0);

        ((x / world).rem_euclid(1.0), (y / world).clamp(0.0, 1.0))
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Ground distance one pixel covers at latitude `lat`, in meters.
    pub fn meters_per_pixel(&self, lat: f64) -> f64 {
        EARTH_CIRCUMFERENCE * lat.to_radians().cos() / world_size(self.zoom)
    }

    /// Viewport position of a point in Web Mercator coordinates spanning 0..1.
    pub fn to_screen(&self, position: (f64, f64), viewport: Size) -> Point {
        let world = world_size(self.zoom);
        // The world repeats horizontally: use the copy closest to the center.
        let dx = (position.0 * world - self.center.0 + world / 2.0).rem_euclid(world) - world / 2.0;
        let dy = position.1 * world - self.center.1;

        Point::new(
            viewport.width / 2.0 + dx as f32,
            viewport.height / 2.0 + dy as f32,
        )
    }

    /// Moves the map content by `(dx, dy)` screen pixels.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        let world = world_size(self.zoom);
        // The world repeats horizontally but stops at the poles.
        self.center.0 = (self.center.0 - dx).rem_euclid(world);
        self.center.1 = (self.center.1 - dy).clamp(0.0, world);
    }

    /// Changes the zoom by `delta` levels, keeping the point under `anchor` (in
    /// viewport coordinates) in place.
    pub fn zoom_at(&mut self, delta: f64, anchor: Point, viewport: Size) {
        let zoom = (self.zoom + delta).clamp(MIN_ZOOM, MAX_ZOOM);
        let factor = 2f64.powf(zoom - self.zoom);
        self.zoom = zoom;

        // Offset of the anchor from the center, which stays the same on screen.
        let offset_x = f64::from(anchor.x - viewport.width / 2.0);
        let offset_y = f64::from(anchor.y - viewport.height / 2.0);

        self.center.0 = (self.center.0 + offset_x) * factor - offset_x;
        self.center.1 = (self.center.1 + offset_y) * factor - offset_y;
        self.pan(0.0, 0.0);
    }

    /// Tiles covering a viewport of the given size, closest to the center first.
    pub fn visible_tiles(&self, viewport: Size) -> Vec<VisibleTile> {
        let level = (self.zoom.round() as u8).min(MAX_TILE_ZOOM);
        // Tiles of the nearest level are scaled to the fractional zoom.
        let tile = f64::from(TILE_SIZE) * 2f64.powf(self.zoom - f64::from(level));
        let tiles_per_side = 1i64 << level;
        let left = self.center.0 - f64::from(viewport.width) / 2.0;
        let top = self.center.1 - f64::from(viewport.height) / 2.0;

        let first_x = (left / tile).floor() as i64;
        let last_x = ((left + f64::from(viewport.width)) / tile).floor() as i64;
        let first_y = ((top / tile).floor() as i64).max(0);
        let last_y = (((top + f64::from(viewport.height)) / tile).floor() as i64)
            .min(tiles_per_side - 1);

        let mut tiles = Vec::new();
        for y in first_y..=last_y {
            for x in first_x..=last_x {
                tiles.push(VisibleTile {
                    id: TileId {
                        x: x.rem_euclid(tiles_per_side) as i32,
                        y: y as i32,
                        z: level,
                    },
                    position: Point::new(
                        (x as f64 * tile - left) as f32,
                        (y as f64 * tile - top) as f32,
                    ),
                    size: tile as f32,
                });
            }
        }

        // Where the top-left corner of a tile centered in the viewport would be.
        let tile = tile as f32;
        let centered = Point::new(
            (viewport.width - tile) / 2.0,
            (viewport.height - tile) / 2.0,
        );
        tiles.sort_by(|a, b| {
            a.position
                .distance(centered)
                .total_cmp(&b.position.distance(centered))
        });

        tiles
    }
}

/// Zoom at which a circle of `radius` meters around latitude `lat` spans half of
/// `span` pixels.
pub fn zoom_showing(radius: f64, lat: f64, span: f32) -> f64 {
    let meters_per_pixel = 4.0 * radius / f64::from(span);
    (EARTH_CIRCUMFERENCE * lat.to_radians().cos() / (f64::from(TILE_SIZE) * meters_per_pixel))
        .log2()
        .clamp(MIN_ZOOM, MAX_ZOOM)
}

/// Web Mercator position of a latitude and longitude, spanning 0..1 on both axes.
pub fn world_position(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.to_radians();
    let x = (lon + 180.0) / 360.0;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0;

    (x, y)
}

/// Latitude and longitude of a Web Mercator position spanning 0..1.
pub fn lat_lon(position: (f64, f64)) -> (f64, f64) {
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * position.1))
        .sinh()
        .atan()
        .to_degrees();

    (lat, position.0 * 360.0 - 180.0)
}

fn world_size(zoom: f64) -> f64 {
    f64::from(TILE_SIZE) * 2f64.powf(zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_shows_the_whole_box() {
        // Musée du Louvre to Gare de Lyon, Paris.
        let (min, max) = (world_position(48.8606, 2.3376), world_position(48.8443, 2.3744));
        let (min, max) = ((min.0.min(max.0), min.1.min(max.1)), (min.0.max(max.0), min.1.max(max.1)));
        let viewport = Size::new(800.0, 600.0);
        let camera = Camera::fitting(min, max, viewport, 40.0);

        for corner in [min, max] {
            let point = camera.to_screen(corner, viewport);
            assert!((40.0 - 0.5..=760.0 + 0.5).contains(&point.x), "{point:?}");
            assert!((40.0 - 0.5..=560.0 + 0.5).contains(&point.y), "{point:?}");
        }
        // The box fills one direction of the space.
        let width = camera.to_screen(max, viewport).x - camera.to_screen(min, viewport).x;
        let height = camera.to_screen(max, viewport).y - camera.to_screen(min, viewport).y;
        assert!((width - 720.0).abs() < 1.0 || (height - 520.0).abs() < 1.0);
    }

    #[test]
    fn fitting_a_point_zooms_in_fully() {
        let point = world_position(48.8606, 2.3376);
        let camera = Camera::fitting(point, point, Size::new(800.0, 600.0), 40.0);
        assert_eq!(camera.zoom(), MAX_ZOOM);
    }
}
