//! A pilot's picture, and the pilots who took off near a place.
//!
//! The store keeps a picture as the bytes the server made, with no meaning
//! given to them: it decodes nothing. One picture per account; a new one
//! takes the place of the old under a new identifier.

use venti_domain::Timestamp;

/// Where a picture came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureOrigin {
    /// The pilot sent it.
    Upload,
    /// The server took it from the Google account the pilot signs in
    /// with.
    Google,
}

impl PictureOrigin {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Google => "google",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "upload" => Some(Self::Upload),
            "google" => Some(Self::Google),
            _ => None,
        }
    }
}

/// A picture to keep for an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPicture {
    /// Its identifier: new for every picture, never used again.
    pub id: String,
    /// The account it is the picture of.
    pub account_id: String,
    /// The picture as the server made it.
    pub bytes: Vec<u8>,
    /// Where it came from.
    pub origin: PictureOrigin,
    /// When it was made.
    pub created_at: Timestamp,
}

/// A kept picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Its identifier.
    pub id: String,
    /// The account it is the picture of.
    pub account_id: String,
    /// The picture as the server made it.
    pub bytes: Vec<u8>,
    /// Where it came from.
    pub origin: PictureOrigin,
    /// When it was made.
    pub created_at: Timestamp,
}

/// A pilot whose public flights took off near a place
/// ([`crate::Store::pilots_near`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearPilot {
    /// The account.
    pub account_id: String,
    /// Its handle, as the account is now.
    pub handle: String,
    /// Its display name, as the account is now.
    pub name: String,
    /// The identifier of its picture, if it has one.
    pub picture: Option<String>,
    /// How many of its public flights took off near the place in the span
    /// asked about.
    pub flights: u32,
}
