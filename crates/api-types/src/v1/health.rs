use serde::{Deserialize, Serialize};
use venti_domain::{ModelId, SourceId, Timestamp};

/// The answer of `GET /v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// The state of the whole server.
    pub status: HealthStatus,
    /// The server's version.
    pub version: String,
    /// The server's clock. Ages of observations are best computed against
    /// this time, not against the clock of the device.
    pub time: Timestamp,
    /// Whether the server answers point forecasts at all. Left out by a
    /// server older than forecasts, which has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forecasts: Option<ForecastsHealth>,
    /// Whether the server takes new accounts, and whether it asks for an
    /// invite code. An app shows the field for the code, or says that the
    /// server takes nobody, by this. Left out by an older server: such a
    /// server may ask for a code, so an app that reads nothing here offers
    /// the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<RegistrationMode>,
    /// Whether the server takes flying sites from pilots: whether a
    /// signed-in pilot can add a site and correct one. An app offers
    /// neither where this is not `open`. Left out by a server older than
    /// the sites that pilots make, which takes none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sites_from_pilots: Option<SitesFromPilots>,
    /// Whether the server sees the addresses of those who ask it. For
    /// whoever runs the server; an app has no use for it. Left out while
    /// too few requests have arrived to tell, and by an older server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_addresses: Option<ClientAddressesHealth>,
    /// Where the server's thermal map stands: which run of each model it
    /// is made from, and when the map it serves was worked out. For
    /// whoever runs the server; an app asks for the map itself. Left out
    /// by a server that makes none, which is one without forecasts, and by
    /// an older server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_map: Option<ThermalMapHealth>,
    /// Where the server's rain radar stands: for each radar source, the
    /// newest frame it holds and how many. For whoever runs the server; an
    /// app asks for the frames themselves. Left out by a server that
    /// collects no radar, and by an older server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radar: Vec<RadarHealth>,
    /// One entry per data source the server collects, sorted by identifier:
    /// the sources of observations, of airspace, of flying sites, of
    /// forecasts and of rain radar alike.
    pub sources: Vec<SourceHealth>,
}

/// Where one radar source of a server stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarHealth {
    /// The source.
    pub source: SourceId,
    /// How many frames the server holds of it.
    pub frames: u32,
    /// The time of the newest frame held: the end of the interval it is
    /// of. Left out while the server holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_frame: Option<Timestamp>,
    /// How old the newest frame is by the server's clock, in seconds.
    /// Left out while the server holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_frame_age_s: Option<i64>,
    /// How many bytes the pictures of the held frames take in the
    /// server's memory.
    pub bytes: u64,
}

/// The state of the whole server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Every source delivered data recently.
    Ok,
    /// At least one source is stale.
    Degraded,
}

/// Whether a server answers point forecasts.
///
/// A forecast comes from a service that whoever runs the server has to run
/// beside it. A server that was not told where that service is has no
/// forecasts, and says so here instead of asking someone else's service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastsHealth {
    /// The server asks a forecast service. Whether that service answers is
    /// said by its entry among the sources.
    Available,
    /// The server has no forecast service: it was not told where one is,
    /// or none of its regions names one. `GET /v1/forecast` answers with
    /// [`ErrorCode::NoForecasts`](super::ErrorCode::NoForecasts).
    NotConfigured,
    /// A state this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// Whether a server takes flying sites from pilots.
///
/// Reading the sites that pilots made is never off: this is about adding
/// and correcting them. An account whose publishing is suspended writes
/// none on a server that takes them; that is the account's and is not
/// said here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SitesFromPilots {
    /// A signed-in pilot can add a site and correct one, inside the
    /// regions the server serves.
    Open,
    /// The server takes none: every write answers
    /// [`ErrorCode::SitesClosed`](super::ErrorCode::SitesClosed).
    Closed,
    /// A state this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// Who may create an account on a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationMode {
    /// Anyone. `POST /v1/accounts` needs no invite code.
    Open,
    /// Whoever has an invite code from whoever runs the server.
    Invite,
    /// Nobody. Existing accounts sign in as before.
    Closed,
    /// A rule this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// Whether a server sees the addresses of those who ask it.
///
/// A server counts attempts to sign in, and requests to its connector, by
/// the address they come from. Behind a proxy that hides the addresses
/// every visitor is the same one, and what one of them does is counted
/// against all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAddressesHealth {
    /// Requests arrive from addresses of their own.
    Seen,
    /// A large share of the last hour's requests came from one and the
    /// same private address. On a server on the internet that is a proxy
    /// or a container network hiding the visitors; its log and its
    /// runbook say more.
    Hidden,
    /// A state this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// Where a server's thermal map stands.
///
/// The server makes the map anew from every run of its models, some
/// minutes after its look at the forecast source sees the run. The runs
/// here are the ones the map is made from; the runs the source holds are
/// in the source's entry ([`SourceHealth::models`]). A map that stays
/// behind them for more than a few looks is one that waits, and the
/// server's log says for what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalMapHealth {
    /// The forecast source the map is made from.
    pub source: SourceId,
    /// When the server worked out the map it serves. Left out while it
    /// has none: after a first start, until the finest model's part was
    /// fetched whole. `GET /v1/thermals` answers `503` until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<Timestamp>,
    /// The models the map is made of, the finest first.
    pub models: Vec<ThermalMapModelHealth>,
}

/// One model's part of a server's thermal map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalMapModelHealth {
    /// The model, for example `icon-d2`.
    pub model: ModelId,
    /// The run the model's part of the map was last made from, every cell
    /// of it. Left out until it was made once: the map then has no day of
    /// this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Timestamp>,
}

/// How one data source is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHealth {
    /// The source.
    pub source: SourceId,
    /// Who publishes the data.
    pub provider: String,
    /// How often the server asks the provider for new data, in seconds.
    pub cadence_seconds: u32,
    /// When a run last delivered its data without trouble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<Timestamp>,
    /// When a run last delivered nothing or ran into trouble.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<Timestamp>,
    /// The time of the newest observation the source has delivered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_observation: Option<Timestamp>,
    /// Whether the source is behind: no run has succeeded for longer than
    /// its cadence allows for. `docs/api.md` gives the rule.
    pub stale: bool,
    /// How many stations the last successful run delivered. Zero for a
    /// source of anything else.
    pub stations: u32,
    /// How many airspaces the server holds from the source. Left out for a
    /// source that delivers none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airspaces: Option<u32>,
    /// The date of the oldest file the source's airspace was read from.
    /// It can lag behind `last_success`: a run succeeds when the provider
    /// answers, also with a file it has not renewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_date: Option<Timestamp>,
    /// How many flying sites the server holds from the source. Left out
    /// for a source that delivers none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sites: Option<u32>,
    /// The date the source's flying sites are of, by the source's own
    /// word. Left out for a source that delivers none, and before its
    /// first set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sites_as_of: Option<Timestamp>,
    /// For a source of forecasts: the weather models it serves, each with
    /// the run it was at when the server last looked, sorted by model.
    /// Left out for every other source, and before the first look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelHealth>,
}

/// Which run a weather model was at when the server last looked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelHealth {
    /// The model, for example `icon-d2`.
    pub model: ModelId,
    /// When the newest run the forecast service held began. A forecast
    /// cannot be newer than this.
    pub run: Timestamp,
    /// When the service had that run ready, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_at: Option<Timestamp>,
    /// When the server looked.
    pub checked_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_servers_answer_says_nothing_about_forecasts_or_sites() {
        let text = r#"{
            "status": "ok",
            "version": "0.1.0",
            "time": "2026-11-18T19:05:00Z",
            "sources": [{
                "source": "dwd-10min",
                "provider": "Deutscher Wetterdienst",
                "cadence_seconds": 600,
                "stale": false,
                "stations": 3
            }]
        }"#;

        let health: HealthResponse = serde_json::from_str(text).unwrap();

        assert_eq!(health.forecasts, None);
        assert_eq!(health.client_addresses, None);
        assert_eq!(health.registration, None);
        assert_eq!(health.thermal_map, None);
        assert!(health.radar.is_empty());
        assert_eq!(health.sources[0].sites, None);
        assert!(health.sources[0].models.is_empty());
        let written = serde_json::to_value(&health).unwrap();
        assert!(written.get("forecasts").is_none());
        assert!(written.get("client_addresses").is_none());
        assert!(written.get("thermal_map").is_none());
        assert!(written.get("radar").is_none());
        assert!(written["sources"][0].get("models").is_none());
    }

    #[test]
    fn a_thermal_map_not_made_yet_names_its_models_and_no_run() {
        let text = r#"{
            "source": "open-meteo",
            "models": [{"model": "icon-d2"}, {"model": "icon-eu"}]
        }"#;

        let map: ThermalMapHealth = serde_json::from_str(text).unwrap();

        assert_eq!(map.computed_at, None);
        assert_eq!(map.models.len(), 2);
        assert!(map.models.iter().all(|model| model.run.is_none()));
        // What is not known is left out, not written as null.
        let written = serde_json::to_value(&map).unwrap();
        assert!(written.get("computed_at").is_none());
        assert!(written["models"][0].get("run").is_none());
    }

    #[test]
    fn a_server_says_who_may_create_an_account() {
        let mode = |text: &str| serde_json::from_str::<RegistrationMode>(text).unwrap();
        assert_eq!(mode(r#""open""#), RegistrationMode::Open);
        assert_eq!(mode(r#""invite""#), RegistrationMode::Invite);
        assert_eq!(mode(r#""closed""#), RegistrationMode::Closed);
        // A rule a newer server invents does not break an older client.
        assert_eq!(mode(r#""by_email""#), RegistrationMode::Unknown);
        assert_eq!(
            serde_json::to_string(&RegistrationMode::Invite).unwrap(),
            r#""invite""#
        );
    }

    #[test]
    fn a_server_says_whether_it_sees_who_asks() {
        let state = |text: &str| serde_json::from_str::<ClientAddressesHealth>(text).unwrap();
        assert_eq!(state(r#""seen""#), ClientAddressesHealth::Seen);
        assert_eq!(state(r#""hidden""#), ClientAddressesHealth::Hidden);
        assert_eq!(state(r#""partly_hidden""#), ClientAddressesHealth::Unknown);
        assert_eq!(
            serde_json::to_string(&ClientAddressesHealth::Hidden).unwrap(),
            r#""hidden""#
        );
    }

    #[test]
    fn a_server_says_plainly_whether_it_has_forecasts() {
        let state = |text: &str| serde_json::from_str::<ForecastsHealth>(text).unwrap();
        assert_eq!(state(r#""available""#), ForecastsHealth::Available);
        assert_eq!(state(r#""not_configured""#), ForecastsHealth::NotConfigured);
        // A state a newer server invents does not break an older client.
        assert_eq!(state(r#""paused""#), ForecastsHealth::Unknown);
        assert_eq!(
            serde_json::to_string(&ForecastsHealth::NotConfigured).unwrap(),
            r#""not_configured""#
        );
    }
}
