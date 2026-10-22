use std::sync::mpsc::{Receiver, Sender};

pub struct MapState {
    renderer_tx: Sender<MapCommand>,
    image_rx: Receiver<MapImage>,
}

pub struct MapImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub enum MapCommand {
    Resize(u32, u32),
    Pan(f64, f64),
    Zoom(f64),
    Redraw,
}

impl MapState {
    pub fn new() -> Self {
        let (renderer_tx, renderer_rx) = std::sync::mpsc::channel();
        let (image_tx, image_rx) = std::sync::mpsc::channel();

        crate::map::worker::spawn(renderer_rx, image_tx);

        Self {
            renderer_tx,
            image_rx,
        }
    }

    pub fn pan(&self, dx: f64, dy: f64) {
        let _ = self.renderer_tx.send(MapCommand::Pan(dx, dy));
    }

    pub fn zoom(&self, level: f64) {
        let _ = self.renderer_tx.send(MapCommand::Zoom(level));
    }

    pub fn try_image(&self) -> Option<MapImage> {
        self.image_rx.try_recv().ok()
    }

    pub fn redraw(&self) {
        let _ = self.renderer_tx.send(MapCommand::Redraw);
    }
}