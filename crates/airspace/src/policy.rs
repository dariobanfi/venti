//! Which airspaces a paraglider is warned about.
//!
//! Whether an airspace concerns a paraglider depends on its class, its type
//! and the country it is in. Those rules are a table here, not `if`s spread
//! through the warning code: [`PARAGLIDER_RULES`] is read from the top and
//! the first rule that fits an airspace decides. The table and the sources
//! of each row are set out in `docs/airspace.md`; a change to one is a
//! change to the other.
//!
//! The table is written for a paraglider flying by visual rules in Germany
//! and Austria with no aviation radio and no transponder, and it assumes
//! nothing is known about which areas are active today. Where a rule was in
//! doubt it warns. An airspace that fits no rule is warned about.

use venti_domain::{Airspace, AirspaceClass, AirspaceType};

/// What an airspace means for the pilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Policy {
    /// Nothing a paraglider flying by visual rules needs permission for.
    /// Never warned about.
    Free,
    /// Not closed, but something there deserves attention: parachutists,
    /// aerobatics, a nature reserve with its own rules. The pilot is told
    /// when about to enter and while inside, and not on merely passing by.
    Caution,
    /// Entering needs a clearance or equipment a paraglider normally lacks,
    /// or is forbidden while the area is active. The pilot is warned when
    /// near, when about to enter and when inside.
    Avoid,
}

/// One row of the table: what it fits, and what follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The country the rule is for, as an ISO 3166-1 alpha-2 code. `None`
    /// fits every country, and also an airspace whose country is unknown.
    pub country: Option<&'static str>,
    /// The type the rule is for. `None` fits every type. An
    /// [`AirspaceType::Other`] fits every other type, whatever its text.
    pub airspace_type: Option<AirspaceType>,
    /// The class the rule is for. `None` fits every class.
    pub class: Option<AirspaceClass>,
    /// What follows.
    pub policy: Policy,
    /// Why, in a sentence a pilot can be shown.
    pub reason: &'static str,
}

impl Rule {
    /// Whether the rule fits an airspace.
    pub fn fits(&self, airspace: &Airspace) -> bool {
        let country = self
            .country
            .is_none_or(|country| airspace.country.as_deref() == Some(country));
        let airspace_type = self.airspace_type.as_ref().is_none_or(|wanted| {
            std::mem::discriminant(wanted) == std::mem::discriminant(&airspace.airspace_type)
        });
        let class = self.class.is_none_or(|class| class == airspace.class);
        country && airspace_type && class
    }
}

const fn by_type(airspace_type: AirspaceType, policy: Policy, reason: &'static str) -> Rule {
    Rule {
        country: None,
        airspace_type: Some(airspace_type),
        class: None,
        policy,
        reason,
    }
}

const fn by_class(class: AirspaceClass, policy: Policy, reason: &'static str) -> Rule {
    Rule {
        country: None,
        airspace_type: None,
        class: Some(class),
        policy,
        reason,
    }
}

const SECTOR: &str = "Opens to gliders when activated and closes nothing. \
    The airspace it lies in keeps warning, because the app does not know whether it is open.";
const INFORMATION: &str = "A boundary of an information or control service, not a restriction.";
const CONTROL_ZONE: &str = "Control zone: entry needs a clearance by radio.";
const ACTIVE_UNKNOWN: &str =
    "Entry is restricted while active, and the app does not know when it is.";
const MILITARY: &str = "Military airspace: avoid unless known to be inactive.";
const HAZARD: &str = "Not closed, but an activity here can be dangerous to a paraglider.";
const NATURE: &str =
    "Overflight or landing is restricted here. The rules differ from area to area.";
const AERODROME: &str = "Traffic zone of an aerodrome.";
const CLEARANCE: &str = "Entry needs a clearance, a radio and a transponder.";
const OPEN: &str = "Open to flights by visual rules without a clearance.";
const UNKNOWN: &str = "The source gives neither a class nor a type the app knows.";

/// The default table, for a paraglider in Germany and Austria.
///
/// Read from the top; the first rule that fits decides. Rules by type come
/// before rules by class, because a type such as a transponder zone
/// restricts airspace whose class alone would be open.
pub static PARAGLIDER_RULES: &[Rule] = &[
    // What closes nothing.
    by_type(AirspaceType::GliderSector, Policy::Free, SECTOR),
    Rule {
        country: Some("AT"),
        airspace_type: Some(AirspaceType::Tra),
        class: None,
        policy: Policy::Free,
        reason: "In Austria a temporary reserved airspace is class C or D airspace that becomes \
            class G while activated. It closes nothing; the airspace it lies in keeps warning.",
    },
    by_type(AirspaceType::Fir, Policy::Free, INFORMATION),
    by_type(AirspaceType::Uir, Policy::Free, INFORMATION),
    by_type(AirspaceType::FisSector, Policy::Free, INFORMATION),
    by_type(AirspaceType::AccSector, Policy::Free, INFORMATION),
    by_type(AirspaceType::VfrSector, Policy::Free, INFORMATION),
    by_type(AirspaceType::VfrRoute, Policy::Free, INFORMATION),
    by_type(AirspaceType::TransponderSetting, Policy::Free, INFORMATION),
    by_type(
        AirspaceType::TransponderRecommended,
        Policy::Free,
        INFORMATION,
    ),
    // What a paraglider stays out of whatever its class.
    by_type(AirspaceType::Ctr, Policy::Avoid, CONTROL_ZONE),
    by_type(AirspaceType::MilitaryCtr, Policy::Avoid, CONTROL_ZONE),
    by_type(AirspaceType::Restricted, Policy::Avoid, ACTIVE_UNKNOWN),
    by_type(AirspaceType::Prohibited, Policy::Avoid, "Prohibited area."),
    by_type(
        AirspaceType::GliderProhibited,
        Policy::Avoid,
        "Closed to gliders.",
    ),
    by_type(AirspaceType::Danger, Policy::Avoid, ACTIVE_UNKNOWN),
    by_type(
        AirspaceType::Tmz,
        Policy::Avoid,
        "Transponder mandatory zone: no entry without a transponder.",
    ),
    by_type(
        AirspaceType::Rmz,
        Policy::Avoid,
        "Radio mandatory zone: no entry without an aviation radio and a call before entering.",
    ),
    by_type(AirspaceType::Tra, Policy::Avoid, ACTIVE_UNKNOWN),
    by_type(AirspaceType::Tsa, Policy::Avoid, ACTIVE_UNKNOWN),
    by_type(
        AirspaceType::TemporaryRestriction,
        Policy::Avoid,
        ACTIVE_UNKNOWN,
    ),
    by_type(AirspaceType::NotamArea, Policy::Avoid, ACTIVE_UNKNOWN),
    by_type(AirspaceType::MilitaryTrainingArea, Policy::Avoid, MILITARY),
    by_type(AirspaceType::Adiz, Policy::Avoid, MILITARY),
    by_type(AirspaceType::Atz, Policy::Avoid, AERODROME),
    by_type(AirspaceType::Matz, Policy::Avoid, AERODROME),
    by_type(AirspaceType::Htz, Policy::Avoid, AERODROME),
    // What is open but deserves a word.
    by_type(AirspaceType::Tiz, Policy::Caution, AERODROME),
    by_type(AirspaceType::Tia, Policy::Caution, AERODROME),
    by_type(AirspaceType::AerialSporting, Policy::Caution, HAZARD),
    by_type(AirspaceType::Alert, Policy::Caution, HAZARD),
    by_type(AirspaceType::Warning, Policy::Caution, HAZARD),
    by_type(AirspaceType::MilitaryTrainingRoute, Policy::Caution, HAZARD),
    by_type(AirspaceType::MilitaryRoute, Policy::Caution, HAZARD),
    by_type(AirspaceType::FeedingRoute, Policy::Caution, HAZARD),
    by_type(AirspaceType::OverflightRestriction, Policy::Caution, NATURE),
    by_type(AirspaceType::Protected, Policy::Caution, NATURE),
    // Everything else goes by its class: terminal and control areas,
    // airways, plain airspace of a class, and types the app does not know.
    by_class(
        AirspaceClass::A,
        Policy::Avoid,
        "Class A: closed to flights by visual rules.",
    ),
    by_class(AirspaceClass::B, Policy::Avoid, CLEARANCE),
    by_class(AirspaceClass::C, Policy::Avoid, CLEARANCE),
    by_class(AirspaceClass::D, Policy::Avoid, CLEARANCE),
    by_class(AirspaceClass::E, Policy::Free, OPEN),
    by_class(AirspaceClass::G, Policy::Free, OPEN),
    by_class(
        AirspaceClass::F,
        Policy::Caution,
        "Class F: advisory airspace, open to flights by visual rules.",
    ),
    by_class(AirspaceClass::Unclassified, Policy::Avoid, UNKNOWN),
];

/// A table of rules that can be asked about an airspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyTable {
    rules: Vec<Rule>,
}

impl PolicyTable {
    /// The default table: [`PARAGLIDER_RULES`].
    pub fn paraglider() -> Self {
        Self::new(PARAGLIDER_RULES.to_vec())
    }

    /// A table of the caller's own rules, read from the first to the last.
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    /// Puts a rule before all others, so that it decides wherever it fits.
    /// This is how a pilot with a radio opens radio zones, for example.
    pub fn prefer(&mut self, rule: Rule) {
        self.rules.insert(0, rule);
    }

    /// The rules, in the order they are read.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The first rule that fits an airspace.
    pub fn rule_for(&self, airspace: &Airspace) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.fits(airspace))
    }

    /// What an airspace means for the pilot. An airspace no rule fits is
    /// one to avoid: when in doubt, warn.
    pub fn policy_for(&self, airspace: &Airspace) -> Policy {
        self.rule_for(airspace)
            .map_or(Policy::Avoid, |rule| rule.policy)
    }
}

impl Default for PolicyTable {
    fn default() -> Self {
        Self::paraglider()
    }
}

#[cfg(test)]
mod tests {
    use venti_domain::{Activity, GeoPoint, Polygon, Validity, VerticalLimit};

    use super::*;

    fn airspace(
        country: Option<&str>,
        airspace_type: AirspaceType,
        class: AirspaceClass,
    ) -> Airspace {
        let point = |lat, lon| GeoPoint::new(lat, lon).unwrap();
        let ring = vec![
            point(47.0, 11.0),
            point(47.0, 11.2),
            point(47.1, 11.2),
            point(47.0, 11.0),
        ];
        Airspace {
            id: "test:a".parse().unwrap(),
            name: "TEST".to_owned(),
            class,
            airspace_type,
            country: country.map(str::to_owned),
            lower: VerticalLimit::ground(),
            upper: VerticalLimit::unlimited(),
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        }
    }

    fn policy(country: Option<&str>, airspace_type: AirspaceType, class: AirspaceClass) -> Policy {
        PolicyTable::paraglider().policy_for(&airspace(country, airspace_type, class))
    }

    #[test]
    fn controlled_airspace_is_avoided_and_open_airspace_is_free() {
        use AirspaceClass::{C, D, E, G, Unclassified};
        let de = Some("DE");
        assert_eq!(policy(de, AirspaceType::Ctr, D), Policy::Avoid);
        assert_eq!(policy(de, AirspaceType::Tma, C), Policy::Avoid);
        assert_eq!(policy(de, AirspaceType::Tma, D), Policy::Avoid);
        assert_eq!(policy(Some("AT"), AirspaceType::Cta, D), Policy::Avoid);
        // The same types in class E are open.
        assert_eq!(policy(Some("AT"), AirspaceType::Tma, E), Policy::Free);
        assert_eq!(policy(Some("AT"), AirspaceType::Cta, E), Policy::Free);
        assert_eq!(policy(de, AirspaceType::Unspecified, E), Policy::Free);
        assert_eq!(policy(de, AirspaceType::Unspecified, G), Policy::Free);
        // A control zone is one whatever class a source gives it.
        assert_eq!(policy(None, AirspaceType::Ctr, Unclassified), Policy::Avoid);
    }

    #[test]
    fn a_type_that_restricts_wins_over_a_class_that_is_open() {
        use AirspaceClass::{E, G, Unclassified};
        assert_eq!(policy(Some("DE"), AirspaceType::Tmz, E), Policy::Avoid);
        assert_eq!(policy(Some("DE"), AirspaceType::Rmz, G), Policy::Avoid);
        assert_eq!(
            policy(Some("DE"), AirspaceType::Restricted, Unclassified),
            Policy::Avoid
        );
        assert_eq!(
            policy(Some("AT"), AirspaceType::Danger, Unclassified),
            Policy::Avoid
        );
        assert_eq!(
            policy(Some("AT"), AirspaceType::MilitaryTrainingArea, Unclassified),
            Policy::Avoid
        );
    }

    #[test]
    fn what_closes_nothing_is_free_and_what_is_hazardous_is_a_caution() {
        use AirspaceClass::{C, Unclassified};
        // A glider sector is free even where a source files it under the
        // class of the airspace it opens.
        assert_eq!(
            policy(Some("DE"), AirspaceType::GliderSector, Unclassified),
            Policy::Free
        );
        assert_eq!(
            policy(Some("DE"), AirspaceType::GliderSector, C),
            Policy::Free
        );
        assert_eq!(
            policy(Some("DE"), AirspaceType::FisSector, Unclassified),
            Policy::Free
        );
        assert_eq!(
            policy(Some("DE"), AirspaceType::Fir, Unclassified),
            Policy::Free
        );
        assert_eq!(
            policy(Some("DE"), AirspaceType::AerialSporting, Unclassified),
            Policy::Caution
        );
        assert_eq!(
            policy(
                Some("AT"),
                AirspaceType::OverflightRestriction,
                Unclassified
            ),
            Policy::Caution
        );
    }

    #[test]
    fn a_temporary_reserved_airspace_means_different_things_in_different_countries() {
        let class = AirspaceClass::Unclassified;
        assert_eq!(policy(Some("AT"), AirspaceType::Tra, class), Policy::Free);
        assert_eq!(policy(Some("DE"), AirspaceType::Tra, class), Policy::Avoid);
        // Where the country is not known, the stricter reading holds.
        assert_eq!(policy(None, AirspaceType::Tra, class), Policy::Avoid);
    }

    #[test]
    fn what_the_table_does_not_know_is_avoided() {
        let unknown = AirspaceType::Other("SOMETHING NEW".to_owned());
        assert_eq!(
            policy(None, unknown.clone(), AirspaceClass::Unclassified),
            Policy::Avoid
        );
        assert_eq!(
            policy(None, AirspaceType::Unspecified, AirspaceClass::Unclassified),
            Policy::Avoid
        );
        // An unknown type in an open class goes by the class.
        assert_eq!(policy(None, unknown, AirspaceClass::E), Policy::Free);
        // An empty table warns about everything.
        let empty = PolicyTable::new(Vec::new());
        let open = airspace(None, AirspaceType::Unspecified, AirspaceClass::G);
        assert_eq!(empty.policy_for(&open), Policy::Avoid);
    }

    #[test]
    fn every_type_and_class_has_a_rule_with_a_reason() {
        let table = PolicyTable::paraglider();
        for rule in table.rules() {
            assert!(
                rule.reason.len() > 10 && rule.reason.ends_with('.'),
                "{rule:?}"
            );
        }
        // Every class is covered, so every airspace is decided by a rule
        // and none by the fallback.
        use AirspaceClass::{A, B, C, D, E, F, G, Unclassified};
        for class in [A, B, C, D, E, F, G, Unclassified] {
            let plain = airspace(None, AirspaceType::Unspecified, class);
            assert!(table.rule_for(&plain).is_some(), "{class:?}");
        }
    }

    #[test]
    fn a_pilot_can_put_a_rule_of_their_own_first() {
        let mut table = PolicyTable::paraglider();
        let radio_zone = airspace(Some("DE"), AirspaceType::Rmz, AirspaceClass::G);
        assert_eq!(table.policy_for(&radio_zone), Policy::Avoid);
        table.prefer(Rule {
            country: None,
            airspace_type: Some(AirspaceType::Rmz),
            class: None,
            policy: Policy::Caution,
            reason: "I carry an aviation radio.",
        });
        assert_eq!(table.policy_for(&radio_zone), Policy::Caution);
        assert_eq!(
            table.rule_for(&radio_zone).unwrap().reason,
            "I carry an aviation radio."
        );
    }
}
