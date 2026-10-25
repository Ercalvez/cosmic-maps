//! Extra POI information fetched from Nominatim.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;

use super::camera::world_position;
use super::http::{CLIENT, RateLimit};
use super::poi::{OsmId, Poi, system_languages};

pub use super::http::Error;

const LOOKUP_URL: &str = "https://nominatim.openstreetmap.org/lookup";
const REVERSE_URL: &str = "https://nominatim.openstreetmap.org/reverse";
/// Nominatim's usage policy allows at most one request per second.
static NOMINATIM: RateLimit = RateLimit::new(Duration::from_secs(1));
/// Languages for addresses, such as "fr,en".
static LANGUAGES: LazyLock<String> = LazyLock::new(|| system_languages().join(","));
/// Countries writing the house number before the street, such as "10 Rue Malakoff".
const NUMBER_FIRST_COUNTRIES: [&str; 9] = ["au", "ca", "fr", "gb", "ie", "lu", "nz", "us", "za"];

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
        OsmId::Relation(id) => format!("R{id}"),
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

/// What is at a latitude and longitude, with its details, or `None` if nothing is.
/// `zoom` sets how detailed the answer is, from 3 (a country) to 18 (a building),
/// close to map zoom levels.
pub async fn reverse(lat: f64, lon: f64, zoom: u8) -> Result<Option<(Poi, PoiDetails)>, Error> {
    NOMINATIM.wait_turn().await;

    let (lat, lon, zoom) = (lat.to_string(), lon.to_string(), zoom.to_string());
    let response: ReverseResponse = CLIENT
        .get(REVERSE_URL)
        .query(&[
            ("lat", lat.as_str()),
            ("lon", lon.as_str()),
            ("zoom", zoom.as_str()),
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

    Ok(match response {
        ReverseResponse::Nothing { .. } => None,
        ReverseResponse::Found(place) => place.into_poi(),
    })
}

/// Nominatim answers `/reverse` with an error object when nothing is there.
#[derive(Deserialize)]
#[serde(untagged)]
enum ReverseResponse {
    // First: `Place` has no required field, so it would match the error too.
    Nothing {
        #[expect(dead_code, reason = "only tells the two answers apart")]
        error: String,
    },
    Found(Place),
}

/// The parts of a Nominatim `jsonv2` result that are used.
#[derive(Deserialize)]
struct Place {
    name: Option<String>,
    osm_type: Option<String>,
    osm_id: Option<u64>,
    /// Main OSM key and value, such as `amenity` and `cafe`.
    category: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    lat: Option<String>,
    lon: Option<String>,
    #[serde(default)]
    address: Address,
    #[serde(default)]
    extratags: Option<HashMap<String, String>>,
}

impl Place {
    /// The place as a POI to select, with its details. Places without a name, such
    /// as plain addresses, are named after their street.
    fn into_poi(mut self) -> Option<(Poi, PoiDetails)> {
        let lat = self.lat.take()?.parse().ok()?;
        let lon = self.lon.take()?.parse().ok()?;
        let name = self
            .name
            .take()
            .filter(|name| !name.is_empty())
            .or_else(|| self.address.street())?;

        let poi = Poi {
            name,
            translated_name: None,
            class: self.category.take(),
            subclass: self.kind.take(),
            rank: 0,
            osm_id: match (self.osm_type.as_deref(), self.osm_id) {
                (Some("node"), Some(id)) => Some(OsmId::Node(id)),
                (Some("way"), Some(id)) => Some(OsmId::Way(id)),
                (Some("relation"), Some(id)) => Some(OsmId::Relation(id)),
                _ => None,
            },
            position: world_position(lat, lon),
        };
        Some((poi, PoiDetails::from(self)))
    }
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
    country_code: Option<String>,
}

impl Address {
    /// Street and house number, in the order used in the country.
    fn street(&self) -> Option<String> {
        let road = self
            .road
            .clone()
            .or_else(|| self.pedestrian.clone())
            .or_else(|| self.square.clone());
        let number = self.house_number.clone();

        let number_first = self
            .country_code
            .as_deref()
            .is_some_and(|code| NUMBER_FIRST_COUNTRIES.contains(&code));
        if number_first {
            join([number, road], " ")
        } else {
            join([road, number], " ")
        }
    }

    fn format(self) -> Option<String> {
        let street = self.street();
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

    #[test]
    fn parses_reverse_address() {
        // Trimmed `/reverse` response for 10 Rue Malakoff, Brest.
        let json = r#"{
            "osm_type": "node",
            "osm_id": 1693005227,
            "lat": "48.3960198",
            "lon": "-4.4834467",
            "category": "place",
            "type": "house",
            "name": "",
            "address": {
                "house_number": "10",
                "road": "Rue Malakoff",
                "city": "Brest",
                "postcode": "29200",
                "country": "France",
                "country_code": "fr"
            },
            "extratags": {}
        }"#;

        let ReverseResponse::Found(place) = serde_json::from_str(json).unwrap() else {
            panic!("expected a place");
        };
        let (poi, details) = place.into_poi().unwrap();

        assert_eq!(poi.name, "10 Rue Malakoff");
        assert_eq!(poi.osm_id, Some(OsmId::Node(1693005227)));
        assert_eq!(poi.subclass.as_deref(), Some("house"));
        assert_eq!(details.address.as_deref(), Some("10 Rue Malakoff, 29200 Brest"));
    }

    #[test]
    fn parses_reverse_nothing() {
        let response: ReverseResponse =
            serde_json::from_str(r#"{"error": "Unable to geocode"}"#).unwrap();

        assert!(matches!(response, ReverseResponse::Nothing { .. }));
    }
}
