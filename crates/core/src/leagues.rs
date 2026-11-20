//! Handing a flight of the logbook on to a league: XContest, where the
//! pilot uploads the file themselves, and the DHV-XC, which takes it from
//! an app.
//!
//! The logbook keeps a flight's file as it was imported, which is what a
//! league wants: a recorder's signature covers every byte. This module
//! says what a pilot should know before the file goes anywhere (until
//! when the leagues take it, and whether it is signed), hands out the file,
//! and puts the request to the DHV-XC together. The shell sends that
//! request from the pilot's own device and keeps the upload password
//! there; nothing of it reaches a Venti server.
//!
//! What the leagues take and how is `venti-leagues`' to know.

use chrono::Days;
use venti_flight::igc;
use venti_leagues::{CLAIM_DAYS, FileFacts, claim_ends, dhvxc, xcontest};

use crate::{Core, CoreError, FlightFile, LogbookError};

/// The code this app's own recorder writes into its files.
const OWN_RECORDER: &str = "XVE";

/// What a pilot is told before a flight goes to a league, and what goes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LeagueHandover {
    /// The flight's file as the logbook keeps it, with a name to save it
    /// under.
    pub file: FlightFile,
    /// Whether the file carries a recorder's signature.
    pub is_signed: bool,
    /// What that means for a league, in a sentence or two.
    pub signature: String,
    /// Whether the days in which the leagues take the flight for their
    /// contests are over.
    pub is_late: bool,
    /// Until when they take it, or since when they do not, in a sentence
    /// or two.
    pub deadline: String,
    /// XContest's page on which a signed-in pilot claims a flight.
    pub xcontest_claim_url: String,
    /// XContest's rules.
    pub xcontest_rules_url: String,
    /// The page of the DHV-XC on which a pilot sets the upload password.
    pub dhvxc_profile_url: String,
    /// The DHV-XC's rules.
    pub dhvxc_rules_url: String,
}

/// A request to a league's server, for the shell to send as it is, from
/// the pilot's own device, with `POST`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LeagueRequest {
    /// The whole address.
    pub url: String,
    /// The value of the `Content-Type` header.
    pub content_type: String,
    /// The body. It holds the pilot's upload password: the shell sends it
    /// and keeps it nowhere.
    pub body: Vec<u8>,
}

/// What a league answered to a flight, for the pilot.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LeagueAnswer {
    /// Whether the league has the flight now.
    pub taken: bool,
    /// What happened, in a few words.
    pub title: String,
    /// The same at length, with the league's own words where it sent any.
    pub words: String,
}

#[uniffi::export]
impl Core {
    /// What to tell the pilot before a flight of the logbook goes to a
    /// league, with the file to hand over. `now_unix_s` is the moment the
    /// pilot asks at.
    pub fn league_handover(
        &self,
        id: String,
        now_unix_s: i64,
    ) -> Result<LeagueHandover, CoreError> {
        let file = self.flight_file(id)?;
        let flight = igc::parse(&file.igc_text).map_err(LogbookError::Invalid)?;
        // A recording without a position has a day and nothing else: the
        // day's first second stands for the flight.
        let start = flight.fixes.first().map_or_else(
            || flight.date.and_time(chrono::NaiveTime::MIN).and_utc(),
            |fix| fix.time,
        );
        let ends = claim_ends(start);
        let last_day = ends
            .date_naive()
            .checked_sub_days(Days::new(1))
            .unwrap_or_else(|| ends.date_naive())
            .format("%-d %b %Y");
        let is_late = now_unix_s >= ends.timestamp();
        let deadline = if is_late {
            format!(
                "The {CLAIM_DAYS} days in which the leagues take a flight for their contests \
                 ended with {last_day}. XContest rejects a claim after them; the DHV-XC still \
                 takes the flight unscored or as private."
            )
        } else {
            format!(
                "XContest and the DHV-XC take a flight for their contests until {CLAIM_DAYS} \
                 days after the day it was flown: this one until the end of {last_day}, by UTC."
            )
        };

        let facts = FileFacts::of(&file.igc_text);
        let unscored = "XContest lists such a flight and gives it no points, and the DHV-XC \
                        does not score it.";
        let signature = if facts.signed {
            let recorder = if facts.recorder.is_empty() {
                String::new()
            } else {
                format!(" ({})", facts.recorder)
            };
            format!(
                "The file carries the signature of its recorder{recorder}. A league checks it \
                 when the file arrives; this app cannot."
            )
        } else if facts.recorder == OWN_RECORDER {
            format!("Venti recorded this flight and does not sign its files yet. {unscored}")
        } else {
            format!("This file carries no recorder's signature. {unscored}")
        };

        Ok(LeagueHandover {
            file,
            is_signed: facts.signed,
            signature,
            is_late,
            deadline,
            xcontest_claim_url: xcontest::CLAIM_URL.to_owned(),
            xcontest_rules_url: xcontest::RULES_URL.to_owned(),
            dhvxc_profile_url: dhvxc::PROFILE_URL.to_owned(),
            dhvxc_rules_url: dhvxc::RULES_URL.to_owned(),
        })
    }

    /// The request that sends a flight of the logbook to the DHV-XC.
    ///
    /// `user` is what the pilot signs in to the DHV-XC with, and
    /// `upload_password` the upload password from their profile there,
    /// which is not the one they sign in with. `glider` is text matching a
    /// glider the pilot set up at the DHV-XC; without one the flight waits
    /// among the pilot's uploads. `publish` asks the DHV-XC to publish the
    /// flight at once if it can.
    pub fn dhvxc_upload_request(
        &self,
        id: String,
        user: String,
        upload_password: String,
        glider: Option<String>,
        publish: bool,
    ) -> Result<LeagueRequest, CoreError> {
        let file = self.flight_file(id)?;
        Ok(LeagueRequest {
            url: dhvxc::UPLOAD_URL.to_owned(),
            content_type: dhvxc::CONTENT_TYPE.to_owned(),
            body: dhvxc::body(&dhvxc::Upload {
                user: user.trim(),
                upload_password: &upload_password,
                glider: glider.as_deref().map(str::trim),
                file_name: &file.name,
                igc_text: &file.igc_text,
                publish,
            }),
        })
    }
}

/// Reads the DHV-XC's answer to [`Core::dhvxc_upload_request`] into words
/// for the pilot.
#[uniffi::export]
pub fn dhvxc_uploaded(status: u16, body: Vec<u8>) -> LeagueAnswer {
    let said = |message: &str| {
        if message.is_empty() {
            String::new()
        } else {
            format!(" The DHV-XC says: “{message}”")
        }
    };
    match dhvxc::read_answer(status, &body) {
        dhvxc::Outcome::Taken { message } => LeagueAnswer {
            taken: true,
            title: "The DHV-XC has the flight".to_owned(),
            words: format!(
                "Whether it is published or waits among your uploads there is for the DHV-XC \
                 to say.{}",
                said(&message)
            ),
        },
        dhvxc::Outcome::NotSignedIn { message } => LeagueAnswer {
            taken: false,
            title: "The DHV-XC did not accept the sign-in".to_owned(),
            words: format!(
                "It takes the name you sign in with and the upload password from your DHV-XC \
                 profile, which is not the password you sign in with there.{}",
                said(&message)
            ),
        },
        dhvxc::Outcome::Failed { status, message } => LeagueAnswer {
            taken: false,
            title: "The DHV-XC could not take the flight".to_owned(),
            words: format!(
                "Its answer was not the one it gives for a flight it has (status {status}).{}",
                said(&message)
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::Value;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    fn core_with(text: &str) -> (TempFolder, std::sync::Arc<Core>, String) {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let id = core.import_flight(text.to_owned()).unwrap().id;
        (folder, core, id)
    }

    /// The made-up sample flight, of 4 July 2026, as its recorder would
    /// have signed it.
    fn signed_flight() -> String {
        let mut file = synthetic_flight();
        file.manufacturer = "XCT".to_owned();
        format!("{}G5a1b2c3d4e5f\r\n", igc::write(&file))
    }

    #[test]
    fn a_flight_is_handed_over_as_it_is_kept_with_what_the_leagues_ask_about() {
        let text = signed_flight();
        let (_folder, core, id) = core_with(&text);
        // Ten days after the flight.
        let now = Utc.with_ymd_and_hms(2026, 7, 14, 9, 0, 0).unwrap();

        let handover = core.league_handover(id.clone(), now.timestamp()).unwrap();

        // Byte for byte what was imported: the signature covers every one.
        assert_eq!(handover.file.igc_text, text);
        assert!(handover.file.name.ends_with(".igc"));
        assert!(handover.is_signed);
        assert_eq!(
            handover.signature,
            "The file carries the signature of its recorder (XCT). A league checks it when the \
             file arrives; this app cannot."
        );
        assert!(!handover.is_late);
        assert_eq!(
            handover.deadline,
            "XContest and the DHV-XC take a flight for their contests until 14 days after the \
             day it was flown: this one until the end of 18 Jul 2026, by UTC."
        );
        assert_eq!(
            handover.xcontest_claim_url,
            "https://www.xcontest.org/world/en/flight-claim/"
        );

        // The last second of the 18th is in time, the first of the 19th is
        // not.
        let midnight = Utc
            .with_ymd_and_hms(2026, 7, 19, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert!(
            !core
                .league_handover(id.clone(), midnight - 1)
                .unwrap()
                .is_late
        );
        let late = core.league_handover(id, midnight).unwrap();
        assert!(late.is_late);
        assert_eq!(
            late.deadline,
            "The 14 days in which the leagues take a flight for their contests ended with 18 \
             Jul 2026. XContest rejects a claim after them; the DHV-XC still takes the flight \
             unscored or as private."
        );
    }

    #[test]
    fn a_file_without_a_signature_is_said_to_score_nothing() {
        // As this app's own recorder writes it.
        let mut own = synthetic_flight();
        own.manufacturer = OWN_RECORDER.to_owned();
        let (_folder, core, id) = core_with(&igc::write(&own));
        let handover = core.league_handover(id, 0).unwrap();
        assert!(!handover.is_signed);
        assert_eq!(
            handover.signature,
            "Venti recorded this flight and does not sign its files yet. XContest lists such a \
             flight and gives it no points, and the DHV-XC does not score it."
        );

        // Another recorder's file that lost its signature on the way.
        let mut other = synthetic_flight();
        other.manufacturer = "FLY".to_owned();
        let (_folder, core, id) = core_with(&igc::write(&other));
        assert!(
            core.league_handover(id, 0)
                .unwrap()
                .signature
                .starts_with("This file carries no recorder's signature.")
        );

        // A flight the logbook does not have is not handed over.
        assert!(core.league_handover("nothing".to_owned(), 0).is_err());
    }

    #[test]
    fn the_request_to_the_dhv_xc_carries_the_file_and_what_the_pilot_typed() {
        let text = signed_flight();
        let (_folder, core, id) = core_with(&text);

        let request = core
            .dhvxc_upload_request(
                id.clone(),
                " mara.muster ".to_owned(),
                "made-up upload password ".to_owned(),
                Some(" Atom 3 ".to_owned()),
                false,
            )
            .unwrap();

        assert_eq!(request.url, "https://en.dhv-xc.de/api/v1/flights");
        assert_eq!(request.content_type, "application/json");
        let sent: Value = serde_json::from_slice(&request.body).unwrap();
        // The name is trimmed; a password is sent as it was typed.
        assert_eq!(sent["user"], "mara.muster");
        assert_eq!(sent["pass"], "made-up upload password ");
        assert_eq!(sent["glider"], "Atom 3");
        assert_eq!(sent["publish"], 0);
        assert_eq!(sent["igccontent"], text.as_str());
        assert_eq!(sent["igcname"], core.flight_file(id).unwrap().name.as_str());

        assert!(
            core.dhvxc_upload_request(
                "nothing".to_owned(),
                "mara".to_owned(),
                "x".to_owned(),
                None,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn the_answers_of_the_dhv_xc_are_put_into_words() {
        let taken = dhvxc_uploaded(
            200,
            br#"{"success":true,"message":"The flight has been published under ID 1506695"}"#
                .to_vec(),
        );
        assert!(taken.taken);
        assert_eq!(taken.title, "The DHV-XC has the flight");
        assert_eq!(
            taken.words,
            "Whether it is published or waits among your uploads there is for the DHV-XC to \
             say. The DHV-XC says: “The flight has been published under ID 1506695”"
        );

        let refused = dhvxc_uploaded(
            401,
            br#"{"success": false,"message": "Invalid username or upload password"}"#.to_vec(),
        );
        assert!(!refused.taken);
        assert_eq!(refused.title, "The DHV-XC did not accept the sign-in");
        assert!(
            refused
                .words
                .contains("upload password from your DHV-XC profile")
        );
        assert!(
            refused
                .words
                .ends_with("“Invalid username or upload password”")
        );

        let failed = dhvxc_uploaded(502, b"Bad Gateway".to_vec());
        assert!(!failed.taken);
        assert_eq!(failed.title, "The DHV-XC could not take the flight");
        assert_eq!(
            failed.words,
            "Its answer was not the one it gives for a flight it has (status 502)."
        );
    }
}
