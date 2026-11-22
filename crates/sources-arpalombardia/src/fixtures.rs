//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations of the first region's box, in the upper Valtellina and
//! at Livigno, with what the portal had of them at 19:05 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18. This
//! source was added on 2026-11-22. The portal keeps the values of the whole
//! month, so the values of that evening could be asked for on the 22nd,
//! with the address the adapter writes for 19:05 on the 18th; but that
//! address has no end in time, and the portal answered with every value
//! since, up to the 22nd. So the sample is that answer cut to the values
//! the portal had at 19:05 UTC on the 18th: by its own record of when it
//! took each ten minutes in (the rows' `:updated_at`), the newest it had
//! then was of 18:30 by its clock, 17:30 UTC, taken in at 17:42 UTC; the
//! next arrived at 23:25 UTC. `tests/fixtures/NOTICE.txt` says how.
//!
//! The answers of 2026-11-22 are in `tests/fixtures` as well. They are what
//! the crate's tests read the adapter against, and they are not compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::{ObservationSource, PlanCtx};

use crate::ArpaLombardia;

const SENSORS: &[u8] = include_bytes!("../tests/fixtures/sensors_evening.json");
const VALUES: &[u8] = include_bytes!("../tests/fixtures/values_evening.json");

/// The moment the sample is for.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The area the sample answers for: the first region's.
///
/// # Panics
///
/// Never: the bounds are constants.
pub fn area() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box")
}

/// The sample, with the addresses the adapter asks at [`evening_at`] for
/// [`area`]: Livigno Mottolino at 1869 m, Valdisotto Bormio 2000 at
/// 2328 m, and Valfurva S. Caterina at 1730 m, which has no hygrometer.
///
/// The address of the values is written from the list of sensors, as a run
/// writes it, so it is read here with the adapter.
pub fn evening() -> Vec<(String, &'static [u8])> {
    let source = ArpaLombardia::new();
    let ctx = PlanCtx {
        now: evening_at(),
        area: area(),
    };
    let mut payloads = Vec::new();
    for request in source.plan(&ctx) {
        let values = source
            .parse(&request, SENSORS, &ctx)
            .map(|parsed| parsed.follow_up)
            .unwrap_or_default();
        payloads.push((request.url, SENSORS));
        payloads.extend(values.into_iter().map(|request| (request.url, VALUES)));
    }
    payloads
}
