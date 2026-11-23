//! A field of a change that may be left out, set or cleared: left out
//! keeps what is there, `null` clears it, a value sets it. Read into
//! `Option<Option<T>>`: `None`, `Some(None)` and `Some(Some(value))`.
//!
//! Used with `#[serde(default, skip_serializing_if = "Option::is_none",
//! with = "super::patch")]`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Writes `Some(None)` as `null` and `Some(Some(value))` as the value. A
/// field that is `None` is skipped before this is called.
#[allow(clippy::ref_option, reason = "the signature serde's `with` asks for")]
pub(crate) fn serialize<T: Serialize, S: Serializer>(
    value: &Option<Option<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(inner) => inner.serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// Reads a field that is there: `null` as `Some(None)`.
pub(crate) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}
