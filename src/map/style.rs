//! Light and dark map styles, following COSMIC's dark mode.

use cosmic::iced::Color;
use maplibre::style::Style;
use maplibre::style::layer::{LayerPaint, StyleProperty};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTheme {
    Light,
    Dark,
}

/// Dark colours by style layer id. Layers not listed keep their light colour.
const DARK_PALETTE: [(&str, &str); 8] = [
    ("park", "#24392a"),
    ("landcover", "#24392a"),
    ("landuse", "#2e2e2e"),
    ("building", "#3a3735"),
    ("water", "#1d3442"),
    ("waterway", "#1d3442"),
    ("transportation", "#4d4d4d"),
    ("boundary", "#8a8a8a"),
];

impl MapTheme {
    pub fn from_dark(is_dark: bool) -> Self {
        if is_dark { Self::Dark } else { Self::Light }
    }

    /// The tile style. Both themes have the same layers, only their colours differ.
    pub fn style(self) -> Style {
        let mut style = Style::default();
        if self == Self::Light {
            return style;
        }

        for layer in &mut style.layers {
            let Some((_, hex)) = DARK_PALETTE.iter().find(|(id, _)| *id == layer.id) else {
                continue;
            };
            let color = match &mut layer.paint {
                Some(LayerPaint::Fill(paint)) => &mut paint.fill_color,
                Some(LayerPaint::Line(paint)) => &mut paint.line_color,
                _ => continue,
            };
            if let Some(StyleProperty::Constant(color)) = color {
                *color = hex.parse().expect("palette colours are valid");
            }
        }

        style
    }

    /// Land, which shows through tiles wherever nothing else is drawn.
    pub fn background(self) -> Color {
        match self {
            Self::Light => Color::from_rgb(0.85, 0.85, 0.85),
            Self::Dark => Color::from_rgb8(0x24, 0x24, 0x24),
        }
    }

    /// Tiles not loaded yet.
    pub fn placeholder(self) -> Color {
        match self {
            Self::Light => Color::from_rgb(0.78, 0.78, 0.78),
            Self::Dark => Color::from_rgb8(0x30, 0x30, 0x30),
        }
    }

    /// POI markers, and their outline.
    pub fn poi_marker(self) -> (Color, Color) {
        match self {
            Self::Light => (Color::from_rgb(0.33, 0.33, 0.38), Color::WHITE),
            Self::Dark => (Color::from_rgb8(0xc8, 0xc8, 0xcc), Color::from_rgb8(0x24, 0x24, 0x24)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(style: &Style) -> Vec<(String, Option<String>)> {
        style
            .layers
            .iter()
            .map(|layer| {
                let color = match &layer.paint {
                    Some(LayerPaint::Fill(paint)) => paint.fill_color.as_ref(),
                    Some(LayerPaint::Line(paint)) => paint.line_color.as_ref(),
                    _ => None,
                };
                let color = color.and_then(|color| match color {
                    StyleProperty::Constant(color) => Some(color.to_hex_string()),
                    StyleProperty::Expression(_) => None,
                });
                (layer.id.clone(), color)
            })
            .collect()
    }

    #[test]
    fn dark_style_only_changes_colours() {
        let light = colors(&MapTheme::Light.style());
        let dark = colors(&MapTheme::Dark.style());

        let ids = |colors: &[(String, Option<String>)]| -> Vec<String> {
            colors.iter().map(|(id, _)| id.clone()).collect()
        };
        assert_eq!(ids(&light), ids(&dark));

        for (id, hex) in DARK_PALETTE {
            let light_color = light.iter().find(|(layer, _)| layer == id).unwrap();
            let dark_color = dark.iter().find(|(layer, _)| layer == id).unwrap();
            assert_eq!(dark_color.1.as_deref(), Some(hex), "{id}");
            assert_ne!(light_color.1, dark_color.1, "{id}");
        }
    }
}
