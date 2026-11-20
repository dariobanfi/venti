/**
 * The web app's way into the Rust core. Screens import from here and from
 * nowhere else in this folder.
 *
 * The types are generated from the core's Rust definitions by
 * `scripts/build-core.sh`; none of them is written by hand.
 *
 * How the parts fit together:
 *
 * - `client.ts` is what the page calls. It runs on the page.
 * - `worker.ts` runs the core in a worker and answers the client's calls,
 *   as `protocol.ts` lists them.
 * - `session.ts` joins the core, whose calls do not wait, to the browser's
 *   storage, which always makes a page wait.
 * - `idb-store.ts` keeps the core's files in IndexedDB; `store.ts` says what
 *   a store is and has one that lives in memory.
 * - `wind-source.ts` asks the Venti server for the wind, from the worker.
 * - `sites-source.ts` asks it for the flying sites of a part of the map.
 * - `forecast-source.ts` asks it what its weather models say about a place.
 * - `live-source.ts` asks it who is in the air, and follows one flight.
 * - `account-source.ts` talks to the Venti server about the pilot's account
 *   and keeps the logbook in step with it, also from the worker.
 * - `airspace-source.ts` fetches the airspace around a place from the Venti
 *   server when the core says it is time, for the core to keep.
 */

export type {
	AccountView,
	AirspaceCover,
	AirspaceCrossing,
	AirspaceFile,
	AirspaceImport,
	AirspacePolicy,
	AirspaceShape,
	BestFlight,
	Bounds,
	Coordinate,
	CoreErrorKind,
	Credit,
	EntryScore,
	FlightAirspace,
	FlightAnalysis,
	FlightFile,
	FlightMoment,
	FlightScore,
	FlightSharing,
	FlightView,
	FlightVisibility,
	ForecastDay,
	ForecastHourView,
	ForecastLevel,
	ForecastView,
	ForecastWind,
	GliderStatistics,
	GlideSummary,
	ImportedFlight,
	KeptAirspace,
	LiveFollowView,
	LiveGlider,
	LiveMap,
	LogbookEntry,
	LogbookStatistics,
	LogbookView,
	LogbookYear,
	ModelForecast,
	MonthStatistics,
	ProfilePoint,
	Registration,
	RouteKind,
	SiteKind,
	SitesMap,
	SiteView,
	StationHistory,
	SyncSummary,
	ThermalSummary,
	Totals,
	TrackPoint,
	TrailPointView,
	WindBand,
	WindMap,
	WindMark,
	WindReading,
	WindSpan,
	WindStation,
	WindSymbol,
	WindView
} from './pkg/venti_core_wasm.js';
import type { CoreErrorKind } from './pkg/venti_core_wasm.js';
import { UNREACHABLE } from './protocol';

export type { NewAccount, SyncOutcome } from './account-source';
export { CoreClient } from './client';
export type { LiveAnswer, LiveFollowAnswer } from './live-source';
export type { WindAnswer } from './wind-source';

/** The kinds of error the core itself raises. Their messages are fit to show as they are. */
const CORE_ERRORS: CoreErrorKind[] = [
	'InvalidFlight',
	'UnknownFlight',
	'Storage',
	'Server',
	'InvalidFile'
];

/** Whether an error came from the core, rather than from the browser around it. */
export function isCoreError(error: unknown): boolean {
	return error instanceof Error && (CORE_ERRORS as string[]).includes(error.name);
}

/**
 * Whether an error's message was written for the pilot and can be shown as
 * it is: the core's own errors, and a server out of reach.
 */
export function isInWords(error: unknown): boolean {
	return isCoreError(error) || (error instanceof Error && error.name === UNREACHABLE);
}

/**
 * Asks the browser to keep the site's data when the device runs short of
 * space, instead of clearing it as it may clear any site's. Browsers grant
 * this by their own rules, some after asking the pilot; nothing depends on
 * the answer.
 */
export function askToKeepStorage(): void {
	void navigator.storage?.persist?.().catch(() => {});
}
