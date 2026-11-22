//! What the server collects: the sources of the configured regions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use venti_domain::{BoundingBox, SourceId};
use venti_regions::{RegionError, RegionPack};
use venti_sources::{Kind, Manifest, Registry};

/// One source to collect, and the area to collect it for.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// The source.
    pub source: SourceId,
    /// What it is collected for: observations, airspace, flying sites or
    /// rain radar. For a source of forecasts the job is the regular look
    /// at the forecast service, and the area is where forecasts are
    /// answered.
    pub kind: Kind,
    /// The smallest box that covers every configured region listing the
    /// source.
    pub area: BoundingBox,
}

impl Job {
    /// The declared facts about the job's source.
    ///
    /// # Panics
    ///
    /// Panics if the registry has no adapter of the source for the job's
    /// kind. Jobs come from region packs that were checked against the
    /// registry when the plan was made.
    pub fn manifest<'a>(&self, registry: &'a Registry) -> &'a Manifest {
        registry
            .manifest(&self.source, self.kind)
            .unwrap_or_else(|| panic!("the plan names {}, which has no adapter", self.source))
    }
}

/// Why the configured regions cannot be served.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    /// A region pack could not be read.
    #[error("cannot read {}: {error}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        error: std::io::Error,
    },
    /// A region pack is invalid, or names a source this build has no adapter
    /// for.
    #[error("{}: {error}", path.display())]
    Region {
        /// The file.
        path: PathBuf,
        /// What is wrong with it.
        error: RegionError,
    },
    /// No region lists any source.
    #[error("the configured regions list no source to collect")]
    Empty,
    /// A source's licence does not allow serving its data onward, which is
    /// all this server does with data.
    #[error(
        "source {0} may not be redistributed under its licence; the API cannot keep it back yet"
    )]
    NotRedistributable(SourceId),
    /// The regions list one source for two kinds of data. The run history
    /// and the health report are kept per source.
    #[error(
        "source {0} is listed for two kinds of data; \
         the server collects a source for one kind"
    )]
    TwoKinds(SourceId),
}

/// Reads the region packs at `paths`, each checked against the registry.
/// [`plan`] makes the jobs of them.
pub fn read(paths: &[PathBuf], registry: &Registry) -> Result<Vec<RegionPack>, PlanError> {
    paths.iter().map(|path| read_pack(path, registry)).collect()
}

/// The sources of observations in the order they are preferred where two of
/// them have a station at the same place: the `priority` lists of the
/// regions, one after the other.
///
/// A source keeps the place the first region gave it. An order that
/// differs from region to region is not built: the three packs of the Alps
/// name none that does. A source that no list names is not in the answer
/// and comes after all that are.
pub fn station_priority(packs: &[RegionPack]) -> Vec<SourceId> {
    let mut order: Vec<SourceId> = Vec::new();
    for source in packs.iter().flat_map(|pack| &pack.observations.priority) {
        if !order.contains(source) {
            order.push(source.clone());
        }
    }
    order
}

fn read_pack(path: &Path, registry: &Registry) -> Result<RegionPack, PlanError> {
    let text = std::fs::read_to_string(path).map_err(|error| PlanError::Read {
        path: path.to_owned(),
        error,
    })?;
    let region = |error| PlanError::Region {
        path: path.to_owned(),
        error,
    };
    let pack = RegionPack::from_toml(&text).map_err(region)?;
    pack.check_against(registry).map_err(region)?;
    for source in pack.forecasts_without_adapter(registry) {
        tracing::warn!(
            "region {} names forecasts from {source}, and the server was not told where \
             that service is: no forecasts from it",
            pack.id
        );
    }
    Ok(pack)
}

/// Plans the sources of region packs that were already checked against the
/// registry: one job per source, sorted by identifier.
///
/// A source is collected once however many regions list it, because a
/// provider's request limits apply to the source, not to a region. Its area
/// is one box around all those regions.
///
/// A region's forecast source is planned only if the registry has its
/// adapter, which it has when the server was told where the forecast
/// service is. Without it the server runs and has no forecasts.
pub fn plan(packs: &[RegionPack], registry: &Registry) -> Result<Vec<Job>, PlanError> {
    let mut jobs: BTreeMap<SourceId, Job> = BTreeMap::new();
    for pack in packs {
        let forecasts: Vec<SourceId> = pack
            .forecasts
            .sources
            .iter()
            .filter(|source| registry.forecasts(source).is_some())
            .cloned()
            .collect();
        let listed = [
            (Kind::Observations, &pack.observations.sources),
            (Kind::Airspace, &pack.airspace.sources),
            (Kind::Sites, &pack.sites.sources),
            (Kind::Cables, &pack.cables.sources),
            (Kind::Forecasts, &forecasts),
            (Kind::Radar, &pack.radar.sources),
        ];
        for (kind, sources) in listed {
            for source in sources {
                let job = jobs.entry(source.clone()).or_insert_with(|| Job {
                    source: source.clone(),
                    kind,
                    area: pack.bounds,
                });
                if job.kind != kind {
                    return Err(PlanError::TwoKinds(source.clone()));
                }
                job.area = job.area.cover(&pack.bounds);
            }
        }
    }
    if jobs.is_empty() {
        return Err(PlanError::Empty);
    }
    for job in jobs.values() {
        let redistributable = registry
            .manifest(&job.source, job.kind)
            .is_some_and(|manifest| manifest.licence.redistribution);
        if !redistributable {
            return Err(PlanError::NotRedistributable(job.source.clone()));
        }
    }
    Ok(jobs.into_values().collect())
}

/// The networks of live positions that region packs name, each with one
/// box around every region that names it, sorted by source. The server
/// listens to each over a connection of its own; they are not collected.
pub fn live(packs: &[RegionPack]) -> Vec<(SourceId, BoundingBox)> {
    let mut networks: BTreeMap<SourceId, BoundingBox> = BTreeMap::new();
    for pack in packs {
        for source in &pack.live.sources {
            networks
                .entry(source.clone())
                .and_modify(|area| *area = area.cover(&pack.bounds))
                .or_insert(pack.bounds);
        }
    }
    networks.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use venti_sources::{ObservationSource, Parsed, PlanCtx, Request, SourceError};

    use super::*;
    use crate::adapters;

    fn pack(id: &str, bounds: [f64; 4], sources: &[&str]) -> RegionPack {
        pack_with_airspace(id, bounds, sources, &[])
    }

    fn pack_with_airspace(
        id: &str,
        bounds: [f64; 4],
        observations: &[&str],
        airspace: &[&str],
    ) -> RegionPack {
        let [south, west, north, east] = bounds;
        let list = |sources: &[&str]| {
            let sources: Vec<String> = sources.iter().map(|s| format!("{s:?}")).collect();
            sources.join(", ")
        };
        let text = format!(
            "id = {id:?}\nname = {id:?}\nlanguages = [\"de\"]\n\
             [bounds]\nsouth = {south}\nwest = {west}\nnorth = {north}\neast = {east}\n\
             [observations]\nsources = [{}]\n\
             [airspace]\nsources = [{}]\n",
            list(observations),
            list(airspace)
        );
        RegionPack::from_toml(&text).unwrap()
    }

    #[test]
    fn sites_are_planned_and_forecasts_only_where_their_service_is_known() {
        let text = "id = \"alps\"\nname = \"alps\"\nlanguages = [\"de\"]\n\
             [bounds]\nsouth = 46.3\nwest = 9.4\nnorth = 48.5\neast = 17.2\n\
             [observations]\nsources = [\"dwd-10min\"]\n\
             [sites]\nsources = [\"osm-free-flying\"]\n\
             [forecasts]\nsources = [\"open-meteo\"]\n";
        let packs = [RegionPack::from_toml(text).unwrap()];

        // This registry was not told where a forecast service is: the
        // region is planned without its forecasts.
        let jobs = plan(&packs, &adapters::registry()).unwrap();

        let planned: Vec<(&str, Kind)> = jobs
            .iter()
            .map(|job| (job.source.as_str(), job.kind))
            .collect();
        assert_eq!(
            planned,
            [
                ("dwd-10min", Kind::Observations),
                ("osm-free-flying", Kind::Sites)
            ]
        );
        assert_eq!(jobs[1].area, packs[0].bounds);
        assert_eq!(
            jobs[1].manifest(&adapters::registry()).licence.name,
            "ODbL 1.0"
        );
    }

    #[test]
    fn a_source_listed_by_two_regions_runs_once_over_both() {
        let packs = [
            pack(
                "alps",
                [46.3, 9.4, 48.5, 17.2],
                &["dwd-10min", "geosphere-tawes"],
            ),
            pack("north", [52.0, 8.0, 54.0, 12.0], &["dwd-10min"]),
        ];

        let jobs = plan(&packs, &adapters::registry()).unwrap();

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].source.as_str(), "dwd-10min");
        assert_eq!(
            jobs[0].area,
            BoundingBox::new(46.3, 8.0, 54.0, 17.2).unwrap()
        );
        assert_eq!(jobs[1].source.as_str(), "geosphere-tawes");
        assert_eq!(jobs[1].area, packs[0].bounds);
    }

    #[test]
    fn airspace_sources_are_planned_beside_the_observation_sources() {
        let alps = [46.3, 9.4, 48.5, 17.2];
        let packs = [
            pack_with_airspace("alps", alps, &["dwd-10min"], &["openaip"]),
            pack_with_airspace("north", [52.0, 8.0, 54.0, 12.0], &[], &["openaip"]),
        ];

        let jobs = plan(&packs, &adapters::registry()).unwrap();

        let planned: Vec<(&str, Kind)> = jobs
            .iter()
            .map(|job| (job.source.as_str(), job.kind))
            .collect();
        assert_eq!(
            planned,
            [
                ("dwd-10min", Kind::Observations),
                ("openaip", Kind::Airspace)
            ]
        );
        assert_eq!(jobs[0].area, packs[0].bounds);
        assert_eq!(
            jobs[1].area,
            BoundingBox::new(46.3, 8.0, 54.0, 17.2).unwrap()
        );
        assert_eq!(jobs[1].manifest(&adapters::registry()).provider, "openAIP");

        // A region of airspace alone is a plan too.
        let only = [pack_with_airspace("alps", alps, &[], &["openaip"])];
        assert_eq!(plan(&only, &adapters::registry()).unwrap().len(), 1);
    }

    #[test]
    fn a_source_is_collected_for_one_kind_of_data() {
        // The pack is not checked against the registry here; the plan's own
        // rule is what is tested.
        let packs = [pack_with_airspace(
            "alps",
            [46.3, 9.4, 48.5, 17.2],
            &["dwd-10min"],
            &["dwd-10min"],
        )];
        assert!(matches!(
            plan(&packs, &adapters::registry()),
            Err(PlanError::TwoKinds(source)) if source.as_str() == "dwd-10min"
        ));
    }

    /// An adapter for a source that may be used but not served onward.
    struct Closed(Manifest);

    impl ObservationSource for Closed {
        fn manifest(&self) -> &Manifest {
            &self.0
        }
        fn plan(&self, _: &PlanCtx) -> Vec<Request> {
            Vec::new()
        }
        fn parse(&self, request: &Request, _: &[u8], _: &PlanCtx) -> Result<Parsed, SourceError> {
            Err(SourceError::UnknownRequest(request.tag.clone()))
        }
    }

    #[test]
    fn a_source_that_may_not_be_served_onward_is_refused() {
        let manifest = r#"
            id = "closed"
            provider = "Closed Weather"
            kinds = ["observations"]
            countries = ["DE"]
            cadence_seconds = 600
            auth = "api-key"
            [licence]
            name = "Custom"
            commercial = false
            redistribution = false
            share_alike = false
            attribution = "Closed Weather"
            link = "https://example.org"
            verified_on = "2026-11-19"
            verified_from = ["https://example.org/terms"]
        "#;
        let mut registry = adapters::registry();
        let closed = Closed(Manifest::from_toml(manifest).unwrap());
        registry.register_observations(Box::new(closed)).unwrap();
        let packs = [pack(
            "alps",
            [46.3, 9.4, 48.5, 17.2],
            &["dwd-10min", "closed"],
        )];

        let refused = plan(&packs, &registry).unwrap_err();

        assert!(
            matches!(refused, PlanError::NotRedistributable(source) if source.as_str() == "closed")
        );
    }

    #[test]
    fn regions_without_sources_are_refused() {
        let packs = [pack("empty", [46.3, 9.4, 48.5, 17.2], &[])];
        assert!(matches!(
            plan(&packs, &adapters::registry()),
            Err(PlanError::Empty)
        ));
        assert!(matches!(
            plan(&[], &adapters::registry()),
            Err(PlanError::Empty)
        ));
    }

    #[test]
    fn the_first_region_loads_from_its_file() {
        let path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../regions/alps-de-at/region.toml"
        ));
        let packs = read(&[path], &adapters::registry()).unwrap();
        let jobs = plan(&packs, &adapters::registry()).unwrap();
        let sources: Vec<&str> = jobs.iter().map(|job| job.source.as_str()).collect();
        assert_eq!(
            sources,
            [
                "awc-metar",
                "dwd-10min",
                "dwd-radar-ry",
                "geosphere-inca-rr",
                "geosphere-tawes",
                "openaip",
                "osm-cables",
                "osm-free-flying",
                "suedtirol-meteo"
            ]
        );

        // Where a weather service and an airfield's report measure at one
        // place, the weather service's station is the one to hand out: it
        // measures every ten minutes, with gusts, humidity and pressure.
        // So the three weather services come before the reports.
        let preferred: Vec<String> = station_priority(&packs)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            preferred,
            [
                "dwd-10min",
                "geosphere-tawes",
                "suedtirol-meteo",
                "awc-metar"
            ]
        );

        let missing = PathBuf::from("regions/nowhere/region.toml");
        assert!(matches!(
            read(&[missing], &adapters::registry()),
            Err(PlanError::Read { .. })
        ));
    }

    /// The three packs of the Alps, as a server reads them that is given
    /// all three.
    fn the_alps() -> Vec<RegionPack> {
        let paths: Vec<PathBuf> = ["alps-de-at", "alps-west", "alps-south"]
            .iter()
            .map(|id| {
                PathBuf::from(format!(
                    "{}/../../regions/{id}/region.toml",
                    env!("CARGO_MANIFEST_DIR")
                ))
            })
            .collect();
        read(&paths, &adapters::registry()).unwrap()
    }

    #[test]
    fn the_three_regions_tile_the_whole_alps() {
        let packs = the_alps();
        let whole = BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap();
        let area = |b: &BoundingBox| (b.north() - b.south()) * (b.east() - b.west());

        // One box around the three is the whole Alps, and the three fill
        // it without lying over each other: they share edges and no more.
        let around = packs
            .iter()
            .skip(1)
            .fold(packs[0].bounds, |around, pack| around.cover(&pack.bounds));
        assert_eq!(around, whole);
        let filled: f64 = packs.iter().map(|pack| area(&pack.bounds)).sum();
        assert!((filled - area(&whole)).abs() < 1e-9, "{filled}");
        for (index, one) in packs.iter().enumerate() {
            for other in &packs[index + 1..] {
                let (a, b) = (&one.bounds, &other.bounds);
                let shared = (a.north().min(b.north()) - a.south().max(b.south())).max(0.0)
                    * (a.east().min(b.east()) - a.west().max(b.west())).max(0.0);
                assert!(shared < 1e-9, "{} lies over {}", one.id, other.id);
            }
        }
    }

    #[test]
    fn what_all_three_regions_name_is_collected_once_over_the_whole_alps() {
        let packs = the_alps();
        let whole = BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap();
        let first = packs[0].bounds;

        let jobs = plan(&packs, &adapters::registry()).unwrap();
        let area_of = |source: &str| {
            jobs.iter()
                .find(|job| job.source.as_str() == source)
                .unwrap_or_else(|| panic!("{source} is not planned"))
                .area
        };

        // No source is planned that the first region alone does not have:
        // the new packs came with no adapter.
        assert_eq!(jobs.len(), 9);

        // The sites and the airfields' reports are named by all three and
        // are one job over one box around them. So is DWD, which the
        // western pack names for the Black Forest: a job's area is one box
        // around its regions, and Germany's stations end where Germany
        // does.
        for source in ["osm-free-flying", "awc-metar", "dwd-10min"] {
            assert_eq!(area_of(source), whole, "{source}");
        }
        // South Tyrol's stations with the southern pack: Salurn at
        // 46.24° N is inside.
        assert_eq!(
            area_of("suedtirol-meteo"),
            BoundingBox::new(43.5, 9.4, 48.5, 17.2).unwrap()
        );

        // What only the first region names stays in its box. For airspace
        // that is the rule to keep: openAIP's manifest has no France, and
        // a layer that says it is whole over a box that takes in France
        // would be airspace nobody is warned about. The cables' answer for
        // the whole Alps is too large for one request, and the radars'
        // pictures and the thermal map are made over the box of their job.
        for source in [
            "openaip",
            "osm-cables",
            "dwd-radar-ry",
            "geosphere-inca-rr",
            "geosphere-tawes",
        ] {
            assert_eq!(area_of(source), first, "{source}");
        }
        for pack in &packs[1..] {
            assert!(pack.airspace.is_empty(), "{}", pack.id);
            assert!(pack.cables.is_empty(), "{}", pack.id);
            assert!(pack.forecasts.is_empty(), "{}", pack.id);
            assert!(pack.radar.is_empty(), "{}", pack.id);
        }

        // The Open Glider Network is listened to over the whole box.
        let networks = live(&packs);
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].0.as_str(), "ogn");
        assert_eq!(networks[0].1, whole);

        // The order of stations at one place is the first region's, with
        // nothing new in it.
        let preferred: Vec<String> = station_priority(&packs)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            preferred,
            [
                "dwd-10min",
                "geosphere-tawes",
                "suedtirol-meteo",
                "awc-metar"
            ]
        );
    }
}
