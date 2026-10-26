// SPDX-License-Identifier: MPL-2.0

//! How routes are shown in the directions drawer: the options found, the legs of
//! the chosen one, and turn-by-turn steps.

use std::collections::HashSet;

use cosmic::iced::widget::{rich_text, span};
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::prelude::*;
use cosmic::widget::{self, icon};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{Message, RouteChoice};
use crate::domain::route::{Itinerary, Leg, Plan, Planned, RouteMode, Step};
use crate::fl;

const TRANSITOUS_SOURCES: &str = "https://transitous.org/sources/";
const OSM_COPYRIGHT: &str = "https://www.openstreetmap.org/copyright";

/// Names for the trip's ends, which MOTIS calls `START` and `END`.
#[derive(Clone, Copy)]
pub(super) struct Ends<'a> {
    pub start: &'a str,
    pub destination: &'a str,
}

/// The routes of a plan. Walking, cycling and driving show their single route in
/// full; transit lists the options, with the legs of the chosen one under it. `open_stops` holds the
/// legs of the chosen option whose stops are listed.
pub(super) fn plan_view<'a>(
    plan: &'a Plan,
    mode: RouteMode,
    chosen: Option<RouteChoice>,
    open_stops: &HashSet<usize>,
    ends: Ends<'a>,
) -> Element<'a, Message> {
    let spacing = cosmic::theme::spacing();

    if mode != RouteMode::Transit {
        let Some(direct) = &plan.direct else {
            return widget::column::with_capacity(0).into();
        };
        let summary = [
            format_duration(direct.duration),
            format_distance(street_distance(direct)),
        ];
        return widget::column::with_capacity(2)
            .push(widget::text::heading(summary.join(" · ")))
            .push_maybe(steps_view(direct))
            .spacing(spacing.space_s)
            .into();
    }

    let mut options =
        widget::column::with_capacity(2 * plan.transit.len() + 2).spacing(spacing.space_xs);
    // In transit plans, the route all the way by street is walking.
    if let Some(walk) = &plan.direct {
        let is_chosen = chosen == Some(RouteChoice::Direct);
        options = options
            .push(option(walk_option(walk), RouteChoice::Direct, is_chosen))
            .push_maybe(is_chosen.then(|| steps_view(walk)).flatten());
    }
    for (index, itinerary) in plan.transit.iter().enumerate() {
        let choice = RouteChoice::Transit(index);
        let is_chosen = chosen == Some(choice);
        options = options
            .push(option(transit_option(itinerary), choice, is_chosen))
            .push_maybe(is_chosen.then(|| legs_view(itinerary, open_stops, &ends)));
    }
    options.into()
}

/// Credits required by Transitous' usage policy, below the routes. Their links
/// send `open_url`.
pub fn attribution<'a, M: Clone + 'static>(open_url: impl Fn(String) -> M) -> Element<'a, M> {
    let link = |label: &'static str, url: &'static str| {
        widget::button::link(label)
            .padding(0)
            .on_press(open_url(url.to_owned()))
    };

    widget::row::with_capacity(4)
        .push(widget::text::caption(fl!("route-attribution")))
        .push(link("Transitous", TRANSITOUS_SOURCES))
        .push(widget::text::caption("·"))
        .push(link("OpenStreetMap", OSM_COPYRIGHT))
        .spacing(cosmic::theme::spacing().space_xxs)
        .align_y(Alignment::Center)
        .into()
}

/// A route option to choose, highlighted while chosen.
fn option(
    content: Element<'_, Message>,
    choice: RouteChoice,
    is_chosen: bool,
) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();
    let radius = cosmic::theme::active().cosmic().corner_radii.radius_s;

    widget::button::custom(content)
        .class(cosmic::theme::Button::ListItem(radius))
        .selected(is_chosen)
        .width(Length::Fill)
        .padding([spacing.space_xs, spacing.space_s])
        .on_press(Message::Choose(choice))
        .into()
}

/// "Walk only · 20 min", then the distance.
fn walk_option<'a>(walk: &Itinerary) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .push(heading_row(
            fl!("route-walk-only"),
            format_duration(walk.duration),
        ))
        .push(widget::text::caption(format_distance(street_distance(
            walk,
        ))))
        .spacing(2)
        .into()
}

/// Departure → arrival and duration, then the lines taken and the transfers.
fn transit_option(itinerary: &Itinerary) -> Element<'_, Message> {
    let (estimated, scheduled) = itinerary.durations();
    let heading = widget::row::with_capacity(5)
        .push(time_view(itinerary.start(), TextStyle::Heading))
        .push(widget::text::heading("→"))
        .push(time_view(itinerary.end(), TextStyle::Heading))
        .push(widget::space::horizontal())
        .push(duration_view(estimated, scheduled, TextStyle::Body))
        .spacing(cosmic::theme::spacing().space_xxs)
        .align_y(Alignment::Center);

    let mut lines = widget::row::with_capacity(2 * itinerary.legs.len())
        .spacing(cosmic::theme::spacing().space_xxs)
        .align_y(Alignment::Center);
    for (index, leg) in itinerary
        .legs
        .iter()
        .filter(|leg| leg.is_transit())
        .enumerate()
    {
        if index > 0 {
            lines = lines.push(widget::text::caption("›"));
        }
        lines = lines.push(badge(leg));
    }
    let lines = lines
        .push(widget::space::horizontal())
        .push(widget::text::caption(fl!(
            "route-transfers",
            count = itinerary.transfers
        )));

    widget::column::with_capacity(2)
        .push(heading)
        .push(lines)
        .spacing(cosmic::theme::spacing().space_xxs)
        .into()
}

/// A heading on the left and a value on the right, such as a duration.
fn heading_row<'a>(heading: String, value: String) -> Element<'a, Message> {
    widget::row::with_capacity(3)
        .push(widget::text::heading(heading))
        .push(widget::space::horizontal())
        .push(widget::text::body(value))
        .align_y(Alignment::Center)
        .into()
}

/// The legs of a transit option: walks between stops, and the lines taken.
fn legs_view<'a>(
    itinerary: &'a Itinerary,
    open_stops: &HashSet<usize>,
    ends: &Ends<'_>,
) -> Element<'a, Message> {
    let spacing = cosmic::theme::spacing();
    let legs: Vec<Element<'a, Message>> = itinerary
        .legs
        .iter()
        .enumerate()
        .map(|(index, leg)| leg_view(leg, index, open_stops.contains(&index), ends))
        .collect();

    widget::column::with_children(legs)
        .spacing(spacing.space_s)
        .padding([
            spacing.space_xxs,
            spacing.space_s,
            spacing.space_xs,
            spacing.space_m,
        ])
        .into()
}

/// One leg; for transit, `index` and `stops_open` are for its list of stops.
fn leg_view<'a>(
    leg: &'a Leg,
    index: usize,
    stops_open: bool,
    ends: &Ends<'_>,
) -> Element<'a, Message> {
    let at = |time: Timestamp| format_time(time);

    if leg.is_walk() {
        let place = stop_name(&leg.to.name, ends);
        let distance = format_distance(leg.distance.unwrap_or_default());
        let walk = fl!("route-leg-walk", distance = distance, place = place);
        let when = [at(leg.start_time), format_duration(leg.duration)].join(" · ");
        return widget::column::with_capacity(2)
            .push(widget::text::body(walk))
            .push(widget::text::caption(when))
            .spacing(2)
            .into();
    }

    let headsign = leg.line.as_ref().and_then(|line| line.headsign.clone());
    let line = widget::row::with_capacity(2)
        .push(badge(leg))
        .push_maybe(
            headsign
                .map(|headsign| widget::text::body(fl!("route-leg-towards", headsign = headsign))),
        )
        .spacing(cosmic::theme::spacing().space_xs)
        .align_y(Alignment::Center);

    let from = stop_name(&leg.from.name, ends);
    let to = stop_name(&leg.to.name, ends);
    let board = fl!("route-leg-board", stop = from);
    let alight = fl!("route-leg-alight", stop = to);

    widget::column::with_capacity(4)
        .push(line)
        .push(stop_time_view(board, leg.start()))
        .push(stops_view(leg, index, stops_open))
        .push(stop_time_view(alight, leg.end()))
        .spacing(2)
        .into()
}

/// "Board at Louvre - Rivoli · 21:47", with the schedule beside a late time.
fn stop_time_view<'a>(label: String, planned: Planned) -> Element<'a, Message> {
    widget::row::with_capacity(3)
        .push(widget::text::caption(label))
        .push(widget::text::caption("·"))
        .push(time_view(planned, TextStyle::Caption))
        .spacing(cosmic::theme::spacing().space_xxs)
        .align_y(Alignment::Center)
        .into()
}

/// How a time or duration estimated with real-time data compares to its schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deviation {
    Same,
    Late,
    Early,
}

impl Deviation {
    /// Of two values in minutes, as shown.
    fn of_minutes(estimated: i64, scheduled: i64) -> Self {
        match estimated.cmp(&scheduled) {
            std::cmp::Ordering::Equal => Self::Same,
            std::cmp::Ordering::Greater => Self::Late,
            std::cmp::Ordering::Less => Self::Early,
        }
    }

    fn of_times(planned: Planned) -> Self {
        let minutes = |time: Timestamp| time.as_second().div_euclid(60);
        Self::of_minutes(minutes(planned.estimated), minutes(planned.scheduled))
    }
}

/// The kind of text a time is shown in.
#[derive(Debug, Clone, Copy)]
enum TextStyle {
    Heading,
    Body,
    Caption,
}

/// A time as estimated: alone when on schedule, else in a warning colour with the
/// scheduled time struck through beside it.
fn time_view<'a>(planned: Planned, style: TextStyle) -> Element<'a, Message> {
    let deviation = Deviation::of_times(planned);
    versus_schedule(
        format_time(planned.estimated),
        format_time(planned.scheduled),
        deviation,
        style,
    )
}

/// A duration as estimated, in the same way, in seconds.
fn duration_view<'a>(estimated: u64, scheduled: u64, style: TextStyle) -> Element<'a, Message> {
    let minutes = |seconds: u64| i64::try_from(seconds.div_ceil(60)).unwrap_or(i64::MAX);
    versus_schedule(
        format_duration(estimated),
        format_duration(scheduled),
        Deviation::of_minutes(minutes(estimated), minutes(scheduled)),
        style,
    )
}

fn versus_schedule<'a>(
    estimated: String,
    scheduled: String,
    deviation: Deviation,
    style: TextStyle,
) -> Element<'a, Message> {
    let (size, line_height, font) = match style {
        TextStyle::Heading => (14.0, 21.0, cosmic::font::bold()),
        TextStyle::Body => (14.0, 21.0, cosmic::font::default()),
        TextStyle::Caption => (12.0, 17.0, cosmic::font::default()),
    };
    let cosmic = cosmic::theme::active();
    let color = match deviation {
        Deviation::Same => None,
        Deviation::Late => Some(Color::from(cosmic.cosmic().warning_color())),
        Deviation::Early => Some(Color::from(cosmic.cosmic().success_color())),
    };

    let mut spans = vec![span::<(), _>(estimated).font(font).color_maybe(color)];
    if deviation != Deviation::Same {
        spans.push(span("  ").font(font));
        spans.push(
            span(scheduled)
                .font(cosmic::font::default())
                .strikethrough(true),
        );
    }
    rich_text(spans)
        .size(size)
        .line_height(cosmic::iced::widget::text::LineHeight::Absolute(
            line_height.into(),
        ))
        .into()
}

/// "2 stops", which opens the list of the stops passed when there are any.
fn stops_view<'a>(leg: &'a Leg, index: usize, open: bool) -> Element<'a, Message> {
    let count = widget::text::caption(fl!("route-leg-stops", count = leg.stops.len()));
    if leg.stops.is_empty() {
        return count.into();
    }

    let chevron = if open {
        "go-down-symbolic"
    } else {
        "go-next-symbolic"
    };
    let toggle = widget::button::custom(
        widget::row::with_capacity(2)
            .push(icon::from_name(chevron).size(12).icon())
            .push(count)
            .spacing(cosmic::theme::spacing().space_xxs)
            .align_y(Alignment::Center),
    )
    .class(cosmic::theme::Button::Text)
    .padding(0)
    .on_press(Message::ToggleStops(index));

    let stops = open.then(|| {
        let rows: Vec<Element<'a, Message>> = leg
            .stops
            .iter()
            .map(|stop| {
                let line = match stop.arrival {
                    Some(arrival) => format!("{} · {}", format_time(arrival), stop.name),
                    None => stop.name.clone(),
                };
                widget::text::caption(line).into()
            })
            .collect();
        widget::column::with_children(rows).spacing(2).padding([
            2,
            0,
            2,
            cosmic::theme::spacing().space_m,
        ])
    });

    widget::column::with_capacity(2)
        .push(toggle)
        .push_maybe(stops)
        .spacing(2)
        .into()
}

/// The line of a transit leg, in the operator's colours when published.
fn badge<'a>(leg: &Leg) -> Element<'a, Message> {
    let line = leg.line.as_ref();
    let background = line.and_then(|line| line.color);
    let text = line
        .and_then(|line| line.text_color)
        .or_else(|| background.map(contrasting));

    let content = widget::container(widget::text::caption_heading(line_label(leg))).padding([1, 6]);
    match background {
        Some(background) => content
            .class(cosmic::theme::Container::custom(move |_| {
                widget::container::Style {
                    background: Some(Background::Color(background)),
                    text_color: text,
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..widget::container::Style::default()
                }
            }))
            .into(),
        None => content.class(cosmic::theme::Container::Card).into(),
    }
}

/// Black or white, whichever reads better on `background`.
fn contrasting(background: Color) -> Color {
    let luminance = 0.299 * background.r + 0.587 * background.g + 0.114 * background.b;
    if luminance > 0.6 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// "Metro 1", or the line name alone when its kind is unusual.
fn line_label(leg: &Leg) -> String {
    let name = leg
        .line
        .as_ref()
        .and_then(|line| line.name.clone())
        .unwrap_or_default();
    match transit_kind(&leg.mode) {
        Some(kind) => format!("{kind} {name}").trim_end().to_owned(),
        None => name,
    }
}

/// The trip's ends by the names the user sees, other stops by their own name.
fn stop_name(name: &str, ends: &Ends<'_>) -> String {
    match name {
        "START" => ends.start.to_owned(),
        "END" => ends.destination.to_owned(),
        name => name.to_owned(),
    }
}

/// Turn-by-turn steps of a route by street, or `None` when they say nothing more
/// than "go there", as for routes where every step is an unnamed "continue".
fn steps_view<'a>(route: &Itinerary) -> Option<Element<'a, Message>> {
    let steps = merged_steps(route.legs.iter().flat_map(|leg| &leg.steps));
    if steps.len() <= 1 && steps.iter().all(|step| step.street.is_empty()) {
        return None;
    }

    let rows: Vec<Element<'a, Message>> = steps
        .iter()
        .map(|step| {
            widget::row::with_capacity(3)
                .push(widget::text::body(instruction(step)))
                .push(widget::space::horizontal())
                .push(widget::text::caption(format_distance(step.distance)))
                .align_y(Alignment::Center)
                .into()
        })
        .collect();
    Some(
        widget::column::with_children(rows)
            .spacing(cosmic::theme::spacing().space_xs)
            .into(),
    )
}

/// Steps with consecutive repeats joined, such as many "continue" on one street.
fn merged_steps<'a>(steps: impl Iterator<Item = &'a Step>) -> Vec<Step> {
    let mut merged: Vec<Step> = Vec::new();
    for step in steps {
        match merged.last_mut() {
            Some(last) if last.direction == step.direction && last.street == step.street => {
                last.distance += step.distance;
            }
            Some(last) if step.direction == "CONTINUE" && last.street == step.street => {
                last.distance += step.distance;
            }
            _ => merged.push(step.clone()),
        }
    }
    merged
}

/// "Turn left onto Rue de Rivoli", from MOTIS' relative directions.
fn instruction(step: &Step) -> String {
    let action = match step.direction.as_str() {
        "DEPART" => fl!("step-depart"),
        "LEFT" => fl!("step-left"),
        "RIGHT" => fl!("step-right"),
        "SLIGHTLY_LEFT" => fl!("step-slightly-left"),
        // MOTIS also spells it this way.
        "SLIGHTLY_RIGHT" | "SLIGHT_RIGHT" => fl!("step-slightly-right"),
        "HARD_LEFT" => fl!("step-hard-left"),
        "HARD_RIGHT" => fl!("step-hard-right"),
        "CIRCLE_CLOCKWISE" | "CIRCLE_COUNTERCLOCKWISE" => fl!("step-roundabout"),
        "UTURN_LEFT" | "UTRUN_LEFT" | "UTURN_RIGHT" => fl!("step-uturn"),
        "ELEVATOR" => fl!("step-elevator"),
        "STAIRS" => fl!("step-stairs"),
        "ENTER_STATION" => fl!("step-enter-station"),
        "EXIT_STATION" => fl!("step-exit-station"),
        "FOLLOW_SIGNS" => fl!("step-follow-signs"),
        _ => fl!("step-continue"),
    };
    if step.street.is_empty() {
        action
    } else {
        let street = step.street.clone();
        fl!("step-onto", action = action, street = street)
    }
}

/// Distance covered by street, in meters.
fn street_distance(route: &Itinerary) -> f64 {
    route.legs.iter().filter_map(|leg| leg.distance).sum()
}

/// The kind of vehicle of a MOTIS mode, or `None` if the line name says enough.
fn transit_kind(mode: &str) -> Option<String> {
    Some(match mode {
        "SUBWAY" | "METRO" => fl!("transit-metro"),
        "BUS" | "COACH" => fl!("transit-bus"),
        "TRAM" => fl!("transit-tram"),
        "FERRY" => fl!("transit-ferry"),
        "RAIL" | "HIGHSPEED_RAIL" | "LONG_DISTANCE" | "NIGHT_RAIL" | "REGIONAL_FAST_RAIL"
        | "REGIONAL_RAIL" | "SUBURBAN" => fl!("transit-train"),
        _ => return None,
    })
}

/// Local time, such as "15:01".
fn format_time(time: Timestamp) -> String {
    time.to_zoned(TimeZone::system())
        .strftime("%H:%M")
        .to_string()
}

/// "10 min", or "1 h 05" from an hour.
fn format_duration(seconds: u64) -> String {
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        fl!("route-minutes", minutes = minutes)
    } else {
        let hours = minutes / 60;
        let minutes = format!("{:02}", minutes % 60);
        fl!("route-hours", hours = hours, minutes = minutes)
    }
}

/// "850 m", "1.4 km", or "466 km" from 100 km.
fn format_distance(meters: f64) -> String {
    if meters < 1000.0 {
        // To the nearest 10 m: walking distances are estimates.
        let meters = ((meters / 10.0).round() * 10.0) as u64;
        fl!("route-meters", meters = meters)
    } else {
        let decimals = usize::from(meters < 100_000.0);
        let kilometers = format!("{:.*}", decimals, meters / 1000.0);
        fl!("route-kilometers", kilometers = kilometers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(direction: &str, street: &str, distance: f64) -> Step {
        Step {
            direction: direction.to_owned(),
            street: street.to_owned(),
            distance,
        }
    }

    #[test]
    fn durations_and_distances() {
        assert_eq!(format_duration(600), fl!("route-minutes", minutes = 10));
        assert_eq!(
            format_duration(3900),
            fl!("route-hours", hours = 1, minutes = "05")
        );
        assert_eq!(format_distance(847.0), fl!("route-meters", meters = 850));
        assert_eq!(
            format_distance(1358.0),
            fl!("route-kilometers", kilometers = "1.4")
        );
        // Paris → Lyon by car.
        assert_eq!(
            format_distance(465_696.0),
            fl!("route-kilometers", kilometers = "466")
        );
    }

    #[test]
    fn deviations_follow_the_minutes_shown() {
        let planned = |estimated: &str, scheduled: &str| Planned {
            estimated: estimated.parse().unwrap(),
            scheduled: scheduled.parse().unwrap(),
        };

        let on_time = planned("2026-10-25T15:01:00Z", "2026-10-25T15:01:00Z");
        assert_eq!(Deviation::of_times(on_time), Deviation::Same);
        // Seconds are not shown.
        let seconds = planned("2026-10-25T15:01:40Z", "2026-10-25T15:01:00Z");
        assert_eq!(Deviation::of_times(seconds), Deviation::Same);
        let late = planned("2026-10-25T15:05:00Z", "2026-10-25T15:01:00Z");
        assert_eq!(Deviation::of_times(late), Deviation::Late);
        let early = planned("2026-10-25T14:59:00Z", "2026-10-25T15:01:00Z");
        assert_eq!(Deviation::of_times(early), Deviation::Early);

        assert_eq!(Deviation::of_minutes(17, 15), Deviation::Late);
    }

    #[test]
    fn repeated_steps_are_merged() {
        let steps = [
            step("DEPART", "Rue de Rivoli", 15.0),
            step("CONTINUE", "Rue de Rivoli", 120.0),
            step("LEFT", "Rue du Renard", 40.0),
            step("CONTINUE", "", 10.0),
            step("CONTINUE", "", 5.0),
        ];
        let merged = merged_steps(steps.iter());

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].distance, 135.0);
        assert_eq!(merged[1].street, "Rue du Renard");
        assert_eq!(merged[2].distance, 15.0);
    }

    #[test]
    fn instructions_name_the_street() {
        assert_eq!(instruction(&step("LEFT", "", 10.0)), fl!("step-left"));
        let left = fl!("step-left");
        assert_eq!(
            instruction(&step("LEFT", "Rue du Renard", 10.0)),
            fl!("step-onto", action = left, street = "Rue du Renard")
        );
        // Unknown directions read as "continue".
        assert_eq!(
            instruction(&step("SOMETHING_NEW", "", 10.0)),
            fl!("step-continue")
        );
    }

    #[test]
    fn trip_ends_use_the_field_names() {
        let ends = Ends {
            start: "Musée du Louvre",
            destination: "Hôtel de Ville",
        };
        assert_eq!(stop_name("START", &ends), "Musée du Louvre");
        assert_eq!(stop_name("END", &ends), "Hôtel de Ville");
        assert_eq!(stop_name("Châtelet", &ends), "Châtelet");
    }

    #[test]
    fn badge_text_contrasts() {
        // Metro 1's yellow takes black text, a dark blue white text.
        assert_eq!(
            contrasting(Color::from_rgb8(0xff, 0xbe, 0x00)),
            Color::BLACK
        );
        assert_eq!(
            contrasting(Color::from_rgb8(0x00, 0x30, 0x80)),
            Color::WHITE
        );
    }
}
