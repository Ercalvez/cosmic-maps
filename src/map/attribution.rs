// SPDX-License-Identifier: MPL-2.0

//! The credits for the map's data, shown over the map as its licences require.

use cosmic::iced::Alignment;
use cosmic::prelude::*;
use cosmic::widget;

/// One credit, linking to the page it names.
pub struct Credit {
    pub label: &'static str,
    pub url: &'static str,
}

/// What OpenFreeMap asks of apps that do not use MapLibre GL: its own name, the
/// OpenMapTiles schema, and the OpenStreetMap data, which its licence (ODbL)
/// requires to be credited where the map is shown. Search results come from
/// OpenStreetMap too, so this covers them.
pub const CREDITS: [Credit; 3] = [
    Credit {
        label: "OpenFreeMap",
        url: "https://openfreemap.org",
    },
    Credit {
        label: "© OpenMapTiles",
        url: "https://www.openmaptiles.org/",
    },
    Credit {
        label: "© OpenStreetMap contributors",
        url: "https://www.openstreetmap.org/copyright",
    },
];

/// The credits, as links that send `open_url` with their address.
pub fn view<'a, M: Clone + 'static>(open_url: impl Fn(String) -> M) -> Element<'a, M> {
    let spacing = cosmic::theme::spacing();

    let mut credits = widget::row::with_capacity(2 * CREDITS.len())
        .spacing(spacing.space_xxs)
        .align_y(Alignment::Center);
    for (index, credit) in CREDITS.iter().enumerate() {
        if index > 0 {
            credits = credits.push(widget::text::caption("·"));
        }
        credits = credits.push(
            widget::button::link(credit.label)
                .padding(0)
                .on_press(open_url(credit.url.to_owned())),
        );
    }

    // A card, so the credits stay readable over any part of the map.
    widget::container(credits)
        .padding([spacing.space_xxxs, spacing.space_xxs])
        .class(cosmic::theme::Container::Card)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_licence_is_credited_with_a_link() {
        assert!(
            CREDITS
                .iter()
                .all(|credit| credit.url.starts_with("https://"))
        );
        // ODbL: "© OpenStreetMap contributors", linking to the copyright page.
        assert!(CREDITS.iter().any(|credit| {
            credit.label.contains("OpenStreetMap")
                && credit.url == "https://www.openstreetmap.org/copyright"
        }));
    }
}
