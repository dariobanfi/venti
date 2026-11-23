/**
 * The pilot's own flights, on the Venti server only (owner decision 20: the
 * website holds flights online and keeps no logbook in the browser). The
 * worker lists them, opens one from its file, brings a file in as an
 * upload, changes who may see one or the class of its glider, deletes one
 * and asks what all of them add up to, each with the session token, and
 * keeps nothing.
 *
 * Every request and every reading of an answer is the core's
 * (`ownFlightsRequest`, `ownFlights`, `uploadRequest`, …): this file only
 * sends what the core wrote and hands it the answer. A flight's file is
 * read with `flightFromFile`, `flightAnalysisFromFile` and
 * `flightFileFromText`, as everyone's flights are.
 */

import { flightFileText } from './flight-text';
import {
	deleteFlightRequest,
	flightAnalysisFromFile,
	type FlightAnalysis,
	flightDeleted,
	flightFileFromText,
	flightFromFile,
	flightGliderClassRequest,
	type FlightSharing,
	type FlightView,
	type FlightVisibility,
	flightVisibilityRequest,
	type GliderClassKind,
	type OwnFlight,
	ownFlight,
	ownFlightRequest,
	type OwnFlights,
	ownFlights,
	ownFlightsRequest,
	ownHabitsRequest,
	ownStatisticsRequest,
	ownTendencies,
	pilotStatistics,
	type PilotStatistics,
	publicFlight,
	type ServerRequest,
	type Tendencies,
	uploadedFlight,
	type UploadedFlight,
	uploadRequest
} from './pkg/venti_core_wasm.js';
import { type Send, sendToThisOrigin } from './account-source';

export type { OwnFlight, OwnFlights };

/** One of the pilot's flights, prepared for display. */
export interface OpenedOwnFlight extends OwnFlight {
	flight: FlightView;
	/** What happened in it. Without it the flight is still shown in full. */
	analysis: FlightAnalysis | undefined;
	/**
	 * The file's text, as it was uploaded: the page hands it back for the
	 * airspace check and the league's handover, so that it is fetched once.
	 */
	text: string;
	/** The name the logbook would save the file under (`flightFileFromText`). */
	name: string;
}

/** What became of a file brought in. */
export type Uploaded = UploadedFlight;

export class OwnFlightsSource {
	#token: () => Promise<string | undefined>;
	#send: Send;
	/** The address the requests go to, for the flights' links. */
	#server: string;

	constructor(
		token: () => Promise<string | undefined>,
		send: Send = sendToThisOrigin,
		server: string = location.origin
	) {
		this.#token = token;
		this.#send = send;
		this.#server = server;
	}

	/** Whether a session is there to ask with. Without one nothing is asked. */
	async signedIn(): Promise<boolean> {
		return (await this.#token()) !== undefined;
	}

	/**
	 * One page of the account's flights, the newest first. Nothing when this
	 * browser is not signed in. Throws in the core's words when the server
	 * refuses, as for a session that has ended.
	 */
	async list(after?: string): Promise<OwnFlights | undefined> {
		const answer = await this.#ask(ownFlightsRequest(after));
		return answer && ownFlights(this.#server, answer.status, answer.body);
	}

	/** Every flight of the account, page after page: for the map and for choosing one to compare. */
	async all(): Promise<OwnFlight[] | undefined> {
		const first = await this.list();
		if (!first) return undefined;
		const flights = [...first.flights];
		for (let next = first.next; next;) {
			const page = await this.list(next);
			if (!page) break;
			flights.push(...page.flights);
			next = page.next;
		}
		return flights;
	}

	/** One of the account's flights, from its file. Throws in words when it cannot be had. */
	async open(id: string): Promise<OpenedOwnFlight | undefined> {
		const fetched = await this.file(id);
		if (!fetched) return undefined;
		const flight = flightFromFile(fetched.text);
		let analysis: FlightAnalysis | undefined;
		try {
			analysis = flightAnalysisFromFile(fetched.text);
		} catch {
			// Without the analysis the details are shorter, not wrong.
		}
		return { ...fetched, flight, analysis };
	}

	/**
	 * A flight's file as it was uploaded, with the name the logbook would
	 * save it under and what the list says about it. Nothing when this
	 * browser is not signed in.
	 */
	async file(id: string): Promise<(OwnFlight & { text: string; name: string }) | undefined> {
		const about = await this.#ask(ownFlightRequest(id));
		if (!about) return undefined;
		const own = ownFlight(this.#server, about.status, about.body);
		const file = await this.#ask(fileRequest(own.row.filePath));
		if (!file) return undefined;
		// Not the file: the core puts what the server said into words, by throwing.
		if (file.status < 200 || file.status > 299) publicFlight(file.status, file.body);
		const text = flightFileText(file.body);
		return { ...own, text, name: flightFileFromText(text).name };
	}

	/** Brings a flight file in: an upload to the account. Throws in the core's words when refused. */
	async upload(bytes: ArrayBuffer): Promise<Uploaded | undefined> {
		const answer = await this.#ask(uploadRequest(new Uint8Array(bytes)));
		return answer && uploadedFlight(this.#server, answer.status, answer.body);
	}

	/** Changes who may see a flight, and says how it is shared now. */
	async share(id: string, visibility: FlightVisibility): Promise<FlightSharing> {
		return (await this.#change(flightVisibilityRequest(id, visibility))).sharing;
	}

	/** Sets the class of the glider a flight was flown on, or says it is not known. */
	async setGliderClass(id: string, gliderClass: GliderClassKind | undefined): Promise<OwnFlight> {
		return this.#change(flightGliderClassRequest(id, gliderClass));
	}

	/** Deletes a flight of the account, at once and for good. */
	async delete(id: string): Promise<void> {
		const answer = await this.#send(deleteFlightRequest(id), await this.#token());
		flightDeleted(answer.status, answer.body);
	}

	/**
	 * What all the account's flights add up to, the private ones too, as the
	 * core reads a pilot's statistics. Nothing when this browser is not
	 * signed in.
	 */
	async statistics(): Promise<PilotStatistics | undefined> {
		const answer = await this.#ask(ownStatisticsRequest());
		return answer && pilotStatistics(answer.status, answer.body);
	}

	/**
	 * What the account's flights, the private ones too, say about how the
	 * pilot flies: the core's observations ("How you fly") over the figures
	 * the server keeps of each flight. Nothing when this browser is not
	 * signed in.
	 */
	async tendencies(): Promise<Tendencies | undefined> {
		const answer = await this.#ask(ownHabitsRequest());
		return answer && ownTendencies(answer.status, answer.body);
	}

	/** A change to one flight, read as the flight now is. */
	async #change(request: ServerRequest): Promise<OwnFlight> {
		const answer = await this.#send(request, await this.#token());
		return ownFlight(this.#server, answer.status, answer.body);
	}

	/** Sends a request of the core's with the session; nothing without one. */
	async #ask(request: ServerRequest) {
		const token = await this.#token();
		if (!token) return undefined;
		return this.#send(request, token);
	}
}

/** The request for a flight's file, at the path the core read from its row. */
function fileRequest(path: string): ServerRequest {
	return { method: 'GET', path, body: undefined, contentType: undefined, needsToken: true };
}
