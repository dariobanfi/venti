//! Prints the made-up sample flight as an IGC file.
//!
//! ```text
//! cargo run -p venti-flight --example sample_flight > apps/ios/Venti/Resources/sample.igc
//! ```

use venti_flight::igc;
use venti_flight::testing::synthetic_flight;

fn main() {
    print!("{}", igc::write(&synthetic_flight()));
}
