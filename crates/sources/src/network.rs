use venti_domain::Timestamp;

use crate::ObservationSource;

/// A station network as its crate hands it to a build: the adapter and,
/// for the development mode, a sample of recorded answers.
///
/// Every crate of a station network has one function, `network()`, that
/// returns this, and the server's list of adapters has one line for it.
/// The registry, the development mode's recorded answers and the tests
/// that hold every adapter to the contract are made from that list.
pub struct StationNetwork {
    /// The adapter.
    pub adapter: Box<dyn ObservationSource>,
    /// What answers for the network in the development mode. `None` for a
    /// network the development mode does not have: it may then be named by
    /// no region the development mode runs, which is the first.
    pub sample: Option<Sample>,
}

/// Recorded answers of a provider, and the moment a run over them is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// The moment the answers are for: the time of a run over them.
    pub at: Timestamp,
    /// Each answer with the address it answers.
    pub answers: Vec<(String, &'static [u8])>,
}

impl StationNetwork {
    /// A network without a sample for the development mode.
    pub fn new(adapter: impl ObservationSource + 'static) -> Self {
        Self {
            adapter: Box::new(adapter),
            sample: None,
        }
    }

    /// The same network with a sample: such as the crate's
    /// `fixtures::evening_at()` and `fixtures::evening()`.
    #[must_use]
    pub fn with_sample(mut self, at: Timestamp, answers: Vec<(String, &'static [u8])>) -> Self {
        self.sample = Some(Sample { at, answers });
        self
    }
}
