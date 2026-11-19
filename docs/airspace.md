# Airspace

Last revised: 2026-11-19. Facts marked *(to verify)* could not be checked against a primary source and must not be relied on until they are.

Airspace is safety-relevant. This document describes the part of Venti that knows about it: the canonical record, the reader for OpenAir files, the questions the cockpit and the map can ask, and the warnings. All of it is deterministic code in Rust that works offline, reads no clock and needs no assistant.

One rule runs through everything below: **a wrong "you are clear" is the worst thing the app can say.** Where something is unknown, the airspace is taken as reaching toward the pilot and the answer says that it is uncertain. Every such rule is listed under [Conservative rules](#conservative-rules).

Everything is advisory. A record names its source and carries the age of its data, and a screen that shows a warning must show both. The pilot remains responsible for the flight.

## Where it lives

| Crate | What it holds |
|---|---|
| `venti-domain` | The canonical record, `Airspace`, with its checks |
| `venti-airspace` | The OpenAir reader, the index with its questions, the warning table and the warning engine. Depends on `venti-domain` only |
| `venti-sources` | `AirspaceSource`, the adapter trait for providers, and `run_airspace`, the pipeline that runs one |
| `venti-sources-openaip` | The first provider: openAIP's airspace of Germany and Austria. See [data-sources.md](data-sources.md#airspace) |
| `venti-store`, `venti-server` | Keep each source's airspace as one whole set and serve it at `GET /v1/airspace`. See [api.md](api.md#get-v1airspacebboxwestsoutheastnorth) |

A client reaches airspace through `venti-airspace`; nothing in it touches a network or a provider. The other rows are server-side: they produce the records and hand them out.

**On the server (since 2026-11-19).** The server fetches openAIP's files for Germany and Austria once a day, keeps what reaches into its regions as one set, and serves it in the canonical record, the one described below and the one the apps already read from OpenAir files. Three rules of this document continue there:

- *No half set.* The stored set is replaced only by a run in which every file arrived. A run that got Germany and not Austria changes nothing, and the answer keeps saying when the set it serves was fetched.
- *Source and age travel with the data.* An answer is in layers by source. Each layer says when the server fetched it and how old the source's files are, each record says the date of its own file, and the answer carries the source's credit and openAIP's notice that the data is not certified.
- *No data is not "no airspace".* The server holds Germany and Austria, and of those only what reaches into its regions' box. Every layer of an answer says so itself: the box inside which the set is whole and the countries it holds ([api.md](api.md#where-a-layer-is-whole)). Outside that box an empty answer says nothing about the air, and inside it nothing about a country that is not named.

No app reads the endpoint yet; the pilot's own OpenAir files remain the app's only airspace until the core does.

**In the app (since 2026-11-19).** Until that bundle exists, the pilot brings airspace in as OpenAir files. The core (`crates/core/src/airspace.rs`) keeps each file as it came, under the day it was brought in, reads all of them into one index, and gives a map the airspaces in view and the cockpit's session the warnings: the session feeds the engine every position, at most once a second, with the flight computer's altitude, pressure altitude, speed, course and averaged climb, and no ground elevation. The wording of a warning ("Inside CTR Innsbruck", "CTR Innsbruck in 30 s", "CTR Innsbruck 300 m NE", "Climbing into ...") is made in the core, so every shell says the same. The iPhone draws the airspace on both maps, shows the most urgent warning over the cockpit's map, says a new or more urgent one aloud, and offers "Seen: quiet for 10 minutes" (acknowledge) and "Does not apply today" (ignore). What a file left out is said at import and stays on the file's row in red. Not done: the ground elevation, so every limit above ground is "possibly"; activations; the airspace side view; anything on the web. Android has the same since the same day: the files, both maps, the banner, the sheet and the voice.

**Airspace from the pilot's server (iPhone, since 2026-11-19).** A pilot with a Venti server need not hunt for files. The core (`crates/core/src/kept_airspace.rs`) says what to ask (`GET /v1/airspace` for a box of two degrees of latitude and three of longitude to each side of a place, about 220 km), reads the answer and keeps it on the device: an index, and the records of each source in a file of their own, never merged with another source's or with the pilot's files. One answer is kept at a time; a newer one replaces it, an answer without airspace changes nothing. The app asks when nothing is kept, when what is kept is a day old or from another server, and when the place is within about 80 km of the kept area's edge or beyond it; it asks around what the map shows and around the first position of a flight, and otherwise not at all. From then on the kept airspace is part of the one set the maps draw and the cockpit warns from, without a network. An airspace the pilot's own file already gave is not taken twice.

Two things keep this honest. *Where it ends:* the server says for which area each layer is complete (`covers`), and the cockpit takes "no airspace near" from kept airspace only inside that area and inside the box that was asked for. Elsewhere, and for a server that does not say, the cockpit says that nothing is checked there, as it does where the pilot's files do not reach. An answer without airspace for a place inside that area is kept too: there "none" is knowledge. One gap is open: the area is a box and the server fetches countries (`countries`, today Germany and Austria), so inside the box but across a border the kept set is empty and the cockpit still says that it is checking. The screen that lists the airspace names the countries and says so; the fix is for the server to fetch every country its box takes in, which is under way. *Whose it is:* every warning says whether its airspace is from a file of the pilot's or from a provider by way of the server, and as of when; the screen that lists the airspace shows the provider's credit, its licence and its notice in full ("OpenAIP data is not certified ..."), the warnings sheet repeats the notice, and the map names the provider beside the wind's. The pilot can switch the server's airspace off, which also removes what was kept.

The app also says when it is **not** checking, because its silence would otherwise read as "no airspace near" (`FlightSession::airspace_check`): before the first position with a height; when positions have stopped for ten seconds, in which case the warnings shown are from the last one; and when the pilot's files hold no airspace within about a hundred kilometres of the glider, which is how a German file shows over Austria. That last test is coarse: a file that covers a country's border region says nothing about how far beyond the border it is complete. The same airspace in two files is taken once. Files brought in or removed during a flight take effect at once, and what the pilot silenced stays silenced. One stored file that cannot be read does not cost the airspace of the others. A file's date is the day it was brought in, not the age of its data, which OpenAir does not carry.

## The record

One shape for every source, in `crates/domain/src/airspace.rs`:

| Field | Meaning |
|---|---|
| `id` | `source:local`, like a station. The local part is the source's own key where it has one (openAIP), and otherwise derived from the name and the two limits (OpenAir files), so it stays the same when a later edition moves a corner |
| `name` | As published. May be empty |
| `class` | A to G, or unclassified |
| `airspace_type` | Control zone, terminal area, restricted, danger, prohibited, transponder and radio zones, glider sector, drop zones, nature protection and the rest of what OpenAir and openAIP distinguish: 42 kinds, and `other` with the source's own word |
| `country` | ISO code, if the source says. The same type means different things in different countries |
| `lower`, `upper` | The floor and the ceiling: value, unit and reference as published (`FL 95`, `1000 ft AGL`, `GND`, `UNL`), and the same height in metres above the same reference |
| `lower_at_least` | A second floor, for "5000 ft AMSL but at least 1000 ft AGL" |
| `upper_at_most` | How far the ceiling may be raised, for "5000 ft AMSL, at most 8000 ft AMSL on request" |
| `geometry` | One outer ring and, rarely, holes. A circle's centre and radius are kept beside its ring, for drawing |
| `activity` | What the source says about use: by NOTAM, on request, hours by weekday, the period a temporary area exists |
| `frequencies`, `remarks` | As published |
| `validity` | AIRAC cycle if the source states one, the date of the file, and when the source last changed the record |

A limit is not converted to metres above sea level in the record, because that needs the air pressure or the terrain. `VerticalLimit::metres()` gives metres above the limit's own reference.

Checks, made when a value is built and again when it is read from JSON: no coordinate or height that is not a number; heights between 500 m below and 100 km above their reference; a flight level only against standard pressure; rings closed, with at least three distinct corners and an area; at most 50,000 points to a ring and 1,000 holes; no polygon wider than 90 degrees, which also catches rings that wrap around the antimeridian. `Airspace::validate()` adds what needs the whole record: the floor not above the ceiling where both have the same reference, no floor at "unlimited" and no ceiling at the ground, a country that is a code, frequencies that are frequencies. Limits with different references cannot be compared without terrain or pressure and are accepted.

Edges are straight lines in longitude and latitude. That is how the GeoJSON standard defines the line between two positions ([RFC 7946, section 3.1.1](https://www.rfc-editor.org/rfc/rfc7946#section-3.1.1)), and it is what a map draws. A great circle between the same two points differs by about 55 m in the middle of a 50 km east-west edge at 48° north *(computed, not measured)*; sources that mean a curve publish enough points for this not to matter.

## Reading OpenAir files

`venti_airspace::openair::parse(text, &options)` reads a file; `parse_bytes` does the same for bytes whose encoding is unknown (UTF-8, else Latin-1, which keeps umlauts).

The format follows Naviter's specification, [SeeYou OpenAir file format](https://github.com/naviter/seeyou_file_formats/blob/main/OpenAir_File_Format_Support.md), version 2.1.3 of 2026-11-04, read on 2026-11-19. openAIP's documentation names the same text as the specification of its own OpenAir exports. The original WinPilot page (`winpilot.com/UsersGuide/UserAirspace.asp`) could not be reached that day; what goes beyond Naviter's text follows what XCSoar's reader accepts. *To verify against the original:* the old class codes, `AT`, `SP`, `SB` and `V Z=`.

| Line | Read as |
|---|---|
| `AC` | Class. Begins a new airspace. `A` to `G` and `UNC` are classes; in the original format the line carries a type instead (`R`, `Q`, `P`, `GP`, `CTR`, `W`, `TMZ`, `RMZ` and others), which is read as that type with no class |
| `AY` | Type, from Naviter's list. An unknown word is kept as `other` |
| `AN` | Name |
| `AL`, `AH` | Floor and ceiling. `GND`, `SFC`, `UNL`, `UNLIM`, `UNLIMITED`, `FL95`, `FL 095`, `4500ft MSL`, `4500 FT AMSL`, `4500MSL`, `4500` (feet above sea level), `1000 ft AGL`, `2000FT AGL`, `1000ft GND`, `1500m AMSL`, `300m AGL`, `5000ft STD`, in upper or lower case |
| `AF`, `AG` | Frequency and the station on it |
| `AA` | Activation times. Kept as text and not evaluated; `AA NONE` marks the airspace as active by NOTAM |
| `AI` | An identifier some exports carried. Used as the record's key when present |
| `AX`, `AT`, `SP`, `SB`, `V Z=`, `V W=` | Read and not kept: a transponder code, a label position, drawing hints |
| `V X=` | The centre for the lines that follow |
| `V D=+`, `V D=-` | Arcs run clockwise (the default) or counter-clockwise from here on |
| `DP` | A corner, as `47:15:37 N 011:20:38 E`, `47:15.617N 011:20.633E` or decimal degrees |
| `DC` | A circle around the centre; radius in nautical miles |
| `DA` | An arc around the centre: radius, from bearing, to bearing |
| `DB` | An arc around the centre from one point to another |
| `DY` | An airway segment. **Not supported**: Naviter advises against it and no file read while writing this used it. An airspace that uses it is skipped and reported |
| `*` | A comment, to the end of the line. `*VERSION:` and `*WRITTEN_BY:` in the header are kept |

**Circles and arcs become polygons that are never smaller than the true shape.** A circle gets a polygon that touches it from outside. An arc that bulges outward gets pieces outside its circle; an arc that curves inward gets pieces through points on it, which run on the airspace side. Either way the polygon is at most 10 m from the curve (`ARC_MAX_ERROR_M`). A circle of 5 nautical miles becomes 68 corners. The pieces themselves are straight on the map and not great circles, which can take back up to 0.2 m of that on arcs of 60 nautical miles.

**The reader never drops an airspace without saying so.** `OpenAir::skipped()` is the number of airspaces left out, exact for any file. `OpenAir::problems` lists, with line numbers, three kinds of trouble:

| Kind | What happened | What the app must do |
|---|---|---|
| `AirspaceSkipped` | The shape could not be read: an unreadable corner, an arc without a centre, an unknown shape command, fewer than three corners, too many points. Half a shape is a wrong shape, so the airspace is left out | **Tell the pilot.** They will not be warned about that airspace |
| `Assumed` | Something could not be read and the cautious reading was taken. A limit that could not be read or was missing, or a floor above its ceiling: the floor became the ground or the ceiling "unlimited". An airspace that begins with its name, its class line missing: it is kept apart from the one before and has no class, which warns | Nothing is lost: the airspace is larger, or more restricted, than published |
| `LineIgnored` | A line the reader does not know, outside any shape | Nothing |

A hostile file cannot make the reader panic, loop or eat memory: one airspace may have 50,000 points and one file 2,000,000, radii end at 500 nautical miles, and the list of problems keeps the first 1,000 airspaces left out and the first 1,000 lesser problems, the rest counted in `problems_omitted`. The two have room of their own so that a file full of stray lines cannot push a left-out airspace off the list. The tests feed it the sample file cut off at every byte, with its lines shuffled 500 ways, with 3,000 random damages, with random bytes, and with a megabyte in one line.

What the reader cannot notice: a file cut off between two corners of its last airspace. That airspace is closed where it stands and has a wrong shape.

Checked against real files on 2026-11-19: openAIP's OpenAir exports for Germany and Austria, in both versions of the format, read without a single problem, 1,018 airspaces each. openAIP publishes the same airspace as GeoJSON, which the openAIP adapter reads with code of its own; the two readers agree on the name, class, type and both limits of every one of the 1,018. Those exports contain no circles and no arcs, so that part of the reader is tested on shapes built by hand from the definition, and on no real file yet. *To verify* on a file that has them, such as the DAeC's, once there is permission to use it.

## The questions

`AirspaceIndex::new(airspaces)` sorts a set of records into a grid of cells a tenth of a degree wide. The questions then look only at airspaces whose bounding box touches the cells they concern. Airspaces too large for the grid, such as flight information regions, are kept in a list of their own.

| Question | Answer |
|---|---|
| `index.at(position, &altitudes)` | Every airspace whose outline contains the position, each with the pilot's height in relation to it: below, inside or above, by how many metres, with the floor and ceiling placed on the pilot's height scale and a flag when the answer is uncertain |
| `index.nearby(position, &altitudes, radius_m, vertical_margin_m)` | The airspaces within the radius that the pilot is at a height to enter, each with the distance and direction to its outline. From inside, the distance is to the nearest way out |
| `index.in_box(&area, band)` | For a map: what touches a box, optionally only what can reach into a band of altitudes |
| `index.cross_section(&path)` | For a side view: the stretches of a path that run under, in or over each airspace, as distances from its start, with the published floor and ceiling |
| `horizontal(&airspace, position)` | The distance and direction to one airspace's outline |
| `vertical_status(&airspace, &altitudes)`, `resolve(&limit, &altitudes)` | The height relation to one airspace, and one limit on the pilot's height scale |

`Altitudes` is what the pilot knows about their height: the altitude above sea level, and optionally a pressure altitude and the ground elevation below.

Whether a point is inside an outline is decided in the plane of longitude and latitude, with a point on the boundary counted as inside. Distances are measured on the FAI sphere, like everywhere else in the project: the nearest point of an outline is found on a flat map laid out around the pilot, which is exact to half a percent within 50 km, and the distance to it is then taken on the sphere.

### Conservative rules

1. **A point on a boundary is inside.** So is a point on the edge of a hole.
2. **Circles and arcs are never smaller than published**, as described above.
3. **A limit above sea level is compared with the altitude passed in.** That is the satellite altitude or the cockpit's own best altitude. In law such a limit is a barometric altitude at the local pressure setting, which differs from the true altitude by about 4 % of the height above the measuring station for every 10 °C the air is warmer or colder than standard. Nothing corrects for this. XCTrack compares these limits with the barometric altitude at the pilot's QNH setting; Venti has no QNH. Among the height rules this is the one place where the doubt is not taken toward the pilot; the "near" margin of 500 m is what covers it. *To verify in the air.*
4. **A flight level with a pressure altitude is exact.** A flight level is a pressure altitude at 1013.25 hPa, so the distance to it is the difference between it and the pilot's pressure altitude, which is how the rule is written. The pressure altitude the flight computer already has (`LiveState::pressure_altitude_m`) is the right input.
5. **A flight level without a pressure altitude reaches down, or up, to the pilot.** It is placed by the standard atmosphere and then widened: sea-level pressure anywhere from 980 to 1040 hPa, and the air up to 7 % (about 20 °C) warmer or colder than standard. A floor is taken at the bottom of that range and a ceiling at the top. For FL 100, nominally 3048 m, that is 2573 m to 3497 m. A pilot at 2800 m without a barometer is reported inside, uncertain. On a day beyond those bounds the range is too narrow.
6. **A limit above ground with the ground elevation is exact.**
7. **A limit above ground without it reaches to the pilot.** The ground may lie anywhere from 500 m below sea level up to the pilot's own altitude, and so may the limit above it. A floor above ground never puts the pilot below the airspace and a ceiling above ground never puts them above it. The answer is "inside, uncertain".
8. **A floor at the ground never has the pilot under it**, even when a terrain model puts the ground above the satellite altitude.
9. **A second floor ("but at least") raises the floor only when it can be placed.** Otherwise the published floor holds and the answer is marked uncertain.
10. **A ceiling that may be raised counts as raised.**
11. **"Below" and "above" are said only when certain.** The floor is taken as low and the ceiling as high as they can be. What remains is "inside", with `uncertain` set when the pilot could in truth be clear.
12. **Every airspace counts as active.** Hours, NOTAM flags and the published period of a temporary area are carried in the record and not evaluated. An area whose period has ended keeps warning until its source drops it.
13. **An airspace the warning table does not know is warned about.**
14. **A warning rises at once and falls slowly.** See below.
15. **No data is not "no airspace".** The index answers for the airspaces it was given. Outside the area they cover it finds nothing, and cannot know that. A shell must tell the pilot which area its data covers.

## Warnings

`WarningEngine` is fed one `Sample` after another, about once a second, and answers each with the warnings that hold at that moment: at most one per airspace, the most urgent first. It reads no clock; every sample carries its own time, so a replayed flight gives exactly the warnings the pilot got.

| Level | Raised when |
|---|---|
| `Inside` | In the outline and between floor and ceiling, or possibly so |
| `Entering` | On the present path and climb, inside within 60 s |
| `VeryNear` | Within 400 m over the ground and 100 m in height |
| `Near` | Within 2000 m over the ground and 500 m in height |

**The margins are XCTrack's.** The manual its developers recommend describes them: "If the pilot is below the airspace zone (horizontal distance away from the border of the zone is 0 m), a red warning event will be triggered (Airspace is really close) once the vertical distance is 100 m. An orange warning will be triggered (Airspace is close) once the vertical distance is 500 m", with a "scaled distance" that weighs height against distance over the ground "with a ratio of 4:1" ([AIR³ XCTrack Pro manual, Preferences 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences2/), read 2026-11-19). The formula that blends the two into one distance is not given *(to verify)*. Venti warns when both are within their margin, which is never later than any blend.

**The prediction is ours.** XCTrack documents none for airspace. The look-ahead of 60 s is a first value, *to verify in the air*. The direction of travel is taken from the path over the last 20 s, not from the course of the last second, which sweeps the whole compass in a thermal. In straight flight that is the course; in a thermal it is the drift, which is what carries a circling pilot into airspace. The climb comes from the sample; pass the averaged climb. A predicted entry says how many seconds away it is and which way in: through the side, the floor or the ceiling. Floor and ceiling are taken where the pilot is now; a limit above ground is not followed along the terrain ahead.

**Warnings do not flicker.** A warning rises to a more urgent level at once. It falls back only when the pilot is beyond the level's margin by a further 50 m over the ground or 20 m in height, and has been for 10 s. A pilot who leaves an airspace by ten metres is still told "inside" for a moment.

**The pilot can silence a warning, in two ways the engine keeps apart.**

- `acknowledge(id, silence)`: "I have seen it." The warning stays in the answers, marked, and is quiet at its present level and below. If it becomes more urgent it speaks again: acknowledging "near" does not silence "inside".
- `ignore(id, silence)`: "This airspace does not apply to me now", for an area known to be inactive today or one the pilot has permission for. Quiet at every level. The engine cannot check the claim. It may be said before the first sample, at the briefing on the ground; the time then counts from the first sample.

Both take a time: `Silence::For { seconds }`, `Silence::Until { time_s }` or `Silence::RestOfDay { utc_offset_s }`, which ends at local midnight. When a silence runs out while the warning still holds, the warning is new again.

`Warning::is_new` is set on the answer in which a warning appeared, became more urgent or came out of a silence: the moment to sound. `Warning::silenced` says whether to.

### Which airspaces warn

The table is data: `PARAGLIDER_RULES` in `crates/airspace/src/policy.rs`, read from the top, first fit decides. It is written for a paraglider flying by visual rules in Germany and Austria with no aviation radio and no transponder, knowing nothing about what is active today. A change here is a change there.

| Policy | Meaning |
|---|---|
| **Avoid** | Entering needs a clearance or equipment a paraglider normally lacks, or is forbidden while active. All four levels |
| **Caution** | Not closed, but something there deserves attention. `Entering` and `Inside` only |
| **Free** | Nothing a paraglider needs permission for. Never warns |

| Airspace | Policy | Why | Source |
|---|---|---|---|
| Glider sector | Free | It opens airspace to gliders when activated and closes nothing. A paraglider may use an active sector only with an aviation radio. Whether it is active is unknown, so the airspace it lies in keeps warning | DHV, Segelflugsektoren: "Luftsportgeräte dürfen Segelflugsektoren mitbenutzen, wenn sie mit einem Sprechfunkgerät mit dem erforderlichen Kanalraster ausgerüstet sind" [1] |
| Temporary reserved airspace (TRA) in Austria | Free | Class C or D airspace that becomes class G while activated, for gliders, hang gliders and paragliders among others. It closes nothing; the control zone or terminal area it lies in keeps warning | AIP Austria ENR 5.5: "Temporary reserved airspaces are Class C or D airspaces of defined vertical and horizontal extent which are temporarily classified as Class G airspace during the period of the respective activation" [4] |
| Flight information region, FIS and ACC sector, VFR sector and route, transponder code and recommendation areas | Free | Boundaries of a service, not restrictions | *To verify* for each; none restricts entry by its definition |
| Control zone (CTR), also military | Avoid | Entry only with a clearance by radio | DHV: "D: (CTR) Kontrollzone. Einflug nur mit Freigabe über Funk > nix für uns" [3]; AIP Austria ENR 1.4: class D, VFR "Subject to an ATC clearance: YES" [5] |
| Restricted area (ED-R, LO-R), prohibited area, closed to gliders | Avoid | Restricted while active, and the app does not know when | DHV: "ED-R ... Aktivierung erfragen (Tel/Funk) ... Falls nicht möglich: Grundsätzlich weg bleiben" [2] |
| Danger area (ED-D, LO-D) | Avoid | Not closed in law, but the DHV's advice for Austria is to stay out unless known inactive. XCTrack draws these and does not warn; Venti warns | DHV, Austria: "LO-R / LO-D ... Aktivierung per Funk erfragen, falls nicht möglich: Grundsätzlich weg bleiben" [6] |
| Transponder mandatory zone (TMZ) | Avoid | No entry without a transponder; in Germany a listening watch on the radio as well | DHV: "TMZ ... Flugfunk- und Transponderpflicht. Daher: Grundsätzlich weg bleiben" [2]; AIP Austria ENR 2.2: "All flights ... shall carry and operate SSR transponders" [7] |
| Radio mandatory zone (RMZ) | Avoid | No entry without an aviation radio and a call before entering. A pilot with a radio can put a rule of their own first | DHV: "RMZ ... Flugfunk. Ansonsten - weg bleiben" [2]; AIP Austria ENR 2.2 [7] |
| Temporary reserved airspace elsewhere, temporary segregated area, temporary restriction, NOTAM area | Avoid | In Germany a TRA is a military reservation and entry is forbidden | DHV, Austria talk: "TRA Deutschland: temporäre militärische Luftraumreservierung ... Einflug verboten" [6] |
| Military training area, air defence identification zone | Avoid | Military use | *To verify* what applies to a paraglider under each |
| Aerodrome, military aerodrome and helicopter traffic zones | Avoid | Traffic of an aerodrome | *To verify* |
| Traffic information zone and area | Caution | | *To verify* |
| Aerial sporting area: drop zones, aerobatic boxes | Caution | Not closed, but dangerous while in use | DHV: "Drop-Zones, Kunstflugboxen: Sind kein gesperrter Luftraum ... Durchflug möglich - aber ggf. sehr ungesund ... wenn unbekannt oder aktiv: MEIDEN" [2] |
| Alert and warning areas, military routes, feeding routes | Caution | A hazard, not a closure | *To verify* |
| Overflight restriction, protected area | Caution | Nature protection with rules that differ per area: some forbid only take-off and landing, some all flying below a height. The record's remarks say which | openAIP's remarks on the Austrian national parks; the laws behind them *to verify* |
| Anything else in class A, B, C or D: terminal areas, control areas, plain airspace | Avoid | A clearance, a radio and a transponder are needed | DHV: "Wo wir nicht fliegen dürfen: C/D/TMZ/ED-R ... Luftraum C/D: IFR, VFR, Freigabe erforderlich; Flugfunk und Transponderverpflichtung; Daher: Grundsätzlich weg bleiben" [2]; AIP Austria ENR 1.4 [5] |
| Anything else in class E or G | Free | Open to flights by visual rules without a clearance | DHV: "Wo wir fliegen dürfen: G und E" [2]; AIP Austria ENR 1.4: class E and G, VFR "Subject to an ATC clearance: NO" [5] |
| Anything else in class F | Caution | Advisory airspace. Germany and Austria publish none; one Austrian terminal area carries it in openAIP's data | *To verify* |
| No class and no known type | Avoid | When in doubt, warn | |

Sources, all read on 2026-11-19:

1. DHV, [Segelflugsektoren](https://www.dhv.de/flugbetrieb/luftraum-luftrecht/segelflugsektoren/).
2. DHV, [Luftraum-Onlineseminar Basic](https://www.dhv.de/media/seiten/04_flugbetrieb/LuftraumLuftrecht/luftraumbasicseminar.pdf), 3 February 2026.
3. DHV, [Luftraum-Seminar XC](https://www.dhv.de/media/seiten/04_flugbetrieb/LuftraumLuftrecht/luftraumseminar_advanced.24.02.2022.pdf), February 2022.
4. Austro Control, AIP Austria, [ENR 5.5](https://eaip.austrocontrol.at/lo/261030/PART_2/LO_ENR_5_5_en.pdf), edition effective 30 October 2026.
5. Austro Control, AIP Austria, [ENR 1.4](https://eaip.austrocontrol.at/lo/261030/PART_2/LO_ENR_1_4_en.pdf), same edition.
6. DHV, [Luftraum-Seminar Österreich](https://www.dhv.de/media/seiten/04_flugbetrieb/LuftraumLuftrecht/luftraum_at_vortrag_schachner_02.2023.pdf), February 2023.
7. Austro Control, AIP Austria, [ENR 2.2](https://eaip.austrocontrol.at/lo/261030/PART_2/LO_ENR_2_2_en.pdf), same edition.

The DHV's seminars are teaching material, not law, and the German rules were not read in the AIP itself, which DFS sells. *To verify* against the AIP VFR Germany. Only rule texts of the Austrian AIP were read; none of Austro Control's airspace data was fetched or used.

Not in the table, because the app cannot know them: airspace marked HX, which is open at times and must be left within ten minutes of activation; and the relief an active glider sector or Austrian TRA gives to the airspace it lies in. A pilot who knows that a sector is open uses `ignore` on the airspace around it.

## Feeding the engine from the cockpit

The cockpit has everything a sample needs in `LiveState`, the flight computer's output: `position`, `altitude_m`, `pressure_altitude_m`, `ground_speed_ms`, `course_deg` and `average_climb_ms`. The ground elevation comes from the terrain model once there is one; until then pass `None`. Build the index once, when the airspace is loaded, and keep the engine for the flight.

```rust
use venti_airspace::openair::{self, Options};
use venti_airspace::{
    AirspaceIndex, Config, Level, PressureAltitude, Sample, Silence, WarningEngine,
};
use venti_domain::{GeoPoint, SourceId};

// Airspace from a file the pilot imported. Records from a data source go
// into the same index.
let text = "\
AC D
AY CTR
AN CTR INNSBRUCK
AL GND
AH 5000ft AMSL
V X=47:15:37 N 011:20:38 E
DC 5
";
let file = openair::parse(text, &Options::new(SourceId::new("my-file")?));
// An airspace the reader had to leave out will never warn: say so, and
// name them from `file.problems`.
assert_eq!(file.skipped(), 0);

let index = AirspaceIndex::new(file.airspaces);
let mut engine = WarningEngine::new(Config::default());

// Once a second, from the flight computer's state. Here: 300 m west of
// the zone at 1400 m, flying east at 10 m/s.
let sample = Sample {
    time_s: 1_795_000_000.0,
    position: GeoPoint::new(47.2603, 11.2172)?,
    altitude_m: 1400.0,                                      // LiveState::altitude_m
    pressure: Some(PressureAltitude::standard(1385.0)),      // LiveState::pressure_altitude_m
    ground_elevation_m: None,                                // from terrain, when there is any
    ground_speed_ms: Some(10.0),                             // LiveState::ground_speed_ms
    course_deg: Some(90.0),                                  // LiveState::course_deg
    climb_ms: Some(0.5),                                     // LiveState::average_climb_ms
};
let warnings = engine.update(&index, &sample)?;

// The most urgent comes first. Each names its airspace, whose record has
// the source and the age of the data.
let first = &warnings[0];
assert_eq!(first.airspace.name, "CTR INNSBRUCK");
assert_eq!(first.level, Level::Entering);
let entry = first.entry.expect("an entering warning says when");
assert!((entry.seconds - 30.0).abs() < 2.0);
assert!((first.horizontal.distance_m - 300.0).abs() < 20.0);

// Sound when a warning is new and the pilot has not silenced it.
if first.is_new && first.silenced.is_none() {
    // play the sound for first.level
}

// "Ignore for ten minutes."
engine.acknowledge(first.id(), Silence::For { seconds: 600.0 });
# Ok::<(), Box<dyn std::error::Error>>(())
```

A sample whose time or altitude is not a number is refused with an error. Speeds and climbs that are not numbers are treated as unknown. If time runs backwards, as in a rewound replay, the engine starts afresh.

What the shell has to do, because the engine cannot:

- Show the source and the age of the data with every warning (`airspace.id.source()`, `airspace.validity`), and say when the data is old. openAIP's exports are rebuilt daily; an AIRAC cycle is 28 days.
- Tell the pilot when a file left airspaces out, and which area the loaded data covers.
- Mark a warning whose `uncertain()` is set, and say what is missing: `vertical.missing()` is the ground elevation or the pressure, taken from the limit the doubt comes from. It is `None` when no measurement would settle it, as for a ceiling that may be raised on request.
- Keep per-frame drawing off the bridge: ask `in_box` when the map moves, not on every frame.

## Measured

On openAIP's exports for Germany and Austria of 2026-11-19, read as OpenAir files: 1,018 airspaces with 58,980 corners. Release build, one core of a cloud server (Intel Xeon, 2.1 GHz). Not measured on a phone: *to verify*. Reproduce with `VENTI_AIRSPACE_FILES=de.txt,at.txt cargo test -p venti-airspace --release --test speed -- --nocapture`.

| | Germany and Austria | A made-up country of 3,000 airspaces and 225,000 corners |
|---|---|---|
| Reading the files | 21 ms for 1.6 MB, no problems reported | |
| Building the index | 1.2 ms | 4.5 ms |
| Memory, estimated from the records | 2.1 MB | 6.2 MB |
| `at` a position | 14 µs | 100 µs |
| `nearby` within 2 km | 64 µs | 300 µs |
| `nearby` within 20 km | 76 µs | 350 µs |
| `cross_section` along a path of two legs across an area 200 km wide | 180 µs | 2.2 ms |
| One update of the warnings, mean | 7 µs | 470 µs |
| One update of the warnings, slowest | 41 µs | 1.4 ms |

The warning update is cheap on the real data because most of what surrounds a pilot there is class E or an information sector, which the table sets aside before any geometry is done. The made-up country has neither. Both are far below the one second between samples. The memory figure is an estimate summed from the sizes of the records and their corners (`AirspaceIndex::stats`), not a measurement of the process.

## Not built

- **Activations.** Which areas are active today, NOTAMs, and the hours in the records. See [data-sources.md](data-sources.md#airspace): there is no open feed. Every airspace counts as active.
- **Relief.** Glider sectors, Austrian TRAs and HX airspace do not open the airspace around them; the pilot does that by hand with `ignore`.
- **Terrain.** The ground elevation is passed in. Without it, limits above ground are uncertain.
- **QNH.** Limits above sea level are compared with the altitude passed in, not with a barometric altitude at the local setting.
- **Airways in OpenAir files** (`DY`).
- **Drawing.** The crate gives a map the airspaces in a box and a side view its stretches; nothing here draws.
- **Settings for the pilot.** Margins, look-ahead and the table are `Config`; nothing stores a pilot's choices.
- **Bundles.** The region bundle of [architecture.md](architecture.md#api-and-offline-sync) does not exist. The server's airspace endpoint stands in for it; no app uses it yet.
- **Coverage by country.** The server's answer says which box its set is whole in and which countries it holds, but not where inside the box those countries end. An OpenAir file says nothing of the kind at all.
