use serde::Deserialize;

const EARTH_RADIUS_KM: f64 = 6371.0;

/// A saved ChasePlane camera view, reduced to what's needed to pick the closest one.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub guid: String,
    pub name: String,
    pub icao: String,
    pub lat: f64,
    pub lon: f64,
}

impl View {
    /// Great-circle distance in kilometers; altitude is ignored.
    pub fn distance_km(&self, lat: f64, lon: f64) -> f64 {
        haversine_km(self.lat, self.lon, lat, lon)
    }
}

pub(crate) fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1, lat2) = (lat1.to_radians(), lat2.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

/// Returns the view closest to the given coordinates.
pub(crate) fn closest(views: &[View], lat: f64, lon: f64) -> Option<&View> {
    views
        .iter()
        .min_by(|a, b| a.distance_km(lat, lon).total_cmp(&b.distance_km(lat, lon)))
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetViewsReply {
    pub payload: GetViewsPayload,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GetViewsPayload {
    #[serde(default)]
    pub views: Vec<RawView>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawView {
    pub guid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icao: String,
    #[serde(default)]
    pub profile_theme: String,
    #[serde(default)]
    pub skip_cycle: bool,
    pub position: RawPosition,
}

/// ChasePlane view positions use x = longitude, y = altitude, z = latitude.
#[derive(Debug, Deserialize)]
pub(crate) struct RawPosition {
    pub x: f64,
    pub z: f64,
}

/// Keeps only views for `icao` with the given profile theme that aren't flagged `skip_cycle`.
pub(crate) fn filter_views(raw: Vec<RawView>, icao: &str, theme: &str) -> Vec<View> {
    raw.into_iter()
        .filter(|v| {
            v.icao.eq_ignore_ascii_case(icao) && v.profile_theme == theme && !v.skip_cycle
        })
        .map(|v| View {
            guid: v.guid,
            name: v.name,
            icao: v.icao,
            lat: v.position.z,
            lon: v.position.x,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(guid: &str, lat: f64, lon: f64) -> View {
        View {
            guid: guid.into(),
            name: guid.into(),
            icao: "EDDF".into(),
            lat,
            lon,
        }
    }

    #[test]
    fn closest_picks_nearest_view() {
        let views = vec![view("a", 50.0, 8.5), view("b", 50.05, 8.6)];
        assert_eq!(closest(&views, 50.049, 8.59).unwrap().guid, "b");
        assert_eq!(closest(&views, 50.0, 8.5).unwrap().guid, "a");
        assert!(closest(&[], 50.0, 8.5).is_none());
    }

    #[test]
    fn haversine_known_distance() {
        // One degree of latitude is ~111.2 km.
        let d = haversine_km(50.0, 8.0, 51.0, 8.0);
        assert!((d - 111.19).abs() < 0.1, "{d}");
    }

    #[test]
    fn filters_by_icao_and_theme_and_parses_reply() {
        let reply: GetViewsReply = serde_json::from_str(
            r#"{"message":"get_views","payload":{"views":[
            {"name":"East Apron","icao":"EDDF","guid":"g1","profile_theme":"WORLD_TOWER",
             "position":{"x":8.58,"y":155.4,"z":50.05,"pitch":0,"yaw":0,"roll":0,"zoom":1}},
            {"name":"Other","icao":"EDDM","guid":"g2","profile_theme":"WORLD_TOWER","position":{"x":1,"y":2,"z":3}},
            {"name":"Chase","icao":"EDDF","guid":"g3","profile_theme":"OTHER","position":{"x":1,"y":2,"z":3}},
            {"name":"Skipped","icao":"EDDF","guid":"g4","profile_theme":"WORLD_TOWER","skip_cycle":true,"position":{"x":1,"y":2,"z":3}}
            ]}}"#,
        )
        .unwrap();
        let views = filter_views(reply.payload.views, "eddf", "WORLD_TOWER");
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].guid, "g1");
        assert_eq!((views[0].lat, views[0].lon), (50.05, 8.58));
    }
}
