/**
 * The page's side of the conversation in `protocol.ts`: the core as the
 * screens call it. Every call is sent to the worker and answered later, so
 * nothing the core does can hold up the page.
 */

import type { NewAccount } from './account-source';
import type { Bounds, Coordinate, FlightVisibility } from './pkg/venti_core_wasm.js';
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

	deleteFlight(id: string) {
		return this.#call('deleteFlight', id);
	}

	/** The wind stations in a box of the map. Nothing if this site has no server. */
	wind(box: Bounds) {
		return this.#call('wind', box);
	}

	/** What a wind station measured over the last 24 hours. */
	stationHistory(stationId: string) {
		return this.#call('stationHistory', stationId);
	}

	/** The flying sites in a box of the map. Nothing if this site has no server. */
	sites(box: Bounds) {
		return this.#call('sites', box);
	}

	/** What the server's weather models say about a place. Nothing if this site has no server. */
	forecast(place: Coordinate) {
		return this.#call('forecast', place);
	}

	/** Whether this site has a Venti server behind it. */
	hasServer() {
		return this.#call('hasServer', undefined);
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

	/** The airspaces that touch a box of the map. */
	airspaceIn(box: Bounds) {
		return this.#call('airspaceIn', box);
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
