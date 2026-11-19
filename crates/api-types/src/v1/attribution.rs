use serde::{Deserialize, Serialize};
use venti_domain::SourceId;

/// The credit one data source asks for.
///
/// Every response that carries data lists the attribution of each source
/// whose data is in it. A client that shows the data shows the credit: the
/// licences require it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    /// The source the credit belongs to. Records name their source by this
    /// identifier.
    pub source: SourceId,
    /// Who publishes the data.
    pub provider: String,
    /// Name of the licence, for example `CC BY 4.0`.
    pub licence: String,
    /// The credit line, worded as the provider asks.
    pub text: String,
    /// Where the credit line links to.
    pub url: String,
}
