/**
 * The pilot's own flights, on the Venti server only (owner decision 20: the
 * website holds flights online and keeps no logbook in the browser). The
 * worker lists them, opens one from its file, brings a file in as an
 * upload, changes who may see one and deletes one, each with the session
 * token, and keeps nothing.
 *
 * The core reads the answers, as it does everyone's flights: a list with
 * `publicFlights`, one flight with `publicFlight`, a file with
 * `flightFromFile` and `flightAnalysisFromFile`. What it has no call for
 * yet is written here and named so: the path of the account's own list
 * (`GET /v1/flights`), the upload, the change of who may see a flight and
 * the deletion are requests this file makes, and a flight's `visibility`
 * is read from the answer here, since the core's row of a flight leaves it
 * out. `apps/web/AGENTS.md` lists them as the core's to take over.
 */

import { flightFileText } from './flight-text';
import {
	flightAnalysisFromFile,
	type FlightAnalysis,
	type FlightSharing,
	type FlightVisibility,
	flightFromFile,
	type FlightView,
	publicFlight,
	type PublicFlight,
	publicFlightPath,
	publicFlights,
	type ServerRequest
} from './pkg/venti_core_wasm.js';
import { type Send, sendToThisOrigin } from './account-source';

/** One of the pilot's flights as the list shows it, with who may see it. */
export interface OwnFlight {
	row: PublicFlight;
	sharing: FlightSharing;
}

/** A page of the pilot's flights, the newest first. */
export interface OwnFlights {
	flights: OwnFlight[];
	/** What to pass for the page after this one. Absent on the last. */
	next: string | undefined;
}

/** One of the pilot's flights, prepared for display. */
export interface OpenedOwnFlight extends OwnFlight {
	flight: FlightView;
	/** What happened in it. Without it the flight is still shown in full. */
	analysis: FlightAnalysis | undefined;
}

/** What became of a file brought in. */
export interface Uploaded {
	flight: OwnFlight;
	/** Whether the account had it already. */
	alreadyThere: boolean;
}

/** As many flights as the server gives in one page. */
const PAGE = 100;

export class OwnFlightsSource {
	#token: () => Promise<string | undefined>;
	#send: Send;
	#origin: string;

	constructor(
		token: () => Promise<string | undefined>,
		send: Send = sendToThisOrigin,
		origin: string = location.origin
	) {
		this.#token = token;
		this.#send = send;
		this.#origin = origin;
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
		const token = await this.#token();
		if (!token) return undefined;
		const query = new URLSearchParams({ limit: String(PAGE) });
		if (after) query.set('after', after);
		const answer = await this.#send(request('GET', `/v1/flights?${query}`), token);
		const page = publicFlights(answer.status, answer.body);
		const seen = visibilities(answer.body);
		return {
			flights: page.flights.map((row) => ({
				row,
				sharing: this.#sharing(row, seen.get(row.id))
			})),
			next: page.next
		};
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
		return { row: fetched.row, sharing: fetched.sharing, flight, analysis };
	}

	/**
	 * A flight's file as it was uploaded, with a name to save it under and
	 * what the list says about it. Nothing when this browser is not signed in.
	 */
	async file(id: string): Promise<(OwnFlight & { text: string; name: string }) | undefined> {
		const token = await this.#token();
		if (!token) return undefined;
		const about = await this.#send(request('GET', publicFlightPath(id)), token);
		const row = publicFlight(about.status, about.body);
		const sharing = this.#sharing(row, visibilityOf(about.body));
		const file = await this.#send(request('GET', row.filePath), token);
		// Not the file: the core puts what the server said into words, by throwing.
		if (file.status < 200 || file.status > 299) publicFlight(file.status, file.body);
		return { row, sharing, text: flightFileText(file.body), name: fileName(row) };
	}

	/** Brings a flight file in: an upload to the account. Throws in the core's words when refused. */
	async upload(bytes: ArrayBuffer): Promise<Uploaded | undefined> {
		const token = await this.#token();
		if (!token) return undefined;
		const answer = await this.#send(
			{
				...request('POST', '/v1/flights'),
				body: new Uint8Array(bytes),
				contentType: 'application/octet-stream'
			},
			token
		);
		const row = publicFlight(answer.status, answer.body);
		return {
			flight: { row, sharing: this.#sharing(row, visibilityOf(answer.body)) },
			alreadyThere: answer.status === 200
		};
	}

	/** Changes who may see a flight, and says how it is shared now. */
	async share(id: string, visibility: FlightVisibility): Promise<FlightSharing> {
		const token = await this.#token();
		const answer = await this.#send(
			{
				...request('PATCH', publicFlightPath(id)),
				body: new TextEncoder().encode(JSON.stringify({ visibility })),
				contentType: 'application/json'
			},
			token
		);
		const row = publicFlight(answer.status, answer.body);
		return this.#sharing(row, visibilityOf(answer.body));
	}

	/** Deletes a flight of the account, at once and for good. */
	async delete(id: string): Promise<void> {
		const token = await this.#token();
		const answer = await this.#send(request('DELETE', publicFlightPath(id)), token);
		// Anything but done: the core puts what the server said into words.
		if (answer.status < 200 || answer.status > 299) publicFlight(answer.status, answer.body);
	}

	#sharing(row: PublicFlight, visibility: FlightVisibility | undefined): FlightSharing {
		return {
			visibility: visibility ?? 'private',
			link: new URL(row.pagePath, this.#origin).href,
			flightId: row.id
		};
	}
}

function request(method: string, path: string): ServerRequest {
	return { method, path, body: undefined, contentType: undefined, needsToken: true };
}

/** The name the server gives a flight's file: `venti-<date>-<start of id>.igc`. */
function fileName(row: PublicFlight): string {
	const day = new Date(row.startUnixS * 1000).toISOString().slice(0, 10);
	return `venti-${day}-${row.id.slice(0, 8)}.igc`;
}

const VISIBILITIES: readonly string[] = ['private', 'unlisted', 'public'];

/** Who may see each flight of a list, by its address: the field the core's row leaves out. */
function visibilities(body: Uint8Array): Map<string, FlightVisibility> {
	const found = new Map<string, FlightVisibility>();
	try {
		const parsed = JSON.parse(new TextDecoder().decode(body)) as {
			flights?: { id?: unknown; visibility?: unknown }[];
		};
		for (const flight of parsed.flights ?? []) {
			if (typeof flight.id === 'string' && VISIBILITIES.includes(String(flight.visibility))) {
				found.set(flight.id, flight.visibility as FlightVisibility);
			}
		}
	} catch {
		// The core read the answer; without this field the flight counts as private.
	}
	return found;
}

/** Who may see the one flight of an answer. */
function visibilityOf(body: Uint8Array): FlightVisibility | undefined {
	try {
		const parsed = JSON.parse(new TextDecoder().decode(body)) as {
			flight?: { visibility?: unknown };
		};
		const visibility = String(parsed.flight?.visibility);
		return VISIBILITIES.includes(visibility) ? (visibility as FlightVisibility) : undefined;
	} catch {
		return undefined;
	}
}
