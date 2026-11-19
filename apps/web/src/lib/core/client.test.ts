import { beforeAll, describe, expect, it } from 'vitest';
import { type Channel, CoreClient } from './client';
import { flightFileText } from './flight-text';
import { answer, describeFailure, type Reply, type Request } from './protocol';
import { CoreSession } from './session';
import { MemoryStore } from './store';
import { sampleFlight, startCore } from './testing';
import { WindSource } from './wind-source';

beforeAll(startCore);

/**
 * The worker's part without a worker: requests go straight to a core in
 * the same process, and replies come back the way a worker's would, after
 * the call has returned.
 */
class SameProcess implements Channel {
	onmessage: ((event: MessageEvent<Reply>) => void) | null = null;
	onerror: ((event: ErrorEvent) => void) | null = null;
	/** What was handed over with each request instead of copied. */
	transferred: Transferable[] = [];
	#session: Promise<CoreSession>;
	#wind: WindSource;

	constructor(session: Promise<CoreSession>, wind = new WindSource()) {
		this.#session = session;
		this.#wind = wind;
	}

	postMessage({ id, call, given }: Request, transfer: Transferable[] = []): void {
		this.transferred.push(...transfer);
		void this.#session
			.then((session) => answer({ session, wind: this.#wind }, call, given))
			.then(
				(result): Reply => ({ id, answer: result }),
				(error): Reply => ({ id, failure: describeFailure(error) })
			)
			.then((reply) => this.onmessage?.({ data: reply } as MessageEvent<Reply>));
	}

	/** What the worker says when another tab has changed the logbook. */
	changeElsewhere(): void {
		this.onmessage?.({ data: { changedElsewhere: true } } as MessageEvent<Reply>);
	}
}

async function open() {
	const channel = new SameProcess(CoreSession.open(new MemoryStore()));
	return { channel, core: await CoreClient.open(channel) };
}

const bytes = (text: string) => new TextEncoder().encode(text).buffer as ArrayBuffer;

describe('the core as the page calls it', () => {
	it('says how it started', async () => {
		const { core } = await open();

		expect(core.durable).toBe(false);
		expect(core.version).toMatch(/^\d+\.\d+\.\d+$/);
		expect((await core.logbook()).years).toEqual([]);
	});

	it('imports a flight from its bytes, which it takes over instead of copying', async () => {
		const { core, channel } = await open();
		const file = bytes(sampleFlight());

		const imported = await core.importFlight(file);

		expect(imported.alreadyInLogbook).toBe(false);
		expect(channel.transferred).toContain(file);
		expect((await core.logbook()).totals.flights).toBe(1);
		expect((await core.flight(imported.id)).track).toHaveLength(1745);
		expect((await core.flightAnalysis(imported.id)).takeoff?.trackIndex).toBe(0);
		expect((await core.flightFile(imported.id)).name).toBe('2026-07-04-XVE-000-01.igc');
		expect((await core.statistics()).longest?.flightId).toBe(imported.id);

		await core.deleteFlight(imported.id);
		expect((await core.logbook()).years).toEqual([]);
	});

	it('answers each call with its own answer, whatever order they finish in', async () => {
		const { core } = await open();
		const { id } = await core.importFlight(bytes(sampleFlight()));

		const [flight, logbook, file] = await Promise.all([
			core.flight(id),
			core.logbook(),
			core.flightFile(id)
		]);

		expect(flight.id).toBe(id);
		expect(logbook.totals.flights).toBe(1);
		expect(file.igcText).toBe(sampleFlight());
	});

	it('throws the core’s error with its kind and its words', async () => {
		const { core } = await open();

		const refusal = await core.importFlight(bytes('hello')).catch((error: Error) => error);

		expect(refusal).toBeInstanceOf(Error);
		expect((refusal as Error).name).toBe('InvalidFlight');
		expect((refusal as Error).message).toContain('not a flight file');
	});

	it('passes on that another tab changed the logbook', async () => {
		const { core, channel } = await open();
		let told = 0;
		core.onChangedElsewhere = () => (told += 1);

		channel.changeElsewhere();

		expect(told).toBe(1);
	});

	it('stops everyone waiting when the worker breaks down', async () => {
		// A worker that never answers.
		const silent: Channel = { postMessage() {}, onmessage: null, onerror: null };
		const opening = CoreClient.open(silent).catch((error: Error) => error);

		silent.onerror?.({ message: 'The script could not be loaded.' } as ErrorEvent);

		expect(((await opening) as Error).message).toBe('The script could not be loaded.');
	});
});

describe('the text of a flight file', () => {
	it('is read as UTF-8', () => {
		expect(flightFileText(new TextEncoder().encode('HFPLTPILOT:Jörg Müller'))).toBe(
			'HFPLTPILOT:Jörg Müller'
		);
	});

	it('is read as Western European text when it is not UTF-8', () => {
		// "Jörg" as an old recorder writes it: one byte for the ö.
		const latin = Uint8Array.from([0x4a, 0xf6, 0x72, 0x67]);
		expect(flightFileText(latin)).toBe('Jörg');
	});

	it('keeps a name that is not UTF-8 through an import', async () => {
		const { core } = await open();
		const text = sampleFlight().replace('Sample Pilot', 'Jörg');
		const latin = Uint8Array.from(text, (character) => character.charCodeAt(0));

		const { id } = await core.importFlight(latin.buffer as ArrayBuffer);

		expect((await core.flight(id)).pilot).toBe('Jörg');
	});
});
