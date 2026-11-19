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
 */

export type {
	BestFlight,
	Bounds,
	Coordinate,
	CoreErrorKind,
	EntryScore,
	FlightAnalysis,
	FlightFile,
	FlightMoment,
	FlightScore,
	FlightView,
	GliderStatistics,
	GlideSummary,
	ImportedFlight,
	LogbookEntry,
	LogbookStatistics,
	LogbookView,
	LogbookYear,
	MonthStatistics,
	ProfilePoint,
	RouteKind,
	ThermalSummary,
	Totals,
	TrackPoint,
	WindView
} from './pkg/venti_core_wasm.js';
export { CoreClient } from './client';

/** The kinds of error the core itself raises. Their messages are fit to show as they are. */
const CORE_ERRORS = ['InvalidFlight', 'UnknownFlight', 'Storage'];

/** Whether an error came from the core, rather than from the browser around it. */
export function isCoreError(error: unknown): boolean {
	return error instanceof Error && CORE_ERRORS.includes(error.name);
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
