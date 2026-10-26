// SPDX-License-Identifier: MPL-2.0

//! The rendered tiles, and the POIs read from them.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic::iced::futures::Stream;
use cosmic::iced::widget::image::Handle;
use cosmic::iced::{Point, Rectangle, Size};

use crate::domain::place::Poi;
use crate::map::camera::{Camera, TileId};
use crate::map::state::{MapState, RenderedTile};
use crate::map::style::MapPalette;

/// Zoom from which POIs are drawn and clickable, as in GNOME Maps. Below it, the
/// tiles in use only hold a few POIs.
const POI_MIN_ZOOM: f64 = 15.0;
/// Farthest a click may land from a POI marker, in pixels, to select it.
const POI_HIT_RADIUS: f32 = 10.0;
/// The same for POIs without a marker, which have to be aimed at more precisely.
const HIDDEN_POI_HIT_RADIUS: f32 = 6.0;
/// How many levels up to look for a loaded tile to show while one is missing.
const MAX_FALLBACK_LEVELS: u8 = 6;

/// A rendered tile, or `None` if it failed to load.
struct CachedTile {
    image: Option<Handle>,
    palette: MapPalette,
}

impl CachedTile {
    /// Rendered with other colours. Failed tiles are not retried for a new palette.
    fn is_stale(&self, palette: &MapPalette) -> bool {
        self.image.is_some() && self.palette != *palette
    }
}

pub(super) struct Tiles {
    state: MapState,
    /// Rendered tiles, possibly in previous colours until re-rendered.
    rendered: HashMap<TileId, CachedTile>,
    /// Colours tiles are rendered with; tiles in other colours are re-rendered.
    palette: MapPalette,
    pois: HashMap<TileId, Arc<[Poi]>>,
    /// Last request sent to the worker, to avoid resending it on every pan.
    wanted: Vec<TileId>,
}

impl Tiles {
    /// The stream yields each tile when it is rendered; feed them back through
    /// [`Tiles::insert`].
    pub(super) fn new(
        palette: MapPalette,
    ) -> (Self, impl Stream<Item = RenderedTile> + Send + 'static) {
        let (state, rendered) = MapState::new(palette);
        let tiles = Self {
            state,
            rendered: HashMap::new(),
            palette,
            pois: HashMap::new(),
            wanted: Vec::new(),
        };
        (tiles, rendered)
    }

    pub(super) fn palette(&self) -> MapPalette {
        self.palette
    }

    /// Switches the colours tiles are rendered with. Returns whether they changed,
    /// in which case tiles on screen stay until [`Tiles::request`] re-renders them.
    pub(super) fn set_palette(&mut self, palette: MapPalette) -> bool {
        if palette == self.palette {
            return false;
        }
        self.palette = palette;
        // Before the next request, which the worker then renders with it.
        self.state.set_palette(palette);
        true
    }

    pub(super) fn insert(&mut self, rendered: RenderedTile) {
        let cached = CachedTile {
            image: rendered.image,
            palette: rendered.palette,
        };
        self.rendered.insert(rendered.tile, cached);
        self.pois.insert(rendered.tile, rendered.pois);
    }

    /// Asks the worker for the tiles `camera` shows that are not rendered yet.
    pub(super) fn request(&mut self, camera: &Camera, viewport: Size) {
        let missing: Vec<TileId> = camera
            .visible_tiles(viewport)
            .into_iter()
            .map(|tile| tile.id)
            .filter(|id| {
                self.rendered
                    .get(id)
                    .is_none_or(|cached| cached.is_stale(&self.palette))
            })
            .collect();

        if missing != self.wanted {
            self.state.want(missing.clone());
            self.wanted = missing;
        }
    }

    /// A tile's picture, if it has been rendered.
    pub(super) fn image(&self, tile: TileId) -> Option<&Handle> {
        self.rendered.get(&tile)?.image.as_ref()
    }

    /// The closest loaded lower-zoom tile that covers `tile`.
    pub(super) fn loaded_ancestor(&self, tile: TileId) -> Option<(TileId, &Handle)> {
        (1..=MAX_FALLBACK_LEVELS)
            .filter_map(|levels| tile.ancestor(levels))
            .find_map(|id| Some((id, self.image(id)?)))
    }

    /// The POI under `point`: the closest marker within `POI_HIT_RADIUS`, or else the
    /// closest POI without one within `HIDDEN_POI_HIT_RADIUS`.
    ///
    /// Markers win, so important POIs stay easy to pick where POIs are dense. POIs
    /// thinned out of the markers can still be picked from `POI_MIN_ZOOM`, since the
    /// tiles in use then hold all of them.
    pub(super) fn poi_at(&self, camera: &Camera, viewport: Size, point: Point) -> Option<Poi> {
        if camera.zoom() < POI_MIN_ZOOM {
            return None;
        }

        closest_poi(self.drawn_pois(camera, viewport), point, POI_HIT_RADIUS)
            .or_else(|| {
                closest_poi(
                    self.pois_in_view(camera, viewport, u32::MAX),
                    point,
                    HIDDEN_POI_HIT_RADIUS,
                )
            })
            .cloned()
    }

    /// POIs to draw as markers, with their position in the viewport: none below
    /// `POI_MIN_ZOOM`, then the most important first.
    pub(super) fn drawn_pois(&self, camera: &Camera, viewport: Size) -> Vec<(&Poi, Point)> {
        match max_poi_rank(camera.zoom()) {
            Some(max_rank) => self.pois_in_view(camera, viewport, max_rank),
            None => Vec::new(),
        }
    }

    /// Loaded POIs of at most `max_rank`, with their position in the viewport.
    fn pois_in_view(&self, camera: &Camera, viewport: Size, max_rank: u32) -> Vec<(&Poi, Point)> {
        let bounds = Rectangle::with_size(viewport);

        camera
            .visible_tiles(viewport)
            .iter()
            .filter_map(|tile| self.pois.get(&tile.id))
            .flat_map(|pois| pois.iter())
            .filter(|poi| poi.rank <= max_rank)
            .map(|poi| (poi, camera.to_screen(poi.position, viewport)))
            .filter(|(_, position)| bounds.contains(*position))
            .collect()
    }
}

/// The POI closest to `point` within `radius` pixels.
fn closest_poi(pois: Vec<(&Poi, Point)>, point: Point, radius: f32) -> Option<&Poi> {
    pois.into_iter()
        .map(|(poi, position)| (poi, position.distance(point)))
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(poi, _)| poi)
}

/// Highest POI rank drawn at `zoom`, following GNOME Maps' density limits, or
/// `None` when no POI is drawn.
fn max_poi_rank(zoom: f64) -> Option<u32> {
    if zoom < POI_MIN_ZOOM {
        None
    } else if zoom < 17.0 {
        Some(50)
    } else {
        Some(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::WorldPoint;

    fn poi(name: &str) -> Poi {
        Poi::new(name, WorldPoint::new(0.5, 0.5))
    }

    #[test]
    fn the_closest_poi_within_the_radius_is_picked() {
        let (near, far) = (poi("Louvre"), poi("Palais Royal"));
        let pois = vec![(&far, Point::new(8.0, 0.0)), (&near, Point::new(3.0, 0.0))];

        let picked = closest_poi(pois.clone(), Point::ORIGIN, 10.0);
        assert_eq!(picked.map(|poi| poi.name.as_str()), Some("Louvre"));
        // Nothing within three pixels of the second one's far side.
        assert!(closest_poi(pois, Point::new(0.0, 50.0), 10.0).is_none());
    }

    #[test]
    fn pois_are_drawn_from_zoom_15_and_thin_out_below_17() {
        assert_eq!(max_poi_rank(14.9), None);
        assert_eq!(max_poi_rank(15.0), Some(50));
        assert_eq!(max_poi_rank(16.9), Some(50));
        assert_eq!(max_poi_rank(17.0), Some(100));
    }
}
