// SPDX-License-Identifier: MPL-2.0

//! Map colours derived from the active COSMIC theme, so light and dark mode, high
//! contrast and custom themes carry over to the map.

use cosmic::cosmic_theme::Theme;
use cosmic::iced::Color;
use maplibre::style::Style;
use maplibre::style::layer::{LayerPaint, StyleProperty};

/// Map colours. Neutral features blend the theme background towards its text
/// colour, which is light in dark themes and dark in light ones, so the same
/// amounts give a dark map in dark mode and a light map in light mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapPalette {
    /// Land, which shows through tiles wherever nothing else is drawn.
    pub land: Color,
    pub landuse: Color,
    /// Tiles not loaded yet.
    pub placeholder: Color,
    pub building: Color,
    pub road: Color,
    pub boundary: Color,
    pub vegetation: Color,
    pub water: Color,
    pub poi_marker: Color,
    pub poi_outline: Color,
}

impl MapPalette {
    pub fn from_theme(theme: &Theme) -> Self {
        let land = Color::from(theme.bg_color());
        let text = Color::from(theme.on_bg_color());
        let neutral = |amount| mix(land, text, amount);

        Self {
            land,
            landuse: neutral(0.05),
            placeholder: neutral(0.08),
            building: neutral(0.12),
            road: neutral(0.28),
            boundary: neutral(0.6),
            vegetation: mix(land, Color::from(theme.palette.bright_green), 0.18),
            water: mix(land, Color::from(theme.palette.accent_blue), 0.25),
            poi_marker: neutral(0.75),
            poi_outline: land,
        }
    }

    /// The tile style: maplibre's default layers, coloured from this palette.
    pub fn style(&self) -> Style {
        let mut style = Style::default();

        for layer in &mut style.layers {
            let Some(color) = self.layer_color(&layer.id) else {
                continue;
            };
            let property = match &mut layer.paint {
                Some(LayerPaint::Fill(paint)) => &mut paint.fill_color,
                Some(LayerPaint::Line(paint)) => &mut paint.line_color,
                _ => continue,
            };
            if let Some(StyleProperty::Constant(value)) = property {
                *value = hex(color).parse().expect("hex colours parse");
            }
        }

        style
    }

    /// The colour of a style layer, by id, for the layers drawn with this palette.
    fn layer_color(&self, id: &str) -> Option<Color> {
        Some(match id {
            "park" | "landcover" => self.vegetation,
            "landuse" => self.landuse,
            "building" => self.building,
            "water" | "waterway" => self.water,
            "transportation" => self.road,
            "boundary" => self.boundary,
            _ => return None,
        })
    }
}

/// `from` moved `amount` (0..1) of the way towards `to`, in sRGB.
fn mix(from: Color, to: Color, amount: f32) -> Color {
    let lerp = |a: f32, b: f32| a + (b - a) * amount;
    Color::from_rgb(lerp(from.r, to.r), lerp(from.g, to.g), lerp(from.b, to.b))
}

/// `#rrggbb`, the form maplibre styles read.
fn hex(color: Color) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", channel(color.r), channel(color.g), channel(color.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color) -> f32 {
        color.r + color.g + color.b
    }

    #[test]
    fn palettes_follow_the_theme() {
        for theme in [Theme::dark_default(), Theme::light_default()] {
            let palette = MapPalette::from_theme(&theme);

            assert_eq!(palette.land, Color::from(theme.bg_color()));
            for color in [palette.landuse, palette.building, palette.road, palette.water] {
                assert_ne!(color, palette.land);
            }
        }

        let dark = MapPalette::from_theme(&Theme::dark_default());
        let light = MapPalette::from_theme(&Theme::light_default());
        assert!(luminance(dark.land) < luminance(light.land));
        // Roads stand out from the land in both.
        assert!(luminance(dark.road) > luminance(dark.land));
        assert!(luminance(light.road) < luminance(light.land));
    }

    #[test]
    fn style_keeps_layers_and_uses_palette() {
        let palette = MapPalette::from_theme(&Theme::dark_default());
        let style = palette.style();

        let ids = |style: &Style| -> Vec<String> {
            style.layers.iter().map(|layer| layer.id.clone()).collect()
        };
        assert_eq!(ids(&style), ids(&Style::default()));

        for layer in &style.layers {
            let Some(expected) = palette.layer_color(&layer.id) else {
                continue;
            };
            let color = match &layer.paint {
                Some(LayerPaint::Fill(paint)) => paint.fill_color.as_ref(),
                Some(LayerPaint::Line(paint)) => paint.line_color.as_ref(),
                _ => None,
            };
            let Some(StyleProperty::Constant(color)) = color else {
                panic!("{} has no constant colour", layer.id);
            };
            assert_eq!(color.to_hex_string(), hex(expected), "{}", layer.id);
        }
    }
}
