// SPDX-License-Identifier: MPL-2.0

//! The place panel: the place selected on the map, in the context drawer.

use cosmic::prelude::*;
use cosmic::widget::{self, icon};

use crate::app::Message;
use crate::fl;
use crate::map::poi::Poi;
use crate::map::view::{DetailsState, Place};
use crate::services::nominatim::{PoiDetails, Wheelchair};

/// The panel's content; the drawer provides the close button and the scrolling.
pub fn view(place: Place<'_>) -> Element<'_, Message> {
    match place {
        Place::Selected { poi, details } => selected_view(poi, details),
        Place::LookingUp => widget::text::caption(fl!("poi-looking-up")).into(),
    }
}

fn selected_view<'a>(poi: &'a Poi, details: Option<&'a DetailsState>) -> Element<'a, Message> {
    let spacing = cosmic::theme::spacing();

    // Translated name first, with the local name below as a caption, the way
    // libcosmic's settings items pair a title with its description.
    let names = match &poi.translated_name {
        Some(translated) => widget::column::with_capacity(3)
            .push(widget::text::heading(translated))
            .push(widget::text::caption(&poi.name)),
        None => widget::column::with_capacity(2).push(widget::text::heading(&poi.name)),
    };
    let names = names
        .push_maybe(poi.category().map(widget::text::caption))
        .spacing(2);

    let directions = widget::button::suggested(fl!("directions")).on_press(Message::OpenDirections);

    let details: Option<Element<'a, Message>> = match details {
        Some(DetailsState::Loading) => {
            Some(widget::text::caption(fl!("poi-details-loading")).into())
        }
        Some(DetailsState::Loaded(details)) => details_rows(details),
        Some(DetailsState::Unavailable) | None => None,
    };

    widget::column::with_capacity(4)
        .push(names)
        .push(directions)
        .push_maybe(details.is_some().then(widget::divider::horizontal::default))
        .push_maybe(details)
        .spacing(spacing.space_xs)
        .into()
}

/// One row per known detail, or `None` if there are none.
fn details_rows<'a>(details: &'a PoiDetails) -> Option<Element<'a, Message>> {
    let spacing = cosmic::theme::spacing();
    let row = |icon_name: &'static str, content: Element<'a, Message>| -> Element<'a, Message> {
        widget::row::with_capacity(2)
            .push(icon::from_name(icon_name).size(16).icon())
            .push(content)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(spacing.space_xs)
            .into()
    };
    let link = |label: &'a str, url: String| -> Element<'a, Message> {
        // `link` needs an owned label.
        widget::button::link(label.to_owned())
            .padding(0)
            .on_press(Message::LaunchUrl(url))
            .into()
    };

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    if let Some(address) = &details.address {
        rows.push(row(
            "mark-location-symbolic",
            widget::text::body(address).into(),
        ));
    }
    if let Some(hours) = &details.opening_hours {
        rows.push(row(
            "document-open-recent-symbolic",
            widget::text::body(hours).into(),
        ));
    }
    if let Some(website) = &details.website {
        rows.push(row("web-browser-symbolic", link(website, website.clone())));
    }
    if let Some(phone) = &details.phone {
        let uri = format!("tel:{}", phone.replace(' ', ""));
        rows.push(row("call-start-symbolic", link(phone, uri)));
    }
    if let Some(wheelchair) = details.wheelchair {
        let label = match wheelchair {
            Wheelchair::Yes => fl!("poi-wheelchair-yes"),
            Wheelchair::Limited => fl!("poi-wheelchair-limited"),
            Wheelchair::No => fl!("poi-wheelchair-no"),
        };
        rows.push(row(
            "preferences-desktop-accessibility-symbolic",
            widget::text::body(label).into(),
        ));
    }

    (!rows.is_empty()).then(|| {
        widget::column::with_children(rows)
            .spacing(spacing.space_xxs)
            .into()
    })
}
