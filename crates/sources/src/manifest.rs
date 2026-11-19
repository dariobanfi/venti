use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::SourceId;

/// Declared facts about one data source.
///
/// The manifest is data, not documentation. The pipeline reads it to enforce
/// request budgets, the server reads it to build the attribution screen, and
/// the licence fields decide which data may be combined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Stable identifier of the source.
    pub id: SourceId,
    /// Who publishes the data.
    pub provider: String,
    /// What kinds of data the source offers.
    pub kinds: Vec<Kind>,
    /// Countries covered, as ISO 3166-1 alpha-2 codes.
    pub countries: Vec<String>,
    /// How often the provider publishes new data, in seconds.
    pub cadence_seconds: u32,
    /// Request limits the provider imposes.
    #[serde(default)]
    pub rate_limit: RateLimit,
    /// What credential the provider requires.
    pub auth: Auth,
    /// Licence terms, as verified on the provider's own pages.
    pub licence: Licence,
}

/// A category of data with one canonical shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Station observations.
    Observations,
    /// Airspace.
    Airspace,
}

/// What credential a provider requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Auth {
    /// Open access.
    None,
    /// An API key held in server configuration.
    ApiKey,
}

/// Request limits a provider imposes. `None` means no published limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimit {
    /// Most requests allowed in any one second.
    pub per_second: Option<u32>,
    /// Most requests allowed in any one hour.
    pub per_hour: Option<u32>,
}

/// Licence terms of a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Licence {
    /// Name of the licence, for example `CC BY 4.0`.
    pub name: String,
    /// Whether commercial use is allowed.
    pub commercial: bool,
    /// Whether we may serve the data onward through our own API.
    pub redistribution: bool,
    /// Whether derived data must carry the same licence.
    pub share_alike: bool,
    /// The credit line the provider asks for.
    pub attribution: String,
    /// Where the credit line links to: the provider's page for the data or
    /// for its terms.
    pub link: String,
    /// What the provider asks to be said wherever its data is shown, beside
    /// the credit: a warning about what the data must not be used for, for
    /// example. Written for the people who see the data. `None` when the
    /// provider asks for nothing of the kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// Further conditions, in plain words, for whoever builds with the data.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// When the terms were last read on the provider's own pages.
    pub verified_on: NaiveDate,
    /// The pages they were read on.
    pub verified_from: Vec<String>,
}

/// Groups of licences whose data may be combined with each other.
///
/// Data from different families is never merged into one record or bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LicenceFamily {
    /// Attribution only, or freer.
    Open,
    /// Attribution and share-alike.
    ShareAlike,
    /// Non-commercial.
    NonCommercial,
    /// Non-commercial and share-alike.
    NonCommercialShareAlike,
    /// May be used but not served onward.
    NoRedistribution,
}

impl Licence {
    /// The family this licence belongs to.
    pub fn family(&self) -> LicenceFamily {
        match (self.redistribution, self.commercial, self.share_alike) {
            (false, _, _) => LicenceFamily::NoRedistribution,
            (true, true, false) => LicenceFamily::Open,
            (true, true, true) => LicenceFamily::ShareAlike,
            (true, false, false) => LicenceFamily::NonCommercial,
            (true, false, true) => LicenceFamily::NonCommercialShareAlike,
        }
    }
}

/// Why a manifest was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The text is not valid TOML or does not match the manifest shape.
    #[error("manifest does not parse: {0}")]
    Parse(#[from] toml::de::Error),
    /// The manifest parsed but a field has an unusable value.
    #[error("manifest for {id}: {problem}")]
    Invalid {
        /// The source the manifest describes.
        id: SourceId,
        /// What is wrong.
        problem: String,
    },
}

impl Manifest {
    /// Parses and checks a manifest written in TOML.
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest = toml::from_str(text)?;
        manifest.check()?;
        Ok(manifest)
    }

    fn check(&self) -> Result<(), ManifestError> {
        let invalid = |problem: &str| ManifestError::Invalid {
            id: self.id.clone(),
            problem: problem.to_owned(),
        };
        if self.provider.trim().is_empty() {
            return Err(invalid("provider is empty"));
        }
        if self.kinds.is_empty() {
            return Err(invalid("no kinds listed"));
        }
        let is_country = |c: &String| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase());
        if self.countries.is_empty() || !self.countries.iter().all(is_country) {
            return Err(invalid("countries must be two-letter upper-case codes"));
        }
        if self.cadence_seconds == 0 {
            return Err(invalid("cadence_seconds is zero"));
        }
        if self.licence.name.trim().is_empty() || self.licence.attribution.trim().is_empty() {
            return Err(invalid("licence name and attribution are required"));
        }
        if self
            .licence
            .notice
            .as_ref()
            .is_some_and(|notice| notice.trim().is_empty())
        {
            return Err(invalid("licence.notice is empty; leave it out instead"));
        }
        let is_https = |url: &String| url.starts_with("https://");
        if !is_https(&self.licence.link) {
            return Err(invalid("licence.link must be an https URL"));
        }
        if self.licence.verified_from.is_empty() || !self.licence.verified_from.iter().all(is_https)
        {
            return Err(invalid(
                "licence.verified_from needs at least one https URL",
            ));
        }
        Ok(())
    }

    /// The most requests one pipeline run may make without breaking the
    /// provider's hourly limit, assuming one run per cadence interval.
    ///
    /// `None` when the provider publishes no hourly limit.
    pub fn requests_per_run(&self) -> Option<usize> {
        let per_hour = u64::from(self.rate_limit.per_hour?);
        let runs_per_hour = (3600 / u64::from(self.cadence_seconds)).max(1);
        Some(usize::try_from((per_hour / runs_per_hour).max(1)).unwrap_or(usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
        id = "example-source"
        provider = "Example Weather Service"
        kinds = ["observations"]
        countries = ["AT"]
        cadence_seconds = 600
        auth = "none"

        [rate_limit]
        per_hour = 240

        [licence]
        name = "CC BY 4.0"
        commercial = true
        redistribution = true
        share_alike = false
        attribution = "Data source: Example"
        link = "https://example.org/data"
        verified_on = "2026-11-18"
        verified_from = ["https://example.org/legal"]
    "#;

    #[test]
    fn parses_a_complete_manifest() {
        let manifest = Manifest::from_toml(GOOD).unwrap();
        assert_eq!(manifest.id.as_str(), "example-source");
        assert_eq!(manifest.licence.family(), LicenceFamily::Open);
        assert_eq!(manifest.requests_per_run(), Some(40));
    }

    #[test]
    fn rejects_unknown_fields_so_typos_are_caught() {
        let text = GOOD.replace("cadence_seconds", "cadence_secs");
        assert!(matches!(
            Manifest::from_toml(&text),
            Err(ManifestError::Parse(_))
        ));
    }

    #[test]
    fn rejects_a_licence_without_a_verification_url() {
        let text = GOOD.replace("https://example.org/legal", "somewhere");
        assert!(matches!(
            Manifest::from_toml(&text),
            Err(ManifestError::Invalid { .. })
        ));
    }

    #[test]
    fn rejects_a_credit_link_that_is_not_https() {
        let text = GOOD.replace("https://example.org/data", "example.org/data");
        assert!(matches!(
            Manifest::from_toml(&text),
            Err(ManifestError::Invalid { .. })
        ));
    }

    #[test]
    fn a_notice_is_optional_and_never_empty() {
        assert_eq!(Manifest::from_toml(GOOD).unwrap().licence.notice, None);

        let with = GOOD.replace(
            "link = ",
            "notice = \"Not for navigation.\"\n        link = ",
        );
        let manifest = Manifest::from_toml(&with).unwrap();
        assert_eq!(
            manifest.licence.notice.as_deref(),
            Some("Not for navigation.")
        );

        let empty = with.replace("Not for navigation.", " ");
        assert!(matches!(
            Manifest::from_toml(&empty),
            Err(ManifestError::Invalid { .. })
        ));
    }

    #[test]
    fn licence_families_follow_the_flags() {
        let mut licence = Manifest::from_toml(GOOD).unwrap().licence;
        licence.commercial = false;
        assert_eq!(licence.family(), LicenceFamily::NonCommercial);
        licence.share_alike = true;
        assert_eq!(licence.family(), LicenceFamily::NonCommercialShareAlike);
        licence.redistribution = false;
        assert_eq!(licence.family(), LicenceFamily::NoRedistribution);
    }
}
