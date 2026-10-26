// SPDX-License-Identifier: MPL-2.0

mod app;
mod config;
mod directions;
mod domain;
mod geo;
mod i18n;
mod key_bind;
mod map;
mod place_view;
mod route_view;
mod search_box;
mod services;

fn main() -> cosmic::iced::Result {
    // Warnings by default, without those maplibre gives for every tile;
    // `RUST_LOG=cosmic_maps=debug` shows more.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,maplibre=error")),
        )
        .init();

    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Settings for configuring the application window and iced runtime.
    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(360.0)
            .min_height(180.0),
    );

    // Starts the application's event loop with `()` as the application's flags.
    cosmic::app::run::<app::AppModel>(settings, ())
}
