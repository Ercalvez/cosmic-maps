use std::sync::mpsc::{Receiver, Sender};

use super::camera::TileId;

pub struct MapState {
    renderer_tx: Sender<MapCommand>,
    tile_rx: Receiver<RenderedTile>,
}

pub struct MapImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Result of rendering one tile; `image` is `None` when the tile could not be loaded.
pub struct RenderedTile {
    pub tile: TileId,
    pub image: Option<MapImage>,
}

pub enum MapCommand {
    /// Tiles to render, in priority order. Replaces any earlier request.
    Want(Vec<TileId>),
}

impl MapState {
    pub fn new() -> Self {
        let (renderer_tx, renderer_rx) = std::sync::mpsc::channel();
        let (tile_tx, tile_rx) = std::sync::mpsc::channel();

        crate::map::worker::spawn(renderer_rx, tile_tx);

        Self {
            renderer_tx,
            tile_rx,
        }
    }

    pub fn want(&self, tiles: Vec<TileId>) {
        let _ = self.renderer_tx.send(MapCommand::Want(tiles));
    }

    pub fn try_tile(&self) -> Option<RenderedTile> {
        self.tile_rx.try_recv().ok()
    }
}
