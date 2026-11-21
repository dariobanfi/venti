//! The projection DWD's composite is drawn in, and where its grid lies.
//!
//! A frame says both itself, in its `/where` group: the projection as a
//! PROJ definition, the size of the grid and of a pixel, and the four
//! corners in degrees. The definition in the recorded frames is
//!
//! ```text
//! +proj=stere +lat_0=90 +lat_ts=60 +lon_0=10 +a=6378137 +b=6356752.3142451802
//! +no_defs +x_0=543196.83521776402 +y_0=3622588.8619310018 +units=m
//! ```
//!
//! a polar stereographic projection of the WGS84 ellipsoid, true to scale
//! at 60° N, with 10° E running straight down. The adapter reads the
//! definition from every frame and takes nothing but its form for granted:
//! a frame in another kind of projection is refused. The formulas are
//! Snyder's for the polar stereographic projection of an ellipsoid with a
//! standard parallel ("Map Projections: A Working Manual", USGS 1987).
//!
//! That the formulas and the reading of the corners are right was checked
//! in two ways against the recorded frames (2026-11-21). The four corners
//! a frame names land on the corners of its grid to within a millimetre,
//! which a test of this crate holds. And the edge of what the radars see
//! lies 150 km from the radars at Isen, Feldberg, Boostedt, Prötzel,
//! Borkum and Rostock, whose places DWD's format description gives: with
//! the rows read from the north, and only so.

use std::collections::BTreeMap;

/// A north polar stereographic projection of an ellipsoid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PolarStereographic {
    /// The ellipsoid's equatorial radius, metres.
    a: f64,
    /// Its eccentricity.
    e: f64,
    /// The meridian that runs straight down the map, degrees.
    lon_0: f64,
    /// What is added to a place's easting and northing, metres.
    x_0: f64,
    y_0: f64,
    /// The radius of a parallel on the map is `scale * t(latitude)`.
    scale: f64,
}

/// Snyder's `t` for a latitude in radians: the tangent of half the
/// colatitude, corrected for the ellipsoid's flattening.
fn t(e: f64, lat: f64) -> f64 {
    let s = e * lat.sin();
    (std::f64::consts::FRAC_PI_4 - lat / 2.0).tan() / ((1.0 - s) / (1.0 + s)).powf(e / 2.0)
}

impl PolarStereographic {
    /// Reads a PROJ definition of the form above. Refuses any other
    /// projection, any other pole, and any parameter it does not know:
    /// one that is left out of account would put the rain in the wrong
    /// place.
    pub(crate) fn from_proj(definition: &str) -> Result<Self, String> {
        let mut given: BTreeMap<&str, &str> = BTreeMap::new();
        for word in definition.split_whitespace() {
            let word = word
                .strip_prefix('+')
                .ok_or_else(|| format!("{word:?} in the projection's definition"))?;
            let (key, value) = word.split_once('=').unwrap_or((word, ""));
            if given.insert(key, value).is_some() {
                return Err(format!("the projection names {key} twice"));
            }
        }
        let mut number = |key: &str| -> Result<f64, String> {
            given
                .remove(key)
                .ok_or_else(|| format!("the projection does not say its {key}"))?
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("the projection's {key} is no number"))
        };
        let (lat_0, lat_ts, lon_0) = (number("lat_0")?, number("lat_ts")?, number("lon_0")?);
        let (a, b) = (number("a")?, number("b")?);
        let (x_0, y_0) = (number("x_0")?, number("y_0")?);
        if given.remove("proj") != Some("stere") {
            return Err("the projection is not stereographic (+proj=stere)".to_owned());
        }
        if given.remove("units").is_some_and(|units| units != "m") {
            return Err("the projection is not in metres".to_owned());
        }
        given.remove("no_defs");
        if let Some(unknown) = given.keys().next() {
            return Err(format!(
                "the projection has a parameter this adapter does not know: {unknown}"
            ));
        }
        // Both checked numbers; the comparison is with what was written.
        #[allow(clippy::float_cmp)]
        if lat_0 != 90.0 {
            return Err("the projection is not centred on the north pole".to_owned());
        }
        if !(a > 6.0e6 && a < 7.0e6 && b > 6.0e6 && b <= a) {
            return Err("the projection's ellipsoid is not the earth's".to_owned());
        }
        if !(0.0..90.0).contains(&lat_ts) || !(-180.0..=180.0).contains(&lon_0) {
            return Err("the projection's standard parallel or meridian is off the map".to_owned());
        }
        let e = (1.0 - (b * b) / (a * a)).sqrt();
        let true_scale = lat_ts.to_radians();
        let m = true_scale.cos() / (1.0 - (e * true_scale.sin()).powi(2)).sqrt();
        Ok(Self {
            a,
            e,
            lon_0,
            x_0,
            y_0,
            scale: a * m / t(e, true_scale),
        })
    }

    /// The radius on the map of the parallel of a latitude, metres.
    pub(crate) fn radius(&self, lat_deg: f64) -> f64 {
        self.scale * t(self.e, lat_deg.to_radians())
    }

    /// Where a place lies on the map: easting and northing in metres.
    pub(crate) fn forward(&self, lon_deg: f64, lat_deg: f64) -> (f64, f64) {
        self.at(self.radius(lat_deg), lon_deg)
    }

    /// Where the place lies whose parallel has this radius on the map.
    pub(crate) fn at(&self, radius: f64, lon_deg: f64) -> (f64, f64) {
        let turned = (lon_deg - self.lon_0).to_radians();
        (
            self.x_0 + radius * turned.sin(),
            self.y_0 - radius * turned.cos(),
        )
    }

    /// The ellipsoid's equatorial radius, metres.
    #[cfg(test)]
    pub(crate) fn equatorial_radius(&self) -> f64 {
        self.a
    }
}

/// Where a frame's grid lies on the map of its projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// The projection.
    pub(crate) projection: PolarStereographic,
    /// The outer corner of the pixel in the grid's north-west, metres.
    pub(crate) left: f64,
    pub(crate) top: f64,
    /// The size of a pixel, metres.
    pub(crate) x_scale: f64,
    pub(crate) y_scale: f64,
    /// How many pixels a row has, and how many rows there are.
    pub(crate) columns: usize,
    pub(crate) rows: usize,
}

/// A corner of a grid, as a frame names it: longitude and latitude.
pub(crate) type Corner = (f64, f64);

/// How far a corner a frame names may lie from where the grid's size puts
/// it, in metres. The recorded frames are within a millimetre.
const CORNER_TOLERANCE_M: f64 = 1.0;

impl Placement {
    /// The placement a frame's own words give: its projection, the size of
    /// its grid and of a pixel, and its four corners, each the outer
    /// corner of a corner pixel as ODIM defines them. The grid is hung on
    /// the upper left corner; the other three must then lie where the
    /// grid's size puts them, or the frame contradicts itself and is
    /// refused.
    pub(crate) fn new(
        projection: PolarStereographic,
        (columns, rows): (usize, usize),
        (x_scale, y_scale): (f64, f64),
        upper_left: Corner,
        others: [(&str, Corner, (f64, f64)); 3],
    ) -> Result<Self, String> {
        if !(x_scale > 0.0 && y_scale > 0.0 && x_scale.is_finite() && y_scale.is_finite()) {
            return Err("a pixel without a size".to_owned());
        }
        let (left, top) = projection.forward(upper_left.0, upper_left.1);
        // The sizes are small whole numbers.
        #[allow(clippy::cast_precision_loss)]
        let (width, height) = (columns as f64 * x_scale, rows as f64 * y_scale);
        for (name, (lon, lat), (right_of, below)) in others {
            let (x, y) = projection.forward(lon, lat);
            let off = (x - (left + right_of * width)).hypot(y - (top - below * height));
            if off.is_nan() || off > CORNER_TOLERANCE_M {
                return Err(format!(
                    "the {name} corner lies {off:.0} m from where the grid's size puts it"
                ));
            }
        }
        Ok(Self {
            projection,
            left,
            top,
            x_scale,
            y_scale,
            columns,
            rows,
        })
    }

    /// The pixel a place on the map falls into, as column and row, the
    /// rows counted from the north. `None` outside the grid.
    pub(crate) fn pixel(&self, (x, y): (f64, f64)) -> Option<(usize, usize)> {
        let column = ((x - self.left) / self.x_scale).floor();
        let row = ((self.top - y) / self.y_scale).floor();
        // Both are checked to be whole numbers within the grid.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        (column >= 0.0 && row >= 0.0 && column < self.columns as f64 && row < self.rows as f64)
            .then_some((column as usize, row as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The definition in the frames recorded on 2026-11-21.
    pub(crate) const RECORDED: &str = "+proj=stere +lat_0=90 +lat_ts=60 +lon_0=10 +a=6378137 \
        +b=6356752.3142451802 +no_defs +x_0=543196.83521776402 +y_0=3622588.8619310018 +units=m";

    #[test]
    fn the_recorded_definition_is_read_and_a_circle_of_latitude_is_true_at_sixty_north() {
        let projection = PolarStereographic::from_proj(RECORDED).unwrap();
        assert!((projection.equatorial_radius() - 6_378_137.0).abs() < 1e-9);

        // On the meridian that runs straight down, a place lies below the
        // pole by the radius of its parallel.
        let (x, y) = projection.forward(10.0, 60.0);
        assert!((x - 543_196.835).abs() < 0.001);
        // At the true-scale parallel a degree of longitude is as long on
        // the map as on the ellipsoid: 55.80 km at 60° N on WGS84.
        let (x1, y1) = projection.forward(11.0, 60.0);
        let degree = (x1 - x).hypot(y1 - y);
        assert!((degree - 55_800.0).abs() < 10.0, "{degree}");
    }

    #[test]
    fn the_corners_a_frame_names_lie_at_the_corners_of_its_grid() {
        let projection = PolarStereographic::from_proj(RECORDED).unwrap();
        // /where of the frames recorded on 2026-11-21.
        let placement = Placement::new(
            projection,
            (1100, 1200),
            (1000.0, 1000.0),
            (1.463_301_51, 55.862_087_11),
            [
                ("upper right", (18.731_616_45, 55.845_438_56), (1.0, 0.0)),
                ("lower left", (3.566_994_635, 45.696_425_38), (0.0, 1.0)),
                ("lower right", (16.580_869_35, 45.684_605_78), (1.0, 1.0)),
            ],
        )
        .unwrap();

        // The grid hangs half a pixel up and to the left of the map's
        // origin: the middle of the first pixel is the origin.
        assert!((placement.left + 500.0).abs() < 0.01, "{}", placement.left);
        assert!((placement.top - 500.0).abs() < 0.01, "{}", placement.top);

        // Isen's radar, 48°10'28.9" N 12°06'06.4" E by DWD's format
        // description, is in column 710 of row 930 from the north.
        let isen = projection.forward(
            12.0 + 6.0 / 60.0 + 6.4 / 3600.0,
            48.0 + 10.0 / 60.0 + 28.9 / 3600.0,
        );
        assert_eq!(placement.pixel(isen), Some((710, 930)));
        // The grid reaches further than the radars: Vienna is a pixel of
        // it, near its eastern edge. Budapest lies beyond.
        assert_eq!(
            placement.pixel(projection.forward(16.37, 48.21)),
            Some((1048, 901))
        );
        assert_eq!(placement.pixel(projection.forward(19.04, 47.5)), None);
        assert_eq!(placement.pixel(projection.forward(0.0, 50.0)), None);
    }

    #[test]
    fn a_grid_whose_corners_contradict_its_size_is_refused() {
        let projection = PolarStereographic::from_proj(RECORDED).unwrap();
        let said = Placement::new(
            projection,
            // A hundred columns fewer than the corners span.
            (1000, 1200),
            (1000.0, 1000.0),
            (1.463_301_51, 55.862_087_11),
            [
                ("upper right", (18.731_616_45, 55.845_438_56), (1.0, 0.0)),
                ("lower left", (3.566_994_635, 45.696_425_38), (0.0, 1.0)),
                ("lower right", (16.580_869_35, 45.684_605_78), (1.0, 1.0)),
            ],
        )
        .unwrap_err();
        assert!(said.contains("upper right corner lies 100000 m"), "{said}");
    }

    #[test]
    fn another_projection_is_refused_and_so_is_a_parameter_nobody_reads() {
        let refuse = |definition: &str| PolarStereographic::from_proj(definition).unwrap_err();
        assert!(refuse(&RECORDED.replace("stere", "laea")).contains("not stereographic"));
        assert!(refuse(&RECORDED.replace("lat_0=90", "lat_0=-90")).contains("north pole"));
        assert!(refuse(&format!("{RECORDED} +k=0.9")).contains("does not know: k"));
        assert!(refuse(&RECORDED.replace("+units=m", "+units=km")).contains("not in metres"));
        assert!(refuse(&RECORDED.replace("+lon_0=10 ", "")).contains("does not say its lon_0"));
        assert!(refuse(&RECORDED.replace("a=6378137", "a=1")).contains("not the earth's"));
        assert!(refuse("proj=stere").contains("in the projection's definition"));
    }
}
