// SPDX-License-Identifier: MPL-2.0

//! The channel between the map view and the rendering worker.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use cosmic::iced::futures::channel::mpsc::{self, UnboundedReceiver};
use cosmic::iced::widget::image::Handle;

use super::camera::TileId;
use super::poi::Poi;
use super::style::MapPalette;

pub struct MapState {
    renderer_tx: Sender<MapCommand>,
}

/// Result of rendering one tile; `image` is `None` when the tile could not be loaded.
#[derive(Debug, Clone)]
pub struct RenderedTile {
    pub tile: TileId,
    pub image: Option<Handle>,
    /// Named POIs in the tile. Shared, so cloning the message stays cheap.
    pub pois: Arc<[Poi]>,
    /// The palette the image was rendered with.
    pub palette: MapPalette,
}

pub enum MapCommand {
    /// Tiles to render, in priority order. Replaces any earlier request.
    Want(Vec<TileId>),
    /// Render with these colours from now on.
    SetPalette(MapPalette),
}

impl MapState {
    /// Starts the worker, rendering with `palette`. The receiver yields tiles as
    /// soon as they are rendered.
    pub fn new(palette: MapPalette) -> (Self, UnboundedReceiver<RenderedTile>) {
        let (renderer_tx, renderer_rx) = std::sync::mpsc::channel();
        // An async channel, so the UI can await tiles instead of polling for them.
        let (tile_tx, tile_rx) = mpsc::unbounded();

        crate::map::worker::spawn(renderer_rx, tile_tx, palette);

        (Self { renderer_tx }, tile_rx)
    }

    pub fn want(&self, tiles: Vec<TileId>) {
        let _ = self.renderer_tx.send(MapCommand::Want(tiles));
    }

    pub fn set_palette(&self, palette: MapPalette) {
        let _ = self.renderer_tx.send(MapCommand::SetPalette(palette));
    }
}
