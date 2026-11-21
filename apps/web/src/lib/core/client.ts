/**
 * The page's side of the conversation in `protocol.ts`: the core as the
 * screens call it. Every call is sent to the worker and answered later, so
 * nothing the core does can hold up the page.
 */

import type { NewAccount } from './account-source';
import type { FlightRef, PublicFlightsAsked } from './community-source';
import type { GroundAsked } from './terrain-source';
import type { RadarColours } from './radar-source';
import type { HoursAsked } from './thermal-map-source';
import type {
	Bounds,
	Coordinate,
	Credit,
	FlightVisibility,
	PlanDraft,
	ReportReason,
	ReportSubject,
	SitesFilter,
	SiteView,
	TodayStanding,
	WindStation
} from './pkg/venti_core_wasm.js';
import {
	type CallName,
	type Calls,
	failureAsError,
	type Reply,
	type Request,
	type Started
} from './protocol';

/** Something that takes requests and gives replies: the worker, or a stand-in for it in a test. */
export interface Channel {
	postMessage(request: Request, transfer?: Transferable[]): void;
	onmessage: ((event: MessageEvent<Reply>) => void) | null;
	/** Called when the worker itself breaks down, which no reply will report. */
	onerror: ((event: ErrorEvent) => void) | null;
}

export class CoreClient {
	#channel: Channel;
	#nextId = 1;
	#waiting = new Map<
		number,
		{ resolve: (answer: unknown) => void; reject: (error: Error) => void }
	>();
	#started: Started;

	/** Called after another tab changed the logbook and the core has caught up. */
	onChangedElsewhere: () => void = () => {};

	private constructor(channel: Channel, started: Started) {
		this.#channel = channel;
		this.#started = started;
	}

	/** Starts the core in a worker of its own and waits until it is ready. */
	static async open(
		channel: Channel = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
	): Promise<CoreClient> {
		const client = new CoreClient(channel, { durable: true, version: '' });
		channel.onmessage = (event) => client.#received(event.data);
		channel.onerror = (event) => client.#brokeDown(event.message);
		client.#started = await client.#call('start', undefined);
		return client;
	}

	/** Whether the logbook is still there after the page is closed. */
	get durable(): boolean {
		return this.#started.durable;
	}

	/** The version of the core, for bug reports. */
	get version(): string {
		return this.#started.version;
	}

	logbook() {
		return this.#call('logbook', undefined);
	}

	statistics() {
		return this.#call('statistics', undefined);
	}

	/** The places flights began at, the one flown from most often first. */
	siteStatistics() {
		return this.#call('siteStatistics', undefined);
	}

	/** What the pilot's own tracks say about how they fly. */
	tendencies() {
		return this.#call('tendencies', undefined);
	}

	/** Imports a flight file. Its bytes are handed over, not copied: `file` is empty afterwards. */
	importFlight(file: ArrayBuffer) {
		return this.#call('importFlight', file, [file]);
	}

	flight(id: string) {
		return this.#call('flight', id);
	}

	flightAnalysis(id: string) {
		return this.#call('flightAnalysis', id);
	}

	flightFile(id: string) {
		return this.#call('flightFile', id);
	}

	/**
	 * What the pilot is told before a flight of the logbook goes to a
	 * league, with its file, by the browser's clock now.
	 */
	leagueHandover(id: string) {
		// The page's clock: the one a pilot sees, and the one a test sets.
		return this.#call('leagueHandover', { id, nowUnixS: Date.now() / 1000 });
	}

	deleteFlight(id: string) {
		return this.#call('deleteFlight', id);
	}

	/**
	 * The ground under a flight's track, from the map's elevation tiles,
	 * which are fetched for it. Empty where there is none to be had, and
	 * when another flight has been asked about by now.
	 */
	flightGround(asked: GroundAsked) {
		return this.#call('flightGround', asked);
	}

	/** Lets go of a question about the ground under a flight, by its number. */
	forgetFlightGround(asking: number) {
		return this.#call('forgetFlightGround', asking);
	}

	/** The wind stations in a box of the map. Nothing if this site has no server. */
	wind(box: Bounds) {
		return this.#call('wind', box);
	}

	/** What a wind station measured over the last 24 hours. */
	stationHistory(stationId: string) {
		return this.#call('stationHistory', stationId);
	}

	/** What a station's readings say about the air, beside the other stations on the map. */
	stationAir(station: WindStation, stations: WindStation[]) {
		return this.#call('stationAir', { station, stations });
	}

	/** The days a station's readings can be asked for, by the reader's offset from UTC. */
	stationDays(utcOffsetS: number) {
		return this.#call('stationDays', utcOffsetS);
	}

	/** What a station measured on one day, a date as `stationDays` names it. */
	stationDay(stationId: string, date: string, utcOffsetS: number) {
		return this.#call('stationDay', { stationId, date, utcOffsetS });
	}

	/** The flying sites in a box of the map. Nothing if this site has no server. */
	sites(box: Bounds) {
		return this.#call('sites', box);
	}

	/** The choices the filter of sites offers, with the core's words. */
	sitesFilterChoices() {
		return this.#call('sitesFilterChoices', undefined);
	}

	/** What a filter leaves of sites the page holds, and the core's words for it. */
	filterSites(sites: SiteView[], filter: SitesFilter) {
		return this.#call('filterSites', { sites, filter });
	}

	/**
	 * The gliders in a box of the map whose pilots show their flight to
	 * everyone. Nothing if this site has no server.
	 */
	live(box: Bounds) {
		return this.#call('live', box);
	}

	/**
	 * Asks the server once about the flight of a live track: for all of it
	 * the first time, then for what is new. Nothing when there is no server,
	 * or when another flight is followed by now.
	 */
	followLive(id: string) {
		return this.#call('followLive', id);
	}

	/** Lets go of the flight being followed. */
	unfollowLive() {
		return this.#call('unfollowLive', undefined);
	}

	/** How old positions are, in the core's words, one answer for each. */
	liveAges(asked: { ageS: number; landed: boolean }[]) {
		return this.#call('liveAges', asked);
	}

	/**
	 * One page of the flights the server's pilots share with everyone.
	 * Nothing if this site has no server.
	 */
	publicFlights(asked: PublicFlightsAsked) {
		return this.#call('publicFlights', asked);
	}

	/** A pilot, by handle, and one page of that pilot's public flights. */
	pilotPage(handle: string, after?: string) {
		return this.#call('pilotPage', { handle, after });
	}

	/**
	 * The server's pilots ranked by their best public flights, in the
	 * season of that name or in the newest, and with a place only of the
	 * flights from there. Nothing if this site has no server.
	 */
	ranking(season?: string, near?: Coordinate) {
		return this.#call('ranking', { season, near });
	}

	/**
	 * The flying site at a place: the one of that identifier, or the
	 * nearest take-off. Nothing if this site has no server.
	 */
	siteAt(place: Coordinate, id?: string) {
		return this.#call('siteAt', { place, id });
	}

	/**
	 * Another pilot's flight, by its address on the server, fetched and
	 * prepared for display. Nothing of it is kept.
	 */
	openPublicFlight(id: string) {
		return this.#call('openPublicFlight', id);
	}

	/**
	 * The take-offs Today lists for a place, as the core chooses them: the
	 * pilot's own, then those around. Nothing if this site has no server.
	 */
	todayTakeoffs(place: Coordinate) {
		return this.#call('todayTakeoffs', place);
	}

	/** The core's order of Today's take-offs on each of several days. */
	todayOrders(days: TodayStanding[][]) {
		return this.#call('todayOrders', days);
	}

	/**
	 * Marks a site as one of the pilot's own and keeps it in the browser:
	 * the site as the core gave it, the credits of the answer it came from
	 * and the date of the source's data by that answer.
	 */
	keepSite(site: SiteView, credits: Credit[], asOfUnixS: number | undefined) {
		return this.#call('keepSite', { site, credits, asOfUnixS });
	}

	/** A site is no longer one of the pilot's own. */
	forgetSite(id: string) {
		return this.#call('forgetSite', id);
	}

	/** Whether a site is one of the pilot's own. */
	isSiteKept(id: string) {
		return this.#call('isSiteKept', id);
	}

	/** The pilot's own sites: from a place the nearest first, without one by name. */
	keptSites(from?: Coordinate) {
		return this.#call('keptSites', from);
	}

	/** What the forecast says about flying from a site, by Venti's clock now. */
	siteOutlook(site: SiteView) {
		return this.#call('siteOutlook', {
			position: site.position,
			wind: site.wind,
			elevationM: site.elevationM
		});
	}

	/** The kind of day, square by square, from today on. Nothing if this site has no server. */
	thermalMap() {
		return this.#call('thermalMap', undefined);
	}

	/** One layer of one day of the thermal map hour by hour. */
	thermalHours(asked: HoursAsked) {
		return this.#call('thermalHours', asked);
	}

	/** What the map draws of one hour of those hours: two GeoJSON texts. */
	thermalHourShapes(asked: HoursAsked, hour: number) {
		return this.#call('thermalHourShapes', { asked, hour });
	}

	/** A square in one hour of those hours, in words. */
	thermalHourSquare(asked: HoursAsked, cell: number, hour: number) {
		return this.#call('thermalHourSquare', { asked, cell, hour });
	}

	/** The square a place lies in. */
	thermalCellAt(asked: HoursAsked, lat: number, lon: number) {
		return this.#call('thermalCellAt', { asked, lat, lon });
	}

	/** Every day of the map at the square of a place, side by side. */
	thermalStrip(place: Coordinate) {
		return this.#call('thermalStrip', place);
	}

	/** The rain radar's layers and their frames. Nothing if this site has no server. */
	radar() {
		return this.#call('radar', undefined);
	}

	/** The list of frames last fetched, read again at this moment: each frame as old as it is now. */
	radarAgain() {
		return this.#call('radarAgain', undefined);
	}

	/** A frame's picture in the colours given, as the bytes of a PNG. */
	radarPicture(path: string, colours: RadarColours) {
		return this.#call('radarPicture', { path, colours });
	}

	/** What the forecast says about flying from one of the take-offs last listed. */
	todayOutlook(id: string, nowUnixS: number) {
		return this.#call('todayOutlook', { id, nowUnixS });
	}

	/**
	 * Lays the flight `other` beside `first`, the one on screen: the other
	 * flight and how the two lie to each other. Nothing is kept.
	 */
	compareFlights(first: FlightRef, other: FlightRef) {
		return this.#call('compareFlights', { first, other });
	}

	/** What the server's weather models say about a place. Nothing if this site has no server. */
	forecast(place: Coordinate) {
		return this.#call('forecast', place);
	}

	/** Whether this site has a Venti server behind it. */
	hasServer() {
		return this.#call('hasServer', undefined);
	}

	/** Who may create an account on this site's server, if the server says. */
	registration() {
		return this.#call('registration', undefined);
	}

	/** The account this browser is signed in to, if any. */
	account() {
		return this.#call('account', undefined);
	}

	signIn(handle: string, password: string) {
		return this.#call('signIn', { handle, password });
	}

	createAccount(wanted: NewAccount) {
		return this.#call('createAccount', wanted);
	}

	signOut() {
		return this.#call('signOut', undefined);
	}

	/** Deletes the account on the server. It needs the password. */
	deleteAccount(password: string) {
		return this.#call('deleteAccount', password);
	}

	/** Brings the logbook and the server in step. */
	sync() {
		return this.#call('sync', undefined);
	}

	/** How a flight is shared, if the server has it. */
	flightSharing(id: string) {
		return this.#call('flightSharing', id);
	}

	/** Changes who may see a flight. */
	shareFlight(id: string, visibility: FlightVisibility) {
		return this.#call('shareFlight', { id, visibility });
	}

	/** What a report can say, and what the pilot is told before sending one. */
	reportForm(subject: ReportSubject) {
		return this.#call('reportForm', subject);
	}

	/**
	 * Reports a flight, a pilot's page or a live track to whoever runs the
	 * server, from the pilot's account. Answers in a line what became of it.
	 */
	report(subject: ReportSubject, reason: ReportReason, text: string) {
		return this.#call('report', { subject, reason, text });
	}

	/**
	 * The pilot's groups and their plans from a day on: the board.
	 * `fromDay` is the pilot's own today, written `2026-11-21`.
	 */
	planBoard(fromDay: string) {
		return this.#call('planBoard', fromDay);
	}

	/** Makes a group, kept by the pilot. The answer has its invite code, this once. */
	makeGroup(name: string) {
		return this.#call('makeGroup', name);
	}

	/** Takes the pilot into the group an invite code belongs to. */
	joinGroup(code: string) {
		return this.#call('joinGroup', code);
	}

	/** One group with its members. */
	group(id: string) {
		return this.#call('group', id);
	}

	renameGroup(id: string, name: string) {
		return this.#call('renameGroup', { id, name });
	}

	/** Hands the keeping of a group to another member. */
	handOverGroup(id: string, handle: string) {
		return this.#call('handOverGroup', { id, handle });
	}

	/** A new invite code for a group. The one before it stops working. */
	newGroupInvite(id: string) {
		return this.#call('newGroupInvite', id);
	}

	removeGroupMember(id: string, handle: string) {
		return this.#call('removeGroupMember', { id, handle });
	}

	leaveGroup(id: string) {
		return this.#call('leaveGroup', id);
	}

	deleteGroup(id: string) {
		return this.#call('deleteGroup', id);
	}

	postPlan(group: string, draft: PlanDraft) {
		return this.#call('postPlan', { group, draft });
	}

	changePlan(group: string, plan: string, draft: PlanDraft) {
		return this.#call('changePlan', { group, plan, draft });
	}

	deletePlan(group: string, plan: string) {
		return this.#call('deletePlan', { group, plan });
	}

	/** Says that the pilot is in on a plan, or takes that back. */
	planIn(group: string, plan: string, isIn: boolean) {
		return this.#call('planIn', { group, plan, isIn });
	}

	/** Takes in an airspace file. Its bytes are handed over, not copied. */
	importAirspace(name: string, bytes: ArrayBuffer) {
		return this.#call('importAirspace', { name, bytes }, [bytes]);
	}

	airspaceFiles() {
		return this.#call('airspaceFiles', undefined);
	}

	deleteAirspaceFile(storedName: string) {
		return this.#call('deleteAirspaceFile', storedName);
	}

	/** The airspaces that touch a box of the map; with a ceiling, without those certainly above it. */
	airspaceIn(box: Bounds, ceilingM?: number) {
		return this.#call('airspaceIn', { ...box, ceilingM });
	}

	/** What airspace lies over a place, in the core's words. */
	airspaceOver(place: Coordinate) {
		return this.#call('airspaceOver', place);
	}

	/** The heights the map's airspace can be filtered by. */
	airspaceHeightFilter() {
		return this.#call('airspaceHeightFilter', undefined);
	}

	/** A flight of the logbook checked against the airspace there is now. */
	flightAirspace(id: string) {
		return this.#call('flightAirspace', id);
	}

	/**
	 * Asks the server for the airspace around a place and keeps it, if the
	 * core says it is time. What is kept afterwards, or nothing when the
	 * server was not asked.
	 */
	refreshAirspace(place: Coordinate) {
		return this.#call('refreshAirspace', place);
	}

	/** The airspace kept from the server, one entry for each source. */
	keptAirspace() {
		return this.#call('keptAirspace', undefined);
	}

	forgetKeptAirspace() {
		return this.#call('forgetKeptAirspace', undefined);
	}

	#call<Name extends CallName>(
		call: Name,
		given: Calls[Name]['given'],
		transfer: Transferable[] = []
	): Promise<Calls[Name]['answer']> {
		const id = this.#nextId++;
		return new Promise((resolve, reject) => {
			this.#waiting.set(id, { resolve: resolve as (answer: unknown) => void, reject });
			this.#channel.postMessage({ id, call, given }, transfer);
		});
	}

	/** The worker cannot answer any more: nobody should go on waiting for it. */
	#brokeDown(reason: string): void {
		const error = new Error(reason || 'The part of the app that keeps the logbook stopped.');
		for (const waiting of this.#waiting.values()) waiting.reject(error);
		this.#waiting.clear();
	}

	#received(reply: Reply): void {
		if ('changedElsewhere' in reply) {
			this.onChangedElsewhere();
			return;
		}
		const waiting = this.#waiting.get(reply.id);
		if (!waiting) return;
		this.#waiting.delete(reply.id);
		if ('failure' in reply) waiting.reject(failureAsError(reply.failure));
		else waiting.resolve(reply.answer);
	}
}
