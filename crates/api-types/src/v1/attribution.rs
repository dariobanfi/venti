use serde::{Deserialize, Serialize};
use venti_domain::SourceId;

/// The credit one data source asks for.
///
/// Every response that carries data lists the attribution of each source
/// whose data is in it. A client that shows the data shows the credit, and
/// the notice if there is one: the licences require it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    /// The source the credit belongs to. Records name their source by this
    /// identifier.
    pub source: SourceId,
    /// Who publishes the data.
    pub provider: String,
    /// The name pilots know a network of stations by, "MeteoSwiss": what
    /// labels its stations in a list. Never in place of the credit. Left
    /// out for a source that is no network of stations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    /// Name of the licence, for example `CC BY 4.0`.
    pub licence: String,
    /// The credit line, worded as the provider asks.
    pub text: String,
    /// Where the credit line links to.
    pub url: String,
    /// What the provider asks to be said wherever its data is shown, beside
    /// the credit: for example that the data is not certified and must not
    /// be used for navigation. Left out when the provider asks for nothing
    /// of the kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
