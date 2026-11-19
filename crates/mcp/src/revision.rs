//! The revisions of the protocol this crate speaks.

/// A dated revision of the Model Context Protocol.
///
/// They fall into two families. The three older ones begin a conversation
/// with an `initialize` request; the specification calls them *legacy*. The
/// newest has no such beginning: every request says which revision it
/// speaks and stands alone. The specification calls it *modern*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Revision {
    /// 2025-03-26: the first with this HTTP transport. Requests may come
    /// several to a body, as a JSON-RPC batch.
    V2025_03_26,
    /// 2025-06-18: no batches; the client names the revision in a header
    /// after `initialize`.
    V2025_06_18,
    /// 2025-11-25: the last revision that begins with `initialize`.
    V2025_11_25,
    /// 2026-07-28: no `initialize`, no sessions; every request stands alone.
    V2026_07_28,
}

impl Revision {
    /// Every revision this crate speaks, newest first: the order in which
    /// a server lists them to a client.
    pub const ALL: [Self; 4] = [
        Self::V2026_07_28,
        Self::V2025_11_25,
        Self::V2025_06_18,
        Self::V2025_03_26,
    ];

    /// The newest revision that begins with `initialize`: what a client is
    /// offered that asks to initialize with a revision this crate does not
    /// speak.
    pub const NEWEST_LEGACY: Self = Self::V2025_11_25;

    /// The revision a request speaks that names none: the only one whose
    /// clients send no `MCP-Protocol-Version` header.
    pub const UNNAMED: Self = Self::V2025_03_26;

    /// The revision as the protocol writes it, for example `2025-11-25`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V2025_03_26 => "2025-03-26",
            Self::V2025_06_18 => "2025-06-18",
            Self::V2025_11_25 => "2025-11-25",
            Self::V2026_07_28 => "2026-07-28",
        }
    }

    /// Reads a revision. `None` for one this crate does not speak.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|revision| revision.as_str() == text)
    }

    /// Whether the revision begins a conversation with `initialize`.
    pub fn is_legacy(self) -> bool {
        self < Self::V2026_07_28
    }

    /// Whether a body may hold several messages, as a JSON-RPC batch.
    pub(crate) fn has_batches(self) -> bool {
        self == Self::V2025_03_26
    }

    /// Whether names have a `title` beside them: tools and the server
    /// itself. Before, a tool's title was one of its annotations.
    pub(crate) fn has_titles(self) -> bool {
        self >= Self::V2025_06_18
    }

    /// Whether an error may leave its `id` out when the request's could not
    /// be read. Before, the field was required, and JSON-RPC's `null` is
    /// what stands there.
    pub(crate) fn may_omit_error_id(self) -> bool {
        self >= Self::V2025_11_25
    }

    /// The names of every revision, newest first.
    pub(crate) fn names() -> Vec<&'static str> {
        Self::ALL.into_iter().map(Self::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_are_read_from_their_dates_and_sort_by_them() {
        for revision in Revision::ALL {
            assert_eq!(Revision::parse(revision.as_str()), Some(revision));
        }
        assert_eq!(Revision::parse("2024-11-05"), None);
        assert_eq!(Revision::parse("2026-07-28 "), None);
        assert_eq!(Revision::parse(""), None);
        assert!(Revision::ALL.is_sorted_by(|a, b| a > b));
        assert_eq!(Revision::names()[0], "2026-07-28");
    }

    #[test]
    fn the_newest_revision_is_the_only_one_without_initialize() {
        let legacy: Vec<&str> = Revision::ALL
            .into_iter()
            .filter(|revision| revision.is_legacy())
            .map(Revision::as_str)
            .collect();
        assert_eq!(legacy, ["2025-11-25", "2025-06-18", "2025-03-26"]);
        assert!(Revision::NEWEST_LEGACY.is_legacy());
        assert!(Revision::UNNAMED.has_batches());
        assert!(!Revision::V2025_06_18.has_batches());
    }
}
