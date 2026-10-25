//! Extra POI information fetched from Nominatim.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;

use super::http::{CLIENT, RateLimit};
use super::poi::{OsmId, system_languages};

pub use super::http::Error;

const LOOKUP_URL: &str = "https://nominatim.openstreetmap.org/lookup";
/// Nominatim's usage policy allows at most one request per second.
static NOMINATIM: RateLimit = RateLimit::new(Duration::from_secs(1));
/// Languages for addresses, such as "fr,en".
static LANGUAGES: LazyLock<String> = LazyLock::new(|| system_languages().join(","));

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoiDetails {
    /// Street and city, such as "Schillingstraße 12, 10179 Berlin".
    pub address: Option<String>,
    /// Raw `opening_hours` tag, such as "Mo-Fr 09:00-19:00".
    pub opening_hours: Option<String>,
    pub website: Option<String>,
    pub phone: Option<String>,
    pub wheelchair: Option<Wheelchair>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wheelchair {
    Yes,
    Limited,
    No,
}

pub async fn fetch(osm_id: OsmId) -> Result<PoiDetails, Error> {
    NOMINATIM.wait_turn().await;

    let id = match osm_id {
        OsmId::Node(id) => format!("N{id}"),
        OsmId::Way(id) => format!("W{id}"),
    };
    let places: Vec<Place> = CLIENT
        .get(LOOKUP_URL)
        .query(&[
            ("osm_ids", id.as_str()),
            ("format", "jsonv2"),
            ("addressdetails", "1"),
            ("extratags", "1"),
            ("accept-language", LANGUAGES.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    places
        .into_iter()
        .next()
        .map(PoiDetails::from)
        .ok_or(Error::NotFound)
}

/// The parts of a Nominatim `jsonv2` result that are used.
#[derive(Deserialize)]
struct Place {
    #[serde(default)]
    address: Address,
    #[serde(default)]
    extratags: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct Address {
    road: Option<String>,
    pedestrian: Option<String>,
    square: Option<String>,
    house_number: Option<String>,
    postcode: Option<String>,
    city: Option<String>,
    town: Option<String>,
    village: Option<String>,
}

impl Address {
    fn format(self) -> Option<String> {
        let street = join(
            [self.road.or(self.pedestrian).or(self.square), self.house_number],
            " ",
        );
        let locality = join(
            [self.postcode, self.city.or(self.town).or(self.village)],
            " ",
        );
        join([street, locality], ", ")
    }
}

/// Joins the present parts, or returns `None` if there are none.
pub(super) fn join<const N: usize>(parts: [Option<String>; N], separator: &str) -> Option<String> {
    let parts: Vec<String> = parts.into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(separator))
}

impl From<Place> for PoiDetails {
    fn from(place: Place) -> Self {
        let mut tags = place.extratags.unwrap_or_default();
        let mut tag = |keys: &[&str]| keys.iter().find_map(|key| tags.remove(*key));

        Self {
            address: place.address.format(),
            opening_hours: tag(&["opening_hours"]),
            website: tag(&["website", "contact:website"]),
            phone: tag(&["phone", "contact:phone"]),
            wheelchair: tag(&["wheelchair"]).and_then(|value| match value.as_str() {
                "yes" | "designated" => Some(Wheelchair::Yes),
                "limited" => Some(Wheelchair::Limited),
                "no" => Some(Wheelchair::No),
                _ => None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lookup_result() {
        // Trimmed response for https://www.openstreetmap.org/way/29387900.
        let json = r#"[{
            "osm_type": "way",
            "osm_id": 29387900,
            "name": "Ambulatorium Schillingstraße 12",
            "address": {
                "amenity": "Ambulatorium Schillingstraße 12",
                "house_number": "12",
                "road": "Schillingstraße",
                "suburb": "Mitte",
                "city": "Berlin",
                "postcode": "10179",
                "country": "Allemagne"
            },
            "extratags": {
                "healthcare": "clinic",
                "wheelchair": "no",
                "opening_hours": "Mo-Fr 08:00-18:00",
                "contact:website": "https://example.org"
            }
        }]"#;

        let place = serde_json::from_str::<Vec<Place>>(json).unwrap().remove(0);

        assert_eq!(
            PoiDetails::from(place),
            PoiDetails {
                address: Some("Schillingstraße 12, 10179 Berlin".to_owned()),
                opening_hours: Some("Mo-Fr 08:00-18:00".to_owned()),
                website: Some("https://example.org".to_owned()),
                phone: None,
                wheelchair: Some(Wheelchair::No),
            }
        );
    }

    #[test]
    fn missing_extratags_and_address() {
        let place = serde_json::from_str::<Vec<Place>>(r#"[{"extratags": null}]"#)
            .unwrap()
            .remove(0);

        assert_eq!(PoiDetails::from(place), PoiDetails::default());
    }
}
