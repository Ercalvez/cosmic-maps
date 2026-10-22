use cosmic::iced::Length;
use cosmic::iced::widget::image::Handle;
use cosmic::prelude::*;
use cosmic::widget;

use super::state::MapState;

pub struct MapView {
    state: MapState,
    image: Option<Handle>,
}

impl MapView {
    pub fn new() -> Self {
        Self {
            state: MapState::new(),
            image: None
        }
    }

    pub fn pan(&self, dx: f64, dy: f64) {
        self.state.pan(dx, dy);
    }

    pub fn zoom(&self, level: f64) {
        self.state.zoom(level);
    }

    pub fn redraw(&self) {
        self.state.redraw();
    }

     pub fn update_image(&mut self) {
        if let Some(image) = self.state.try_image() {
            self.image = Some(
                Handle::from_rgba(
                    image.width,
                    image.height,
                    image.pixels,
                )
            );
        }
    }
    
    pub fn view<Message>(&self) -> Element<'_, Message>
    where
        Message: 'static,
    {
        match &self.image {
            Some(handle) => {
                widget::image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }

            None => {
                widget::container(
                    widget::text("Loading map...")
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        }
    }
}