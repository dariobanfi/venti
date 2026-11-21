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
 *   and keeps the logbook and the pilot's own sites in step with it, also
 *   from the worker.
 * - `tokens-source.ts` talks to it about the programs a pilot gave access.
 * - `alerts-source.ts` talks to it about the pilot's alarms and what they fired.
 * - `airspace-source.ts` fetches the airspace around a place from the Venti
 *   server when the core says it is time, for the core to keep.
 * - `community-source.ts` asks it for the flights other pilots share with
 *   everyone, and fetches one of them to show.
 * - `groups-source.ts` talks to it about the pilot's groups and their plans
 *   for a flying day, from the pilot's account.
 * - `thermal-map-source.ts` asks it for the kind of day, square by square.
 * - `radar-source.ts` asks it for the rain radar's frames and their pictures.
 * - `terrain-source.ts` fetches the map's elevation tiles under a flight,
 *   from the map's tile server, for the core to read the ground from.
 * - `glide-reach-source.ts` fetches them around a place, for what a glide
 *   reaches from there.
 */

export type {
	AccountView,
	AlertDayKind,
	AlertDraft,
	AlertFaultView,
	AlertKind,
	AlertView,
	FiredLine,
	AirspaceCover,
	AirspaceCrossing,
	AirspaceFile,
	AirspaceHeightFilter,
	AirspaceHeightStep,
	AirspaceImport,
	AirspaceOverhead,
	AirspaceOverPlace,
	AirspacePolicy,
	AirspaceShape,
	BestFlight,
	Bounds,
	CandidateOffer,
	FlownMonth,
	PublicFlightTerms,
	SiteBestFlightLine,
	SiteCandidates,
	SiteCandidateView,
	SiteFlights,
	SiteFlightsWords,
	Coordinate,
	CoreErrorKind,
	Credit,
	EntryScore,
	FlightAirspace,
	FlightComparison,
	ComparisonRow,
	ComparisonLead,
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
	CableLine,
	CablesMap,
	ClosingCircleView,
	GlideReach,
	GlideReachSetting,
	GlideSummary,
	GroundUnderFlight,
	GroupDetail,
	GroupJoined,
	GroupMade,
	GroupMember,
	GroupView,
	ImportedFlight,
	KeptAirspace,
	KeptSite,
	KeptSites,
	TodayOrder,
	TodayStanding,
	TodayTakeoff,
	TodayTakeoffs,
	LeagueHandover,
	LiveFollowView,
	LiveGlider,
	LiveMap,
	LiveOthersNetwork,
	LogbookEntry,
	LogbookStatistics,
	SiteStatistics,
	Tendencies,
	Tendency,
	TendencyKind,
	LogbookView,
	LogbookYear,
	ModelForecast,
	OtherAircraftView,
	MonthStatistics,
	RouteStatistics,
	YearStatistics,
	PilotPage,
	PlanBoard,
	PlanDay,
	PlanDraft,
	PlanView,
	ProfilePoint,
	PublicFlight,
	PublicFlights,
	PublicFlightsOrder,
	RadarFrameView,
	RadarKey,
	RadarLayerView,
	RadarMoment,
	RadarView,
	RankedPilot,
	Ranking,
	RankingSeason,
	Registration,
	ReportForm,
	ReportReason,
	ReportReasonChoice,
	ReportSubject,
	PlannedScore,
	RouteKind,
	RouteLegView,
	RoutePlan,
	ReachEnd,
	ReachRay,
	RouteAlong,
	RouteLegWeather,
	RoutePointWeather,
	RouteSetting,
	SiteKind,
	SiteAtPlace,
	SiteOutlook,
	OutlookDay,
	OutlookHour,
	OutlookNearby,
	OutlookLayer,
	Flyability,
	Thermals,
	ThermalLegendEntry,
	ThermalMap,
	ThermalMapCell,
	ThermalMapDay,
	ThermalHoursLayer,
	ThermalMapHourMark,
	ThermalMapHourSquare,
	ThermalMapHoursView,
	ThermalMapKey,
	ThermalMapLayerChoice,
	ThermalMapLevel,
	ThermalMapScale,
	ThermalMapShapes,
	ThermalStripDay,
	SitesFilter,
	SitesFilterChoices,
	SitesFiltered,
	SitesShown,
	SitesWindIs,
	SitesMap,
	SiteAdding,
	SiteAuthor,
	SiteByPilots,
	SiteConfirmations,
	SiteConfirmationLine,
	SiteConfirmDay,
	SiteConfirmFlight,
	SiteConfirmForm,
	SiteCorrecting,
	SiteDraftFault,
	SiteDraftRebased,
	SiteDraftView,
	SiteField,
	SiteForm,
	SiteFormField,
	SiteHistory,
	SiteHistoryOffers,
	SiteInTheWay,
	SiteOffer,
	SiteOffers,
	SitePage,
	SiteRemovedLine,
	SiteRevisionView,
	SiteWords,
	SiteView,
	SiteWritten,
	StationAir,
	StationCloudBase,
	StationDay,
	StationDays,
	StationHistory,
	TemperatureStep,
	SyncSummary,
	ThermalSummary,
	TokenCreated,
	TokenForm,
	TokenScope,
	TokenView,
	TokenWithdrawal,
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
export type { AccessWanted, ProgramView } from './tokens-source';
export type { AlertsHeld, AlertsSynced } from './alerts-source';
export { CoreClient } from './client';
export type { TodayAnswer } from './today-source';
export type { RouteWeatherAnswer, RouteWeatherAsked } from './route-weather-source';
export type { SiteDraftChecked, SiteSent } from './site-edit-source';
export type { Box } from './takeoffs-source';
export type {
	Compared,
	FlightRef,
	OpenedFlight,
	PublicFlightsAsked,
	SeasonRanking
} from './community-source';
export type { LiveAnswer, LiveFollowAnswer, LiveOthersAnswer } from './live-source';
export type { WindAnswer } from './wind-source';
export type { HoursAsked } from './thermal-map-source';
export type { RadarColours } from './radar-source';

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
