use std::borrow::Cow;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};

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

use super::state::{MapCommand, MapImage};

const TILE_SIZE: u32 = 512;

/// Tile shown until camera handling exists: Berlin at zoom 12. The default tile
/// source of maplibre-rs only serves Germany.
const INITIAL_TILE: (i32, i32, u8) = (2200, 1343, 12);

pub fn spawn(rx: Receiver<MapCommand>, tx: Sender<MapImage>) {
    // `HeadlessMap` is not `Send`, so it lives on its own thread with its own runtime.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create map runtime");

        runtime.block_on(run(rx, tx));
    });
}

async fn run(rx: Receiver<MapCommand>, tx: Sender<MapImage>) {
    let style = Style::default();
    let mut map = create_map(style.clone(), tx).await;
    println!("MapLibre initialized");

    let (x, y, z) = INITIAL_TILE;
    let coords = WorldTileCoords::from((x, y, z.into()));

    // Blocking here is fine: this thread only serves the map.
    while let Ok(command) = rx.recv() {
        match command {
            MapCommand::Redraw => render_tile(&mut map, &style, coords).await,

            MapCommand::Resize(_, _) => {
                // TODO: recreate the renderer with the new size
            }

            MapCommand::Pan(_, _) => {
                // TODO: camera update
            }

            MapCommand::Zoom(_) => {
                // TODO: zoom update
            }
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

async fn render_tile(map: &mut HeadlessMap, style: &Style, coords: WorldTileCoords) {
    let tile = match map.fetch_tile(coords).await {
        Ok(tile) => tile,
        Err(err) => {
            eprintln!("failed to fetch tile {coords}: {err:?}");
            return;
        }
    };

    let mut layers = Vec::new();
    for layer in style.layers.iter().filter(|l| l.source_layer.is_some()) {
        layers.extend(map.process_tile(tile.clone(), layer).await);
    }

    // Runs the schedule, which ends with `CaptureFrameSystem` sending the image.
    map.render_tile(layers);
}

/// Sends every rendered frame back to the UI thread.
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
