//! Handing a flight's file back out of the logbook.

use venti_flight::igc;

use crate::{Core, CoreError, LogbookError};

/// A flight file ready to be shared or saved elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlightFile {
    /// A file name in the form flight recorders use:
    /// `2026-07-04-XVE-001-01.igc`.
    pub name: String,
    /// The file's text, exactly as it was imported or recorded.
    pub igc_text: String,
}

#[uniffi::export]
impl Core {
    /// The stored file of one flight, with a name to save it under.
    pub fn flight_file(&self, id: String) -> Result<FlightFile, CoreError> {
        let igc_text = self.logbook.flight_text(&id)?;
        let file = igc::parse(&igc_text).map_err(LogbookError::Invalid)?;
        // Only letters and digits of the recorder's codes may reach a file
        // name.
        let code = |text: &str| -> String {
            let kept: String = text.chars().filter(char::is_ascii_alphanumeric).collect();
            if kept.is_empty() {
                "000".to_owned()
            } else {
                kept
            }
        };
        Ok(FlightFile {
            name: format!(
                "{}-{}-{}-01.igc",
                file.date.format("%Y-%m-%d"),
                code(&file.manufacturer),
                code(&file.recorder_id)
            ),
            igc_text,
        })
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    #[test]
    fn a_flight_comes_back_out_unchanged_under_a_recorder_style_name() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let text = igc::write(&synthetic_flight());
        let id = core.import_flight(text.clone()).unwrap().id;

        let file = core.flight_file(id).unwrap();

        assert_eq!(file.name, "2026-07-04-XVE-000-01.igc");
        assert_eq!(file.igc_text, text);
        assert!(core.flight_file("nothing".to_owned()).is_err());
    }
}
