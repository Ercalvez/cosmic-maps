// SPDX-License-Identifier: MPL-2.0

//! Renders tiles with MapLibre on a thread of its own.

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};

use cosmic::iced::futures::channel::mpsc::UnboundedSender;
use cosmic::iced::widget::image::Handle;

use maplibre::{
    context::MapContext,
    coords::WorldTileCoords,
    headless::{
        HeadlessPlugin, create_headless_renderer, environment::HeadlessEnvironment,
        map::HeadlessMap,
    },
    kernel::Kernel,
    plugin::Plugin,
    raster::{DefaultRasterTransferables, RasterPlugin},
    render::{RenderPlugin, RenderStageLabel, graph::RenderGraph, resource::Head},
    schedule::Schedule,
    style::Style,
    tcs::{
        system::{System, SystemContainer, SystemError, SystemResult},
        world::World,
    },
    vector::{DefaultVectorTransferables, VectorPlugin},
};

use super::camera::TileId;
use super::poi::{Poi, parse_pois, system_languages};
use super::style::MapPalette;
use super::state::{MapCommand, RenderedTile};

const TILE_SIZE: u32 = super::camera::TILE_SIZE as u32;

/// A frame read back from the GPU, as RGBA pixels.
struct MapImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

pub fn spawn(rx: Receiver<MapCommand>, tx: UnboundedSender<RenderedTile>, palette: MapPalette) {
    // `HeadlessMap` is not `Send`, so it lives on its own thread with its own runtime.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create map runtime");

        runtime.block_on(run(rx, tx, palette));
    });
}

async fn run(rx: Receiver<MapCommand>, tx: UnboundedSender<RenderedTile>, mut palette: MapPalette) {
    let languages = system_languages();
    let (frame_tx, frame_rx) = std::sync::mpsc::channel();
    let mut style = palette.style();
    // Palettes only change colours, so the map keeps this style's layers.
    let mut map = create_map(style.clone(), frame_tx).await;
    tracing::debug!("MapLibre initialized");

    let mut queue: Vec<TileId> = Vec::new();

    loop {
        // Block while idle. Blocking is fine: this thread only serves the map.
        if queue.is_empty() {
            let Ok(command) = rx.recv() else { break };
            apply(command, &mut queue, &mut palette, &mut style);
        }

        // Pick up newer requests so tiles that scrolled away are dropped.
        while let Ok(command) = rx.try_recv() {
            apply(command, &mut queue, &mut palette, &mut style);
        }

        if queue.is_empty() {
            continue;
        }
        let tile = queue.remove(0);

        let (image, pois) = match render_tile(&mut map, &style, tile, &languages).await {
            Some(pois) => (frame_rx.try_recv().ok(), pois),
            None => (None, Vec::new()),
        };
        // Built here so the UI thread receives a cheaply clonable handle.
        let image = image.map(|image| Handle::from_rgba(image.width, image.height, image.pixels));

        let rendered = RenderedTile {
            tile,
            image,
            pois: pois.into(),
            palette,
        };
        if tx.unbounded_send(rendered).is_err() {
            break;
        }
    }
}

fn apply(command: MapCommand, queue: &mut Vec<TileId>, palette: &mut MapPalette, style: &mut Style) {
    match command {
        MapCommand::Want(tiles) => *queue = tiles,
        MapCommand::SetPalette(new_palette) => {
            *palette = new_palette;
            *style = new_palette.style();
        }
    }
}

async fn create_map(style: Style, tx: Sender<MapImage>) -> HeadlessMap {
    let (kernel, renderer) = create_headless_renderer(TILE_SIZE, TILE_SIZE, None).await;

    let plugins: Vec<Box<dyn Plugin<_>>> = vec![
        Box::new(RenderPlugin::default()),
        Box::new(VectorPlugin::<DefaultVectorTransferables>::default()),
        Box::new(RasterPlugin::<DefaultRasterTransferables>::default()),
        // Copies the rendered texture into a CPU-readable buffer.
        Box::new(HeadlessPlugin::new(false)),
        // Must come after `HeadlessPlugin` so it reads the copied frame.
        Box::new(FrameCapturePlugin { tx }),
    ];

    HeadlessMap::new(style, renderer, kernel, plugins).expect("failed to create map")
}

/// Renders a tile and returns its POIs, or `None` if it could not be fetched. The
/// frame is sent to the channel given to `FrameCapturePlugin`.
async fn render_tile(
    map: &mut HeadlessMap,
    style: &Style,
    tile: TileId,
    languages: &[String],
) -> Option<Vec<Poi>> {
    let coords = WorldTileCoords::from((tile.x, tile.y, tile.z.into()));
    let data = match map.fetch_tile(coords).await {
        Ok(data) => data,
        Err(err) => {
            tracing::warn!("failed to fetch tile {coords}: {err:?}");
            return None;
        }
    };

    let mut layers = Vec::new();
    for layer in style.layers.iter().filter(|l| l.source_layer.is_some()) {
        layers.extend(map.process_tile(data.clone(), layer).await);
    }

    // Runs the schedule, which ends with `CaptureFrameSystem` sending the image.
    map.render_tile(layers);
    Some(parse_pois(&data, tile, languages))
}

/// Sends every rendered frame through `tx`.
struct FrameCapturePlugin {
    tx: Sender<MapImage>,
}

impl Plugin<HeadlessEnvironment> for FrameCapturePlugin {
    fn build(
        &self,
        schedule: &mut Schedule,
        _kernel: Rc<Kernel<HeadlessEnvironment>>,
        _world: &mut World,
        _graph: &mut RenderGraph,
    ) {
        schedule.add_system_to_stage(
            RenderStageLabel::Cleanup,
            SystemContainer::new(CaptureFrameSystem {
                tx: self.tx.clone(),
            }),
        );
    }
}

struct CaptureFrameSystem {
    tx: Sender<MapImage>,
}

impl System for CaptureFrameSystem {
    fn name(&self) -> Cow<'static, str> {
        "capture_frame".into()
    }

    fn run(&mut self, context: &mut MapContext) -> SystemResult {
        let renderer = &context.renderer;
        let surface = renderer.resources.surface();
        let Head::Headless(buffered_texture) = surface.head() else {
            return Err(SystemError::Setup);
        };

        let width = surface.size().width();
        let height = surface.size().height();
        let unpadded_bytes_per_row = width as usize * 4;

        let buffer_slice = buffered_texture.map_async(&renderer.device);
        let padded_buffer = buffer_slice.get_mapped_range();

        // Rows are padded to wgpu's copy alignment; keep only the pixel data.
        let mut pixels = Vec::with_capacity(unpadded_bytes_per_row * height as usize);
        for row in padded_buffer
            .chunks(buffered_texture.bytes_per_row() as usize)
            .take(height as usize)
        {
            pixels.extend_from_slice(&row[..unpadded_bytes_per_row]);
        }

        // All mapped views must be dropped before unmapping.
        drop(padded_buffer);
        buffered_texture.unmap();

        let _ = self.tx.send(MapImage {
            width,
            height,
            pixels,
        });

        Ok(())
    }
}
