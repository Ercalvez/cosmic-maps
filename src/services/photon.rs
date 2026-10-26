// SPDX-License-Identifier: MPL-2.0

//! Place search with Photon, which unlike Nominatim allows search-as-you-type.

use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;

use super::http::{CLIENT, Error, RateLimit};
use super::join;
use crate::geo::LatLon;
use crate::map::poi::{OsmId, Poi, system_languages};

const SEARCH_URL: &str = "https://photon.komoot.io/api/";
const RESULT_LIMIT: &str = "8";
/// Photon asks for fair use without giving numbers. Typing is already debounced;
/// this also caps bursts.
static PHOTON: RateLimit = RateLimit::new(Duration::from_millis(500));
/// Photon rejects other languages than these with an error.
const SUPPORTED_LANGUAGES: [&str; 3] = ["de", "en", "fr"];
static LANGUAGE: LazyLock<&'static str> = LazyLock::new(|| {
    system_languages()
        .iter()
        .find_map(|language| {
            SUPPORTED_LANGUAGES
                .into_iter()
                .find(|supported| supported == language)
        })
        .unwrap_or("default")
});

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub poi: Poi,
    /// Where the place is, such as "Dircksenstraße 2, Berlin, Allemagne".
    pub area: Option<String>,
}

/// Searches places matching `query`, preferring those near `near`.
pub async fn search(query: String, near: LatLon) -> Result<Vec<SearchResult>, Error> {
    PHOTON.wait_turn().await;

    let (lat, lon) = (near.lat.to_string(), near.lon.to_string());
    let response: Response = CLIENT
        .get(SEARCH_URL)
        .query(&[
            ("q", query.as_str()),
            ("lat", lat.as_str()),
            ("lon", lon.as_str()),
            ("limit", RESULT_LIMIT),
            ("lang", *LANGUAGE),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response
        .features
        .into_iter()
        .filter_map(SearchResult::from_feature)
        .collect())
}

/// The parts of a Photon GeoJSON response that are used.
#[derive(Deserialize)]
struct Response {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
    geometry: Geometry,
}

#[derive(Deserialize)]
struct Geometry {
    /// Longitude, then latitude.
    coordinates: (f64, f64),
}

#[derive(Deserialize)]
struct Properties {
    name: Option<String>,
    osm_type: Option<String>,
    osm_id: Option<u64>,
    osm_key: Option<String>,
    osm_value: Option<String>,
    street: Option<String>,
    housenumber: Option<String>,
    city: Option<String>,
    country: Option<String>,
}

impl SearchResult {
    fn from_feature(feature: Feature) -> Option<Self> {
        let Properties {
            name,
            osm_type,
            osm_id,
            osm_key,
            osm_value,
            street,
            housenumber,
            city,
            country,
        } = feature.properties;

        let street = join([street, housenumber], " ");
        // Plain addresses have no name: show the address itself.
        let (name, street) = match name {
            Some(name) => (name, street),
            None => (street?, None),
        };
        let city = city.filter(|city| *city != name);
        let (lon, lat) = feature.geometry.coordinates;

        Some(Self {
            area: join([street, city, country], ", "),
            poi: Poi {
                name,
                translated_name: None,
                class: osm_key,
                subclass: osm_value,
                rank: 0,
                osm_id: match (osm_type.as_deref(), osm_id) {
                    (Some("N"), Some(id)) => Some(OsmId::Node(id)),
                    (Some("W"), Some(id)) => Some(OsmId::Way(id)),
                    (Some("R"), Some(id)) => Some(OsmId::Relation(id)),
                    _ => None,
                },
                position: LatLon::new(lat, lon).to_world(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_response() {
        // Trimmed response for "alexanderpl" near Berlin, with `lang=fr`.
        let json = r#"{"type": "FeatureCollection", "features": [
            {"type": "Feature",
             "properties": {"osm_type": "N", "osm_id": 3908141014, "osm_key": "railway",
                            "osm_value": "station", "housenumber": "2", "name": "Alexanderplatz",
                            "street": "Dircksenstraße", "city": "Berlin", "country": "Allemagne"},
             "geometry": {"type": "Point", "coordinates": [13.4112804, 52.5215661]}},
            {"type": "Feature",
             "properties": {"osm_type": "R", "osm_id": 62422, "osm_key": "place",
                            "osm_value": "city", "name": "Berlin", "city": "Berlin",
                            "country": "Allemagne"},
             "geometry": {"type": "Point", "coordinates": [13.3888599, 52.5170365]}},
            {"type": "Feature",
             "properties": {"osm_type": "N", "osm_id": 1, "osm_key": "place",
                            "osm_value": "house", "street": "Alexanderstraße",
                            "housenumber": "7", "city": "Berlin"},
             "geometry": {"type": "Point", "coordinates": [13.42, 52.52]}}
        ]}"#;

        let response: Response = serde_json::from_str(json).unwrap();
        let results: Vec<SearchResult> = response
            .features
            .into_iter()
            .filter_map(SearchResult::from_feature)
            .collect();

        assert_eq!(results.len(), 3);

        let station = &results[0];
        assert_eq!(station.poi.name, "Alexanderplatz");
        assert_eq!(station.poi.osm_id, Some(OsmId::Node(3_908_141_014)));
        assert_eq!(station.poi.subclass.as_deref(), Some("station"));
        assert_eq!(
            station.area.as_deref(),
            Some("Dircksenstraße 2, Berlin, Allemagne")
        );

        // The city is not repeated in its own area.
        let city = &results[1];
        assert_eq!(city.poi.osm_id, Some(OsmId::Relation(62422)));
        assert_eq!(city.area.as_deref(), Some("Allemagne"));

        // A plain address is named after itself.
        let address = &results[2];
        assert_eq!(address.poi.name, "Alexanderstraße 7");
        assert_eq!(address.area.as_deref(), Some("Berlin"));
    }
}
