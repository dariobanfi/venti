/**
 * What the page and the core say to each other.
 *
 * The core runs in a worker, so that reading a long flight never holds up
 * the page: an eight-hour flight takes about a second to import and another
 * to analyse. The page sends a call and gets an answer; this file is the
 * list of calls, and the one place that turns a call into the core doing
 * something.
 */

import type {
	AccountSource,
	GoogleEntered,
	NewAccount,
	NewGoogleAccount,
	SyncOutcome
} from './account-source';
import type { AlertsHeld, AlertsSource, AlertsSynced } from './alerts-source';
import type { PushSubscribed, SocialSource } from './social-source';
import type { AirspaceSource } from './airspace-source';
import type {
	CommunitySource,
	Compared,
	FlightRef,
	OpenedFlight,
	PublicFlightsAsked,
	SeasonRanking
} from './community-source';
import { flightFileText } from './flight-text';
import type {
	AccountView,
	AgreementOutcome,
	FoehnView,
	GoogleWords,
	AlertDraft,
	AlertFormWords,
	AlertFaultView,
	AlertView,
	AirspaceFile,
	AirspaceHeightFilter,
	AirspaceImport,
	AirspaceOverPlace,
	AirspaceShape,
	Bounds,
	CablesMap,
	MapLayersWords,
	CandidateOffer,
	GlideReach,
	GlideReachSetting,
	Coordinate,
	SiteCandidates,
	SiteFlights,
	SiteFlightsWords,
	Credit,
	FlightAirspace,
	FlightAnalysis,
	FlightFile,
	FlightSharing,
	FlightView,
	FlightVisibility,
	ForecastView,
	GroundUnderFlight,
	GroupDetail,
	GroupJoined,
	GroupMade,
	ImportedFlight,
	KeptAirspace,
	KeptSites,
	LeagueHandover,
	LogbookStatistics,
	SiteStatistics,
	Tendencies,
	LogbookView,
	PilotPage,
	PilotsFound,
	PilotStatistics,
	PlanBoard,
	PlanDraft,
	PlanView,
	PublicFlights,
	RadarView,
	Registration,
	RoutePlan,
	ReportForm,
	ReportReason,
	ReportSubject,
	SiteAdding,
	SiteAtPlace,
	SiteConfirmations,
	SiteConfirmFlight,
	SiteConfirmForm,
	SiteCorrecting,
	SiteDraftRebased,
	SiteDraftView,
	SiteForm,
	SiteHistory,
	SiteHistoryOffers,
	SiteOffers,
	SiteOutlook,
	SitePage,
	SiteRemovedLine,
	SiteWords,
	SiteWritten,
	ThermalMap,
	SitesFilter,
	SitesFilterChoices,
	SitesFiltered,
	SitesMap,
	SiteView,
	StationAir,
	StationDay,
	ThermalMapHourSquare,
	ThermalMapHoursView,
	ThermalMapShapes,
	ThermalStripDay,
	StationDays,
	StationHistory,
	SyncSummary,
	TokenCreated,
	TokenForm,
	TodayOrder,
	TodayStanding,
	WindStation,
	CheerList,
	CheerState,
	CommonWords,
	PagesWords,
	CommentDraft,
	CommentList,
	CommentPosted,
	CommentsSwitch,
	Feed,
	FollowList,
	FollowState,
	Notifications,
	PilotsAlarmNumbers,
	PilotsWords,
	PlannerWords,
	RouteFromFile,
	SiteSheetWords,
	SocialWords,
	StationSheetWords,
	TodayWords
} from './pkg/venti_core_wasm.js';
import {
	commonWords,
	pagesWords,
	pilotsAlarmNumbers,
	pilotsWords,
	plannerWords,
	routeFromGpx,
	routeToGpx,
	siteSheetWords,
	stationSheetWords,
	todayWords
} from './pkg/venti_core_wasm.js';
import type { ForecastSource } from './forecast-source';
import type { GroupsSource } from './groups-source';
import type { LiveAnswer, LiveFollowAnswer, LiveSource } from './live-source';
import type { CoreSession } from './session';
import type { SiteDraftChecked, SiteEditSource, SiteSent } from './site-edit-source';
import type { SitesSource } from './sites-source';
import type { Box, TakeoffsSource } from './takeoffs-source';
import type { RadarColours, RadarSource } from './radar-source';
import type { HoursAsked, ThermalMapSource } from './thermal-map-source';
import type { AccessWanted, ProgramView, TokensSource } from './tokens-source';
import type { AgreementRead, AgreementSaid, AgreementSource } from './agreement-source';
import type { FoehnSource } from './foehn-source';
import { routePlan } from './route-plan';
import type {
	RouteWeatherAnswer,
	RouteWeatherAsked,
	RouteWeatherSource
} from './route-weather-source';
import { type TodayAnswer, todayOrders, type TodaySource } from './today-source';
import type { GroundAsked, TerrainSource } from './terrain-source';
import type { CablesSource } from './cables-source';
import type { GlideReachSource } from './glide-reach-source';
import type { SlopesAnswer, SlopesAsked, SlopesSource } from './slopes-source';
import type { AirSource, ClimbsAnswer, FlownAnswer, SiteAirAsked } from './air-source';
import type { WindAnswer, WindSource } from './wind-source';

/** What the page learns when the core has started. */
export interface Started {
	/** Whether the logbook is still there after the page is closed. */
	durable: boolean;
	/** The version of the core, for bug reports. */
	version: string;
}

/** Every call, with what it is given and what it answers. */
export interface Calls {
	start: { given: undefined; answer: Started };
	logbook: { given: undefined; answer: LogbookView };
	statistics: { given: undefined; answer: LogbookStatistics };
	/** The places flights began at, the one flown from most often first. */
	siteStatistics: { given: undefined; answer: SiteStatistics[] };
	/** What the pilot's own tracks say about how they fly. */
	tendencies: { given: undefined; answer: Tendencies };
	/** A flight file's bytes, as the pilot picked or dropped it. */
	importFlight: { given: ArrayBuffer; answer: ImportedFlight };
	/** The following are given a flight's identifier. */
	flight: { given: string; answer: FlightView };
	flightAnalysis: { given: string; answer: FlightAnalysis };
	flightFile: { given: string; answer: FlightFile };
	/** What the pilot is told before the flight goes to a league, with its file. */
	leagueHandover: { given: { id: string }; answer: LeagueHandover };
	deleteFlight: { given: string; answer: undefined };
	/**
	 * The ground under a flight's track, read from the map's elevation
	 * tiles, which are fetched for it. Empty where there is none to be
	 * had, and when the page has asked about another flight by now.
	 */
	flightGround: { given: GroundAsked; answer: GroundUnderFlight[] };
	/** Given the number of a question about the ground: lets go of it. */
	forgetFlightGround: { given: number; answer: undefined };
	/** The wind stations in a box of the map. Nothing if this site has no server. */
	wind: { given: Bounds; answer: WindAnswer | undefined };
	/** Given a station's identifier: what it measured over the last 24 hours. */
	stationHistory: { given: string; answer: StationHistory };
	/**
	 * What a station's readings say about the air, beside the other
	 * stations on the wind map.
	 */
	stationAir: { given: { station: WindStation; stations: WindStation[] }; answer: StationAir };
	/** Given the reader's offset from UTC in seconds: the days a station can be asked about. */
	stationDays: { given: number; answer: StationDays };
	/** One day of a station's readings, by its date as `stationDays` names dates. */
	stationDay: {
		given: { stationId: string; date: string; utcOffsetS: number };
		answer: StationDay;
	};
	/** The flying sites in a box of the map. Nothing if this site has no server. */
	sites: { given: { box: Bounds; filter: SitesFilter }; answer: SitesMap | undefined };
	/** The form for a site a pilot adds or corrects, in the core's words. */
	siteForm: { given: undefined; answer: SiteForm };
	/** An empty draft of a site at a place. */
	siteDraftNew: { given: Coordinate; answer: SiteDraftView };
	/** The draft that corrects a site of Venti's own layer; refused for another source's. */
	siteDraftOf: { given: SiteView; answer: SiteDraftView };
	siteDraftMoved: { given: { draft: SiteDraftView; place: Coordinate }; answer: SiteDraftView };
	/** What is wrong with a draft, field by field, and its wind in words. */
	siteDraftCheck: { given: { draft: SiteDraftView; comment: string }; answer: SiteDraftChecked };
	/**
	 * Adds a site for everyone, from the pilot's account. This and the
	 * three calls after it are sent with the session; a refusal of these
	 * two is an answer, with the site that stands in the way if one does.
	 */
	addSite: { given: { draft: SiteDraftView; comment: string }; answer: SiteSent };
	changeSite: { given: { draft: SiteDraftView; comment: string }; answer: SiteSent };
	/** Removes a site from the map. The answer is what became of it, in a line. */
	removeSite: { given: { id: string; comment: string }; answer: string };
	/** Puts a site back to what one of its revisions said. */
	restoreSite: { given: { id: string; revision: number; comment: string }; answer: SiteWritten };
	/** The words of a site's page, the core's. */
	siteWords: { given: undefined; answer: SiteWords };
	/**
	 * What may be offered for a site to whoever is signed in here: the
	 * core's one answer, by the account as the server described it last
	 * and by whether the server takes sites from pilots.
	 */
	siteOffers: {
		given: { site: SiteView; confirmations: SiteConfirmations | undefined };
		answer: SiteOffers;
	};
	/** Whether to offer adding a site, into which tier, and what the entry and its form say. */
	siteAdding: { given: undefined; answer: SiteAdding };
	/** Given the pilot's today (`YYYY-MM-DD`): the form of "I took off here and it was good". */
	siteConfirmForm: { given: string; answer: SiteConfirmForm };
	/** The pilot's own flights of a day that began at a site. */
	siteConfirmFlights: {
		given: { site: SiteView; day: string; utcOffsetS: number };
		answer: SiteConfirmFlight[];
	};
	/** Says of a site "I took off here and it was good". Fails in the core's words. */
	confirmSite: {
		given: { id: string; day: string; today: string; note: string; flight: string | undefined };
		answer: SiteWritten;
	};
	/** Given a site's identifier: takes the pilot's confirmation back. The answer is a line. */
	withdrawSiteConfirmation: { given: string; answer: string };
	/** Given a site: who says they took off there, read for its page. */
	siteConfirmations: { given: SiteView; answer: SiteConfirmations };
	/** What the form says when it corrects a site: the words of the site's tier. */
	siteCorrecting: { given: SiteView; answer: SiteCorrecting };
	/**
	 * What may be offered in a site's history to whoever is signed in
	 * here, for the history as it was read.
	 */
	siteHistoryOffers: { given: SiteHistory; answer: SiteHistoryOffers };
	/**
	 * The removed sites of a tier, the crowdsourced one when `true`: the
	 * one removed last first, each in the core's line.
	 */
	sitesRemoved: { given: boolean; answer: SiteRemovedLine[] };
	/** A draft carried onto the site as it stands now, after someone else changed it. */
	siteDraftRebased: {
		given: { base: SiteView; edited: SiteDraftView; now: SiteView };
		answer: SiteDraftRebased;
	};
	/** Given a site's identifier: the site as it stands now. */
	sitePage: { given: string; answer: SitePage };
	/** Given the identifier of a site that pilots made: all of its history. */
	siteHistory: { given: string; answer: SiteHistory };
	/** The words around what public flights say about a site, the core's. */
	siteFlightsWords: { given: undefined; answer: SiteFlightsWords };
	/** Given a site's identifier: what the public flights that began there say about it. */
	siteFlights: { given: string; answer: SiteFlights };
	/**
	 * The places in a part of the map where pilots take off and no site
	 * stands. A part too large is refused in the core's words.
	 */
	siteCandidates: { given: Box; answer: SiteCandidates };
	/** The candidates of an answer this pilot took off at, each with the core's offer. */
	candidateOffers: {
		given: { candidates: SiteCandidates; utcOffsetS: number };
		answer: CandidateOffer[];
	};
	/** Given a candidate's place: "Not this one", kept in this browser. */
	setCandidateAside: { given: Coordinate; answer: undefined };
	/** The choices the filter of sites offers, with the core's words for them. */
	sitesFilterChoices: { given: undefined; answer: SitesFilterChoices };
	/**
	 * What a filter leaves of sites the page holds: those that fit, the
	 * take-offs whose source does not say, and the core's words for both.
	 */
	filterSites: { given: { sites: SiteView[]; filter: SitesFilter }; answer: SitesFiltered };
	/** What the server's weather models say about a place. Nothing if this site has no server. */
	forecast: { given: Coordinate; answer: ForecastView | undefined };
	/**
	 * The gliders in a box of the map whose pilots show their flight to
	 * everyone. Nothing if this site has no server.
	 */
	live: { given: Bounds; answer: LiveAnswer | undefined };
	/**
	 * Given a live track's identifier: asks the server once about that
	 * flight, for all of it the first time and then for what is new.
	 * Nothing if this site has no server, or the page follows another
	 * flight by now.
	 */
	followLive: { given: string; answer: LiveFollowAnswer | undefined };
	/** Lets go of the flight being followed. */
	unfollowLive: { given: undefined; answer: undefined };
	/** How old positions are, in the core's words, one answer for each. */
	liveAges: { given: { ageS: number; landed: boolean }[]; answer: string[] };
	/**
	 * One page of the flights the server's pilots share with everyone.
	 * Nothing if this site has no server.
	 */
	publicFlights: { given: PublicFlightsAsked; answer: PublicFlights | undefined };
	/** A pilot, by handle, and one page of that pilot's public flights. */
	pilotPage: { given: { handle: string; after?: string }; answer: PilotPage | undefined };
	/**
	 * Given the name of a season, or nothing for the newest, and a place,
	 * or nothing for everywhere: the server's pilots ranked by their best
	 * public flights. Nothing if this site has no server.
	 */
	ranking: {
		given: { season?: string; near?: Coordinate; month?: string };
		answer: SeasonRanking | undefined;
	};
	/**
	 * The pilots whose handle or name holds a text. Refused in the core's
	 * words for fewer than two letters. Nothing if this site has no server.
	 */
	pilotsFound: { given: string; answer: PilotsFound | undefined };
	/** What a pilot's public flights add up to, by handle. Nothing if this site has no server. */
	pilotStatistics: { given: string; answer: PilotStatistics | undefined };
	/**
	 * The flying site at a place: the one of that identifier, or the
	 * nearest take-off. Nothing if this site has no server.
	 */
	siteAt: { given: { place: Coordinate; id?: string }; answer: SiteAtPlace | undefined };
	/**
	 * Given a flight's address on the server: another pilot's flight,
	 * fetched and prepared for display. Nothing of it is kept.
	 */
	openPublicFlight: { given: string; answer: OpenedFlight | undefined };
	/**
	 * Lays one flight beside another, the first being the one on screen:
	 * the other flight, and how the two lie to each other. Nothing is kept.
	 */
	compareFlights: { given: { first: FlightRef; other: FlightRef }; answer: Compared };
	/**
	 * The take-offs Today lists for a place, as the core chooses them: the
	 * pilot's own, then those around. Nothing if this site has no server.
	 */
	todayTakeoffs: { given: Coordinate; answer: TodayAnswer | undefined };
	/**
	 * The core's order of Today's take-offs on each of several days: given
	 * how each take-off stands on each day, one list for a day.
	 */
	todayOrders: { given: TodayStanding[][]; answer: TodayOrder[] };
	/**
	 * A route drawn on the map, given its places in order: each leg, the
	 * whole, and what it would score flown as drawn, in the core's words.
	 */
	planRoute: { given: Coordinate[]; answer: RoutePlan };
	/**
	 * A route as the pilot means to fly it, with the weather along it on a
	 * day of the thermal map: when the pilot is at each place, what the
	 * forecast has there then, each leg's wind and the heights to leave at.
	 */
	routeWeather: { given: RouteWeatherAsked; answer: RouteWeatherAnswer };
	/**
	 * What a glide reaches from a place on the map, over the ground there:
	 * the area as a ring, each direction's glide, and the core's words.
	 */
	glideReach: { given: GlideReachSetting; answer: GlideReach };
	/** Lets go of the ground kept for the glide's reach: the area was cleared. */
	forgetGlideReach: { given: undefined; answer: undefined };
	/**
	 * The cables around a place (power lines and ropeways), as the core
	 * makes them a map layer. Nothing if this site has no server.
	 */
	cables: { given: Coordinate; answer: CablesMap | undefined };
	/**
	 * How steep the ground is over a box of the map, cell by cell, in the
	 * core's classes. Nothing for a question asked since.
	 */
	slopes: { given: SlopesAsked; answer: SlopesAnswer | undefined };
	/** Where pilots climb right now in a part of the map, as the core words each climb. */
	liveClimbs: { given: Bounds; answer: ClimbsAnswer | undefined };
	/** Where thermals have been in a part of the map, in a month of the year. */
	flownThermals: { given: Bounds; answer: FlownAnswer | undefined };
	/** What the layers' sheet of the map says, in the core's words. */
	mapLayersWords: { given: undefined; answer: MapLayersWords };
	/** The core's line of who flies at a site now; empty when nobody does. */
	siteAir: { given: SiteAirAsked; answer: string };
	/** Lets go of the ground kept for the map of slopes: the layer was switched off. */
	forgetSlopes: { given: undefined; answer: undefined };
	/**
	 * Marks a site as one of the pilot's own. Given the site as the core
	 * gave it, the credits of the answer it came from, and the date of the
	 * source's data by that answer.
	 */
	keepSite: {
		given: { site: SiteView; credits: Credit[]; asOfUnixS: number | undefined };
		answer: undefined;
	};
	/** Given a site's identifier: it is no longer one of the pilot's own. */
	forgetSite: { given: string; answer: undefined };
	/** Given a site's identifier: whether it is one of the pilot's own. */
	isSiteKept: { given: string; answer: boolean };
	/**
	 * The pilot's own sites, from the browser alone: given a place, the
	 * nearest first with their distances; given none, by name.
	 */
	keptSites: { given: Coordinate | undefined; answer: KeptSites };
	/** What the forecast says about flying from one of them, from a moment on. */
	todayOutlook: { given: { id: string; nowUnixS: number }; answer: SiteOutlook };
	/** What the forecast says about flying from a site, by Venti's clock now. */
	siteOutlook: {
		given: Pick<SiteView, 'position' | 'wind' | 'elevationM'>;
		answer: SiteOutlook | undefined;
	};
	/** The kind of day, square by square, from today on. Nothing if this site has no server. */
	thermalMap: { given: undefined; answer: ThermalMap | undefined };
	/**
	 * One layer of one day of the thermal map hour by hour, as a card shows
	 * it. The worker keeps what it read for the calls below, while the map
	 * of the days is the same.
	 */
	thermalHours: { given: HoursAsked; answer: ThermalMapHoursView };
	/** What the map draws of one hour of hours read before: two GeoJSON texts. */
	thermalHourShapes: { given: { asked: HoursAsked; hour: number }; answer: ThermalMapShapes };
	/** A square in one hour of hours read before, in words. */
	thermalHourSquare: {
		given: { asked: HoursAsked; cell: number; hour: number };
		answer: ThermalMapHourSquare | undefined;
	};
	/** The square a place lies in, in hours read before. */
	thermalCellAt: {
		given: { asked: HoursAsked; lat: number; lon: number };
		answer: number | undefined;
	};
	/** Every day of the map at the square of a place, side by side. */
	thermalStrip: { given: Coordinate; answer: ThermalStripDay[] };
	/**
	 * The rain radar's layers and their frames, as the server has them now.
	 * Nothing if this site has no server.
	 */
	radar: { given: undefined; answer: RadarView | undefined };
	/** The list last fetched, read again at this moment; nothing before one was fetched. */
	radarAgain: { given: undefined; answer: RadarView | undefined };
	/**
	 * A frame's picture, by the frame's path, in the app's colours: a PNG,
	 * clear wherever one of the frames `above` it, by their paths, sees the place.
	 */
	radarPicture: {
		given: { path: string; above: string[]; colours: RadarColours };
		answer: Uint8Array;
	};
	/** Whether this site has a Venti server behind it. Yes, if that cannot be found out. */
	hasServer: { given: undefined; answer: boolean };
	/** Who may create an account on this site's server, if the server says. */
	registration: { given: undefined; answer: Registration | undefined };
	/** Whether this site's server signs pilots in with Google. No, if that cannot be found out. */
	googleSignIn: { given: undefined; answer: boolean };
	/** The words of signing in with Google. */
	googleWords: { given: undefined; answer: GoogleWords };
	/** Given an ID token from Google: signed in, or on to the handle step. */
	signInWithGoogle: { given: string; answer: GoogleEntered };
	/** The handle step after a first sign-in with Google: creates the account. */
	createGoogleAccount: { given: NewGoogleAccount; answer: AccountView };
	/** Given an ID token from Google: links the account to that Google account. */
	linkGoogle: { given: string; answer: AccountView | undefined };
	unlinkGoogle: { given: undefined; answer: AccountView | undefined };
	/** Given a fresh ID token from Google: deletes an account that signs in with it. */
	deleteAccountWithGoogle: { given: string; answer: undefined };
	/** The account this browser is signed in to, if any. */
	account: { given: undefined; answer: AccountView | undefined };
	/** The same after asking the server what it says of the account now. */
	refreshAccount: { given: undefined; answer: AccountView | undefined };
	signIn: { given: { handle: string; password: string }; answer: AccountView };
	createAccount: { given: NewAccount; answer: AccountView };
	signOut: { given: undefined; answer: undefined };
	/** Given the password: deletes the account on the server. */
	deleteAccount: { given: string; answer: undefined };
	/** Brings the logbook and the server in step, and then the pilot's own sites. */
	sync: { given: undefined; answer: SyncOutcome };
	/**
	 * Brings the pilot's own sites and their account in step. What the run
	 * did, or nothing: not signed in, out of reach, or stopped.
	 */
	syncKeptSites: { given: undefined; answer: SyncSummary | undefined };
	/** "Programs with access": the words of the screen and of its form. */
	tokenForm: { given: undefined; answer: TokenForm };
	/** Given a program's name: the name as it will be kept. Fails in the core's words. */
	tokenNameCheck: { given: string; answer: string };
	/** The programs that have access to the pilot's account, the newest first. */
	programs: { given: undefined; answer: ProgramView[] };
	/** Gives a program access. The answer holds the token's secret, this once. */
	giveAccess: { given: AccessWanted; answer: TokenCreated };
	/** Given a token's identifier: withdraws that program's access. */
	withdrawAccess: { given: string; answer: undefined };
	/** The page `/connect/`: its buttons' words and the page of a request that has ended. */
	agreementSaid: { given: undefined; answer: AgreementSaid };
	/**
	 * Given the identifier in the page's address: the request a program
	 * made, as a page, for the account this browser is signed in to. It
	 * never fails, and for text that is no identifier nothing is sent.
	 */
	agreementRead: { given: string; answer: AgreementRead };
	/**
	 * Answers a program's request with the session. The answer may hold the
	 * address the browser goes to, a secret for two minutes: go there and
	 * keep it nowhere.
	 */
	agreementAnswer: { given: { id: string; agree: boolean }; answer: AgreementOutcome };
	/**
	 * Given where Today's take-offs are: the foehn view and Today's line,
	 * by the server's clock. Nothing if this site has no server.
	 */
	foehn: { given: Coordinate[]; answer: FoehnView | undefined };
	/** The sixteen points of the compass an alarm's directions are chosen from. */
	alertPoints: { given: undefined; answer: string[] };
	/** Every word of the alarms' screens. */
	alertFormWords: { given: undefined; answer: AlertFormWords };
	/** What a draft of an alarm waits for, in the core's line. */
	alertLine: { given: AlertDraft; answer: string };
	/** What is wrong with a draft of an alarm, field by field. */
	alertCheck: { given: AlertDraft; answer: AlertFaultView[] };
	/** The alarms and what they fired as the browser holds them, read by the clock given. */
	alertsHeld: { given: { nowUnixS: number; utcOffsetS: number }; answer: AlertsHeld };
	/**
	 * Given how far the clock is ahead of UTC: fetches the alarms and what
	 * they fired. Nothing in a browser that is not signed in.
	 */
	alertsSync: { given: number; answer: AlertsSynced | undefined };
	addAlert: { given: AlertDraft; answer: AlertView };
	changeAlert: { given: { id: string; draft: AlertDraft }; answer: AlertView };
	/** Given an alarm's identifier: removes it. */
	removeAlert: { given: string; answer: undefined };
	/** The words around following, cheers, comments and notifications that no answer carries. */
	socialWords: { given: undefined; answer: SocialWords };
	/** Given a pilot's handle: the counts of followers and followed in words; signed in, whether the pilot follows them. */
	follows: { given: string; answer: FollowList };
	/** Follows a pilot, or follows them no more. */
	follow: { given: { handle: string; follow: boolean }; answer: FollowState };
	/** A page of the flights of the pilots followed; given the `next` of the page before. Nothing when signed out. */
	feed: { given: string | undefined; answer: Feed | undefined };
	/** Given a public flight's address: who cheered, in a line, and the button's word. */
	cheers: { given: string; answer: CheerList };
	cheer: { given: { flightId: string; cheer: boolean }; answer: CheerState };
	/** The comments under a public flight, the oldest first. */
	comments: {
		given: { flightId: string; flightHandle?: string; after?: string };
		answer: CommentList;
	};
	/** A comment while it is written. */
	commentDraft: { given: string; answer: CommentDraft };
	comment: { given: { flightId: string; text: string }; answer: CommentPosted };
	removeComment: { given: { flightId: string; commentId: string }; answer: undefined };
	commentReportForm: { given: undefined; answer: ReportForm };
	reportComment: {
		given: { flightId: string; commentId: string; reason: ReportReason; text: string };
		answer: string;
	};
	/** The switch on the pilot's own flight: whether it takes comments, in words. */
	commentsSwitch: { given: boolean; answer: CommentsSwitch };
	setCommentsOpen: { given: { flightId: string; open: boolean }; answer: CommentsSwitch };
	/** What the pilot was told; given the cursor of the answer before. Nothing when signed out. */
	notifications: { given: string | undefined; answer: Notifications | undefined };
	/** The line beside the switch that lets this browser be told of an alarm. */
	pushLine: { given: undefined; answer: string };
	/** The server's public key for Web Push, base64url. */
	pushKey: { given: undefined; answer: string };
	pushSubscribe: { given: PushSubscribed; answer: undefined };
	/** Given a subscription's endpoint: tells the server it has ended. */
	pushUnsubscribe: { given: string; answer: undefined };
	/** The words around pilots, their totals and the ranking. */
	pilotsWords: { given: undefined; answer: PilotsWords };
	/** A page of a pilot's public flights of a season, between two moments. */
	pilotFlights: {
		given: { handle: string; fromUnixS?: number; toUnixS?: number; after?: string };
		answer: PublicFlights | undefined;
	};
	/** A route's places as a GPX file's text; given latitudes and longitudes in turn. */
	routeToGpx: { given: number[]; answer: string };
	/** A route read from a GPX file's text. Refused in the core's words. */
	routeFromGpx: { given: string; answer: RouteFromFile };
	/** The core's words for the screens that no answer carries, asked once when the app starts. */
	screenWords: { given: undefined; answer: ScreenWords };
	/** Given a flight's identifier: how it is shared, if the server has it. */
	flightSharing: { given: string; answer: FlightSharing | undefined };
	shareFlight: { given: { id: string; visibility: FlightVisibility }; answer: FlightSharing };
	/** What a report can say, and what the pilot is told before sending one. */
	/** The form for a report about such a thing as the subject given. */
	reportForm: { given: ReportSubject; answer: ReportForm };
	/**
	 * Reports a flight, a pilot's page or a live track to whoever runs the
	 * server, from the pilot's account. The answer is what became of it, in
	 * a line.
	 */
	report: {
		given: { subject: ReportSubject; reason: ReportReason; text: string };
		answer: string;
	};
	/**
	 * The pilot's groups and their plans from a day on: the board. Given
	 * the pilot's own today, written `2026-11-21`. This and the calls after
	 * it, down to `planIn`, are sent from the pilot's account and fail in
	 * the core's words where the server refuses.
	 */
	planBoard: { given: string; answer: PlanBoard };
	/** Given a name: makes a group. The answer has its invite code, this once. */
	makeGroup: { given: string; answer: GroupMade };
	/** Given an invite code: takes the pilot into its group. */
	joinGroup: { given: string; answer: GroupJoined };
	/** Given a group's identifier: the group with its members. */
	group: { given: string; answer: GroupDetail };
	renameGroup: { given: { id: string; name: string }; answer: GroupDetail };
	/** Hands the keeping of a group to the member of that handle. */
	handOverGroup: { given: { id: string; handle: string }; answer: GroupDetail };
	/** Given a group's identifier: a new invite code, which ends the one before. */
	newGroupInvite: { given: string; answer: string };
	removeGroupMember: { given: { id: string; handle: string }; answer: undefined };
	/** Given a group's identifier: takes the pilot out of it. */
	leaveGroup: { given: string; answer: undefined };
	deleteGroup: { given: string; answer: undefined };
	postPlan: { given: { group: string; draft: PlanDraft }; answer: PlanView };
	changePlan: { given: { group: string; plan: string; draft: PlanDraft }; answer: PlanView };
	deletePlan: { given: { group: string; plan: string }; answer: undefined };
	/** Says that the pilot is in on a plan, or takes that back. */
	planIn: { given: { group: string; plan: string; isIn: boolean }; answer: PlanView };
	/** An airspace file's name and bytes, as the pilot picked or dropped it. */
	importAirspace: { given: { name: string; bytes: ArrayBuffer }; answer: AirspaceImport };
	airspaceFiles: { given: undefined; answer: AirspaceFile[] };
	/** Given the name a file is stored under. */
	deleteAirspaceFile: { given: string; answer: undefined };
	/** The airspaces that touch a box of the map, from the server's and from the pilot's files. */
	airspaceIn: { given: Bounds & { ceilingM?: number }; answer: AirspaceShape[] };
	/** What airspace lies over a place, in the core's words. */
	airspaceOver: { given: Coordinate; answer: AirspaceOverPlace };
	/** The heights the map's airspace can be filtered by. */
	airspaceHeightFilter: { given: undefined; answer: AirspaceHeightFilter };
	/** Given a flight's identifier: the flight checked against the airspace there is now. */
	flightAirspace: { given: string; answer: FlightAirspace };
	/**
	 * Given a place: asks the server for the airspace around it and keeps it,
	 * if the core says it is time. What is kept afterwards, or nothing when
	 * the server was not asked.
	 */
	refreshAirspace: { given: Coordinate; answer: KeptAirspace[] | undefined };
	/** The airspace kept from the server, one entry for each source. */
	keptAirspace: { given: undefined; answer: KeptAirspace[] };
	forgetKeptAirspace: { given: undefined; answer: undefined };
}

export type CallName = keyof Calls;

/** A call on its way to the core. `id` pairs the answer with it. */
export interface Request<Name extends CallName = CallName> {
	id: number;
	call: Name;
	given: Calls[Name]['given'];
}

/**
 * The core's words for the screens that no answer carries: the same in the
 * three apps (`crates/core/src/screen_words.rs`).
 */
export interface ScreenWords {
	/** "Done", "Back", "More", "OK", "Cancel", "Try again", "Close". */
	common: CommonWords;
	pages: PagesWords;
	/** What the alarm for pilots in the air starts at and may be. */
	pilotsAlarm: PilotsAlarmNumbers;
	/** The route planner's menu: "Export GPX", "Import GPX". */
	planner: PlannerWords;
	siteSheet: SiteSheetWords;
	stationSheet: StationSheetWords;
	today: TodayWords;
}

/** What comes back from the core. */
export type Reply =
	| { id: number; answer: unknown }
	/** The call failed. An error does not survive the trip whole; its name and message do. */
	| { id: number; failure: { name: string; message: string } }
	/** Not an answer: another tab changed the logbook, and the core has caught up. */
	| { changedElsewhere: true };

/** What answers the calls: the core over the browser's store, and the ways to the server. */
export interface Answering {
	session: CoreSession;
	wind: WindSource;
	account: AccountSource;
	tokens: TokensSource;
	agreement: AgreementSource;
	foehn: FoehnSource;
	alerts: AlertsSource;
	social: SocialSource;
	airspace: AirspaceSource;
	sites: SitesSource;
	siteEdits: SiteEditSource;
	takeoffs: TakeoffsSource;
	forecast: ForecastSource;
	live: LiveSource;
	community: CommunitySource;
	groups: GroupsSource;
	terrain: TerrainSource;
	today: TodaySource;
	thermals: ThermalMapSource;
	radar: RadarSource;
	routeWeather: RouteWeatherSource;
	glideReach: GlideReachSource;
	cables: CablesSource;
	slopes: SlopesSource;
	air: AirSource;
}

/** Does what a call asks of the core. */
export async function answer<Name extends CallName>(
	{
		session,
		wind,
		account,
		tokens,
		agreement,
		foehn,
		alerts,
		social,
		airspace,
		sites,
		siteEdits,
		takeoffs,
		forecast,
		live,
		community,
		groups,
		terrain,
		today,
		thermals,
		radar,
		routeWeather,
		glideReach,
		cables,
		slopes,
		air
	}: Answering,
	call: Name,
	given: Calls[Name]['given']
): Promise<Calls[Name]['answer']> {
	const answers: { [N in CallName]: (given: Calls[N]['given']) => unknown } = {
		start: () => ({ durable: session.durable, version: session.version() }),
		logbook: () => session.logbook(),
		statistics: () => session.statistics(),
		siteStatistics: () => session.siteStatistics(),
		tendencies: () => session.tendencies(),
		importFlight: (file) => session.importFlight(flightFileText(file)),
		flight: (id) => session.flight(id),
		flightAnalysis: (id) => session.flightAnalysis(id),
		flightFile: (id) => session.flightFile(id),
		leagueHandover: async ({ id }) => session.leagueHandover(id, await wind.serverNow()),
		deleteFlight: (id) => session.deleteFlight(id),
		flightGround: (asked) => terrain.under(asked),
		forgetFlightGround: (asking) => terrain.forget(asking),
		wind: (box) => wind.windMap(box),
		stationHistory: (id) => wind.stationHistory(id),
		stationAir: ({ station, stations }) => wind.air(station, stations),
		stationDays: (utcOffsetS) => wind.days(utcOffsetS),
		stationDay: ({ stationId, date, utcOffsetS }) => wind.day(stationId, date, utcOffsetS),
		sites: ({ box, filter }) => sites.sites(box, filter),
		siteForm: () => siteEdits.form(),
		siteDraftNew: (place) => siteEdits.draftNew(place),
		siteDraftOf: (site) => siteEdits.draftOf(site),
		siteDraftMoved: ({ draft, place }) => siteEdits.draftMoved(draft, place),
		siteDraftCheck: ({ draft, comment }) => siteEdits.check(draft, comment),
		addSite: ({ draft, comment }) => siteEdits.add(draft, comment),
		changeSite: ({ draft, comment }) => siteEdits.change(draft, comment),
		removeSite: ({ id, comment }) => siteEdits.remove(id, comment),
		restoreSite: ({ id, revision, comment }) => siteEdits.restore(id, revision, comment),
		siteWords: () => siteEdits.words(),
		siteOffers: async ({ site, confirmations }) =>
			siteEdits.offers(site, await wind.takesSitesFromPilots(), confirmations),
		siteAdding: async () => siteEdits.adding(await wind.takesSitesFromPilots()),
		siteConfirmForm: (today) => siteEdits.confirmForm(today),
		siteConfirmFlights: ({ site, day, utcOffsetS }) =>
			siteEdits.confirmFlights(site, day, utcOffsetS),
		confirmSite: ({ id, day, today, note, flight }) =>
			siteEdits.confirm(id, day, today, note, flight),
		withdrawSiteConfirmation: (id) => siteEdits.withdraw(id),
		siteConfirmations: (site) => siteEdits.confirmations(site),
		siteCorrecting: (site) => siteEdits.correcting(site),
		siteHistoryOffers: async (history) =>
			siteEdits.historyOffers(history, await wind.takesSitesFromPilots()),
		sitesRemoved: async (fromPilots) =>
			siteEdits.removed(fromPilots, await wind.takesSitesFromPilots()),
		siteDraftRebased: ({ base, edited, now }) => siteEdits.rebased(base, edited, now),
		sitePage: (id) => siteEdits.page(id),
		siteHistory: (id) => siteEdits.history(id),
		siteFlightsWords: () => takeoffs.words(),
		siteFlights: (id) => takeoffs.site(id),
		siteCandidates: (box) => takeoffs.candidates(box),
		candidateOffers: ({ candidates, utcOffsetS }) => takeoffs.offers(candidates, utcOffsetS),
		setCandidateAside: (position) => takeoffs.setAside(position),
		sitesFilterChoices: () => sites.filterChoices(),
		filterSites: ({ sites: held, filter }) => sites.filter(held, filter),
		forecast: (place) => forecast.forecast(place),
		live: (box) => live.gliders(box),
		followLive: (id) => live.follow(id),
		unfollowLive: () => live.unfollow(),
		liveAges: (asked) => live.ages(asked),
		publicFlights: (asked) => community.flights(asked),
		pilotPage: ({ handle, after }) => community.pilot(handle, after),
		ranking: ({ season, near, month }) => community.ranking(season, near, undefined, month),
		pilotsFound: (text) => community.pilots(text),
		pilotStatistics: (handle) => community.statistics(handle),
		siteAt: ({ place, id }) => sites.siteAt(place, id),
		openPublicFlight: (id) => community.open(id),
		todayTakeoffs: (place) => today.takeoffs(place),
		todayOrders: (days) => todayOrders(days),
		planRoute: (places) => routePlan(places),
		routeWeather: (asked) => routeWeather.along(asked),
		glideReach: (setting) => glideReach.reach(setting),
		forgetGlideReach: () => glideReach.forget(),
		cables: (place) => cables.around(place),
		slopes: (asked) => slopes.classes(asked),
		forgetSlopes: () => slopes.forget(),
		liveClimbs: (box) => air.climbs(box),
		flownThermals: (box) => air.flown(box),
		mapLayersWords: () => air.words(),
		siteAir: (site) => air.siteAir(site),
		keepSite: ({ site, credits, asOfUnixS }) => session.keepSite(site, credits, asOfUnixS),
		forgetSite: (id) => session.forgetSite(id),
		isSiteKept: (id) => session.isSiteKept(id),
		keptSites: (from) => session.keptSites(from),
		todayOutlook: ({ id, nowUnixS }) => today.outlook(id, nowUnixS),
		siteOutlook: (site) => today.outlookFor(site),
		thermalMap: () => thermals.map(),
		thermalHours: (asked) => thermals.hours(asked),
		thermalHourShapes: ({ asked, hour }) => thermals.shapes(asked, hour),
		thermalHourSquare: ({ asked, cell, hour }) => thermals.square(asked, cell, hour),
		thermalCellAt: ({ asked, lat, lon }) => thermals.cellAt(asked, lat, lon),
		thermalStrip: (place) => thermals.strip(place.lat, place.lon),
		radar: () => radar.radar(),
		radarAgain: () => radar.again(),
		radarPicture: ({ path, above, colours }) => radar.picture(path, above, colours),
		compareFlights: ({ first, other }) =>
			community.compare(first, other, async (id) => (await session.flightFile(id)).igcText),
		hasServer: () => wind.present(),
		registration: () => wind.registration(),
		googleSignIn: () => wind.googleSignIn(),
		googleWords: () => account.googleWords(),
		signInWithGoogle: (idToken) => account.googleSignIn(idToken),
		createGoogleAccount: (wanted) => account.createGoogleAccount(wanted),
		linkGoogle: (idToken) => account.linkGoogle(idToken),
		unlinkGoogle: () => account.unlinkGoogle(),
		deleteAccountWithGoogle: (idToken) => account.deleteAccountWithGoogle(idToken),
		account: () => account.account(),
		refreshAccount: () => account.refresh(),
		signIn: ({ handle, password }) => account.signIn(handle, password),
		createAccount: (wanted) => account.createAccount(wanted),
		signOut: () => account.signOut(),
		deleteAccount: (password) => account.deleteAccount(password),
		sync: () => account.sync(),
		syncKeptSites: () => account.syncKeptSites(),
		tokenForm: () => tokens.form(),
		tokenNameCheck: (name) => tokens.checkName(name),
		programs: () => tokens.list(),
		giveAccess: (wanted) => tokens.give(wanted),
		withdrawAccess: (id) => tokens.withdraw(id),
		agreementSaid: () => agreement.said(),
		agreementRead: (id) => agreement.read(id),
		agreementAnswer: ({ id, agree }) => agreement.answer(id, agree),
		foehn: (places) => foehn.read(places),
		socialWords: () => social.words(),
		follows: (handle) => social.follows(handle),
		follow: ({ handle, follow }) => social.follow(handle, follow),
		feed: (before) => social.feed(before),
		cheers: (flightId) => social.cheers(flightId),
		cheer: ({ flightId, cheer }) => social.cheer(flightId, cheer),
		comments: ({ flightId, flightHandle, after }) => social.comments(flightId, flightHandle, after),
		commentDraft: (text) => social.draft(text),
		comment: ({ flightId, text }) => social.comment(flightId, text),
		removeComment: ({ flightId, commentId }) => social.removeComment(flightId, commentId),
		commentReportForm: () => social.commentReportForm(),
		reportComment: ({ flightId, commentId, reason, text }) =>
			social.reportComment(flightId, commentId, reason, text),
		commentsSwitch: (open) => social.commentsSwitch(open),
		setCommentsOpen: ({ flightId, open }) => social.setCommentsOpen(flightId, open),
		notifications: (since) => social.notifications(since),
		pushLine: () => social.pushLine(),
		pushKey: () => social.pushKey(),
		pushSubscribe: (subscribed) => social.pushSubscribe(subscribed),
		pushUnsubscribe: (endpoint) => social.pushUnsubscribe(endpoint),
		pilotsWords: () => pilotsWords(),
		pilotFlights: ({ handle, fromUnixS, toUnixS, after }) =>
			community.pilotFlights(handle, fromUnixS, toUnixS, after),
		routeToGpx: (latLon) => routeToGpx(new Float64Array(latLon)),
		routeFromGpx: (text) => routeFromGpx(text),
		screenWords: () => ({
			common: commonWords(),
			pages: pagesWords(),
			pilotsAlarm: pilotsAlarmNumbers(),
			planner: plannerWords(),
			siteSheet: siteSheetWords(),
			stationSheet: stationSheetWords(),
			today: todayWords()
		}),
		alertPoints: () => alerts.points(),
		alertFormWords: () => alerts.formWords(),
		alertLine: (draft) => alerts.line(draft),
		alertCheck: (draft) => alerts.check(draft),
		alertsHeld: ({ nowUnixS, utcOffsetS }) => alerts.held(nowUnixS, utcOffsetS),
		alertsSync: (utcOffsetS) => alerts.sync(utcOffsetS),
		addAlert: (draft) => alerts.add(draft),
		changeAlert: ({ id, draft }) => alerts.change(id, draft),
		removeAlert: (id) => alerts.remove(id),
		flightSharing: (id) => account.sharing(id),
		shareFlight: ({ id, visibility }) => account.share(id, visibility),
		reportForm: (subject) => account.reportForm(subject),
		report: ({ subject, reason, text }) => account.report(subject, reason, text),
		planBoard: (fromDay) => groups.board(fromDay),
		makeGroup: (name) => groups.make(name),
		joinGroup: (code) => groups.join(code),
		group: (id) => groups.group(id),
		renameGroup: ({ id, name }) => groups.rename(id, name),
		handOverGroup: ({ id, handle }) => groups.handOver(id, handle),
		newGroupInvite: (id) => groups.newInvite(id),
		removeGroupMember: ({ id, handle }) => groups.remove(id, handle),
		leaveGroup: (id) => groups.leave(id),
		deleteGroup: (id) => groups.delete(id),
		postPlan: ({ group, draft }) => groups.postPlan(group, draft),
		changePlan: ({ group, plan, draft }) => groups.changePlan(group, plan, draft),
		deletePlan: ({ group, plan }) => groups.deletePlan(group, plan),
		planIn: ({ group, plan, isIn }) => groups.setIn(group, plan, isIn),
		importAirspace: ({ name, bytes }) =>
			session.importAirspace(name, new Uint8Array(bytes), Math.floor(Date.now() / 1000)),
		airspaceFiles: () => session.airspaceFiles(),
		deleteAirspaceFile: (storedName) => session.deleteAirspaceFile(storedName),
		airspaceIn: (box) => session.airspaceIn(box, box.ceilingM),
		airspaceOver: (place) => session.airspaceOver(place),
		airspaceHeightFilter: () => session.airspaceHeightFilter(),
		flightAirspace: (id) => session.flightAirspace(id),
		refreshAirspace: (place) => airspace.refresh(place),
		keptAirspace: () => airspace.kept(),
		forgetKeptAirspace: () => airspace.forget()
	};
	return (await answers[call](given)) as Calls[Name]['answer'];
}

/**
 * The name of the error for a server that gave no answer at all. Like the
 * core's own errors, its message is written for the pilot.
 */
export const UNREACHABLE = 'Unreachable';

/** The error for a server that gave no answer at all, with words for the pilot. */
export function unreachable(message: string): Error {
	const error = new Error(message);
	error.name = UNREACHABLE;
	return error;
}

/** An error as it is sent to the page. */
export function describeFailure(error: unknown): { name: string; message: string } {
	return error instanceof Error
		? { name: error.name, message: error.message }
		: { name: 'Error', message: String(error) };
}

/** The error the page throws for a failure the core sent. */
export function failureAsError(failure: { name: string; message: string }): Error {
	const error = new Error(failure.message);
	error.name = failure.name;
	return error;
}
