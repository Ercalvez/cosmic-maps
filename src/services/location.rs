// SPDX-License-Identifier: MPL-2.0

//! The user's position, from the XDG Location portal, the way COSMIC apps reach
//! desktop services. The portal asks for permission and gets positions from GeoClue.

use cosmic::dialog::ashpd::desktop::location::{Accuracy, LocationProxy};
use cosmic::dialog::ashpd::{self, AppID};
use cosmic::iced::futures::channel::mpsc::Sender;
use cosmic::iced::futures::{SinkExt, Stream, StreamExt};

use crate::geo::{LatLon, Position};

/// Moves shorter than this, in meters, are not reported.
const DISTANCE_THRESHOLD: u32 = 50;

/// The user's position whenever it changes. Ends with an error if it cannot be
/// known, such as when location services are off or permission is denied.
pub fn updates(app_id: &'static str) -> impl Stream<Item = Result<Position, String>> {
    cosmic::iced::stream::channel(4, move |mut output: Sender<_>| async move {
        if let Err(err) = watch(app_id, &mut output).await {
            let _ = output.send(Err(err.to_string())).await;
        }
    })
}

async fn watch(app_id: &str, output: &mut Sender<Result<Position, String>>) -> ashpd::Result<()> {
    // Outside Flatpak, the portal only knows which app asks once told, which it
    // needs to remember the permission. Older portals lack this: carry on.
    if let Ok(app_id) = AppID::try_from(app_id) {
        let _ = ashpd::register_host_app(app_id).await;
    }

    let proxy = LocationProxy::new().await?;
    let session = proxy
        .create_session(Some(DISTANCE_THRESHOLD), None, Some(Accuracy::Street))
        .await?;
    let mut locations = proxy.receive_location_updated().await?;
    // Waits for the user to answer the permission dialog, the first time.
    proxy.start(&session, None).await?.response()?;

    // The session stays open, and positions keep coming, while this runs.
    while let Some(location) = locations.next().await {
        let position = Position {
            lat_lon: LatLon::new(location.latitude(), location.longitude()),
            accuracy: location.accuracy(),
        };
        if output.send(Ok(position)).await.is_err() {
            break;
        }
    }

    Ok(())
}
