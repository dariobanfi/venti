import { describe, expect, it } from 'vitest';
import type { PublicFlight } from '#lib/core/index.ts';
import { createFormat } from '#lib/design/format.ts';
import {
	askedIn,
	EVERYTHING,
	type FlightsPage,
	type ListState,
	NOT_LISTED,
	PagedFlights,
	queryOf,
	span,
	withDay,
	words
} from './pages';
import { detail, distance, nothingListed, shape } from './words';

/** A public flight; `given` replaces what a test cares about. */
function flight(given: Partial<PublicFlight> = {}): PublicFlight {
	return {
		id: '5t1eshypdtvrq7rxw39a18vsrg',
		handle: 'demo',
		pilot: 'Demo Pilot',
		// 15 August 2026, 10:30 UTC.
		startUnixS: 1_786_789_800,
		airtimeS: 1752,
		takeoff: { lat: 47.6642, lon: 11.7967 },
		takeoffName: undefined,
		takeoffSiteId: undefined,
		takeoffCountry: undefined,
		pilotCountry: undefined,
		pilotPicture: undefined,
		gliderClass: undefined,
		trackLengthM: 21_340,
		maxAltitudeM: 2117,
		maxClimbMs: 2.4,
		glider: 'Sample Wing',
		score: { kind: 'freeFlight', closed: false, distanceM: 11_975.7, points: 11.98 },
		outline: [
			{ lat: 47.6642, lon: 11.7967 },
			{ lat: 47.6697, lon: 11.7899 }
		],
		pagePath: '/f/5t1eshypdtvrq7rxw39a18vsrg',
		filePath: '/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/igc',
		...given
	};
}

/** A list over a server the test plays. */
function listing(reply: (after: string | undefined) => Promise<FlightsPage | undefined>) {
	const server = { reply, asked: [] as (string | undefined)[] };
	const told: ListState[] = [];
	const list = new PagedFlights(
		(after) => {
			server.asked.push(after);
			return server.reply(after);
		},
		(state) => told.push(state)
	);
	return { list, server, told, state: () => told.at(-1) ?? NOT_LISTED };
}

const unreachable = () => {
	const error = new Error('Venti could not be reached for other pilots’ flights.');
	error.name = 'Unreachable';
	return Promise.reject(error);
};

describe('a list of other pilots’ flights', () => {
	it('is read from its top, and says while it is being fetched', async () => {
		const { list, server, told, state } = listing(async () => ({
			flights: [flight()],
			next: undefined
		}));

		await list.load();

		expect(server.asked).toEqual([undefined]);
		expect(told[0]).toMatchObject({ known: false, loading: true });
		expect(state()).toEqual({
			known: true,
			flights: [flight()],
			more: false,
			loading: false,
			problem: undefined,
			absent: false
		});
	});

	it('goes on a page at a time, from where the page before ended', async () => {
		const pages: Record<string, FlightsPage> = {
			top: { flights: [flight({ id: 'a' }), flight({ id: 'b' })], next: 'after-b' },
			'after-b': { flights: [flight({ id: 'c' })], next: undefined }
		};
		const { list, server, state } = listing(async (after) => pages[after ?? 'top']);

		await list.load();
		expect(state()).toMatchObject({ more: true });
		await list.more();

		expect(server.asked).toEqual([undefined, 'after-b']);
		expect(state().flights.map((flight) => flight.id)).toEqual(['a', 'b', 'c']);
		expect(state().more).toBe(false);
		// The last page has been read: there is nothing more to ask for.
		await list.more();
		expect(server.asked).toHaveLength(2);
	});

	it('asks one question at a time, so that no page is listed twice', async () => {
		let arrive: (page: FlightsPage) => void = () => {};
		const { list, server, state } = listing(
			(after) =>
				new Promise((resolve) => {
					arrive = resolve;
					if (after === undefined) resolve({ flights: [flight({ id: 'a' })], next: 'after-a' });
				})
		);
		await list.load();

		const first = list.more();
		// The button is pressed again before the page is there.
		const second = list.more();
		arrive({ flights: [flight({ id: 'b' })], next: undefined });
		await Promise.all([first, second]);

		expect(server.asked).toEqual([undefined, 'after-a']);
		expect(state().flights.map((flight) => flight.id)).toEqual(['a', 'b']);
	});

	it('keeps what is listed when a further page cannot be fetched, and can ask for it again', async () => {
		const { list, server, state } = listing(async () => ({
			flights: [flight({ id: 'a' })],
			next: 'after-a'
		}));
		await list.load();

		server.reply = unreachable;
		await list.more();
		expect(state()).toMatchObject({
			known: true,
			more: true,
			loading: false,
			problem: 'Venti could not be reached for other pilots’ flights.'
		});
		expect(state().flights).toHaveLength(1);

		server.reply = async () => ({ flights: [flight({ id: 'b' })], next: undefined });
		await list.more();
		expect(state().problem).toBeUndefined();
		expect(state().flights.map((flight) => flight.id)).toEqual(['a', 'b']);
	});

	it('says why a list could not be fetched at all, and is read again from its top', async () => {
		const { list, server, state } = listing(unreachable);
		await list.load();
		expect(state()).toMatchObject({ known: false, flights: [], loading: false });
		expect(state().problem).toContain('could not be reached');

		server.reply = async () => ({ flights: [flight()], next: undefined });
		await list.load();
		expect(state()).toMatchObject({ known: true, problem: undefined });
	});

	it('has nothing on a site without a server', async () => {
		const { list, state } = listing(async () => undefined);
		await list.load();

		expect(state()).toEqual({ ...NOT_LISTED, absent: true });
	});

	it('puts words before what is not written for a pilot', () => {
		const core = new Error('The server could not answer (status 502).');
		core.name = 'Server';
		expect(words(core)).toBe('The server could not answer (status 502).');
		expect(words(new TypeError('x is not a function'))).toBe(
			'Other pilots’ flights could not be fetched. x is not a function'
		);
	});
});

describe('the days a pilot asks for', () => {
	/** Midnight by a clock two hours ahead of UTC, as in Germany in summer. */
	const inSummer = (year: number, month: number, day: number) =>
		Date.UTC(year, month - 1, day) / 1000 - 2 * 3600;

	it('are a span from the first second of the first to the first second after the last', () => {
		expect(span({ from: '2026-08-15', to: '2026-08-15' }, inSummer)).toEqual({
			fromUnixS: Date.UTC(2026, 7, 14, 22) / 1000,
			toUnixS: Date.UTC(2026, 7, 15, 22) / 1000
		});
		expect(span({ from: '2026-08-15', to: '2026-08-16' }, inSummer)).toEqual({
			fromUnixS: Date.UTC(2026, 7, 14, 22) / 1000,
			toUnixS: Date.UTC(2026, 7, 16, 22) / 1000
		});
		// The last day of a month ends where the next month begins.
		expect(span({ from: '', to: '2026-08-31' }, inSummer).toUnixS).toBe(
			Date.UTC(2026, 7, 31, 22) / 1000
		);
	});

	it('are no bound where a field is empty or holds something else', () => {
		expect(span({ from: '', to: '' })).toEqual({ fromUnixS: undefined, toUnixS: undefined });
		expect(span({ from: 'yesterday', to: '2026-8-5' })).toEqual({
			fromUnixS: undefined,
			toUnixS: undefined
		});
		expect(span({ from: '2026-08-15', to: '' }, inSummer)).toEqual({
			fromUnixS: Date.UTC(2026, 7, 14, 22) / 1000,
			toUnixS: undefined
		});
	});

	it('are days by the reader’s own clock, also on the day it is put back', () => {
		// 25 October 2026 has 25 hours in Berlin. Whatever zone this test
		// runs in, a day begins when the one before has ended.
		const day = span({ from: '2026-10-25', to: '2026-10-25' });
		const next = span({ from: '2026-10-26', to: '2026-10-26' });
		expect(day.toUnixS).toBe(next.fromUnixS);
		expect(day.fromUnixS).toBe(new Date(2026, 9, 25).getTime() / 1000);
	});

	it('mean one day when only the first is chosen, and stay in order', () => {
		const any = { ...EVERYTHING };
		// One click chooses a day.
		expect(withDay(any, 'from', '2026-08-15')).toEqual({ from: '2026-08-15', to: '2026-08-15' });
		// The last day is moved on: a span.
		const span = { ...any, from: '2026-08-15', to: '2026-08-16' };
		expect(withDay({ ...any, from: '2026-08-15', to: '2026-08-15' }, 'to', '2026-08-16')).toEqual({
			from: '2026-08-15',
			to: '2026-08-16'
		});
		// A first day inside the span leaves its end; one past it takes the end along.
		expect(withDay(span, 'from', '2026-08-16')).toEqual({ from: '2026-08-16', to: '2026-08-16' });
		expect(withDay(span, 'from', '2026-08-20')).toEqual({ from: '2026-08-20', to: '2026-08-20' });
		expect(withDay(span, 'from', '2026-08-01')).toEqual({ from: '2026-08-01', to: '2026-08-16' });
		// A last day before the first moves the first.
		expect(withDay(span, 'to', '2026-08-10')).toEqual({ from: '2026-08-10', to: '2026-08-10' });
		// Emptying a field removes that bound and leaves the other.
		expect(withDay(span, 'from', '')).toEqual({ from: '', to: '2026-08-16' });
		expect(withDay(span, 'to', '')).toEqual({ from: '2026-08-15', to: '' });
		// "Up to this day" is a question too.
		expect(withDay(any, 'to', '2026-08-16')).toEqual({ from: '', to: '2026-08-16' });
	});

	it('are written into the page’s address and read from it, and nothing else is', () => {
		expect(queryOf(EVERYTHING)).toBe('');
		const asked = { order: 'best', from: '2026-08-15', to: '2026-08-16' } as const;
		expect(queryOf(asked)).toBe('?order=best&from=2026-08-15&to=2026-08-16');
		expect(askedIn(new URLSearchParams(queryOf(asked)))).toEqual(asked);
		expect(askedIn(new URLSearchParams('order=worst&from=<b>&to=2026-13-99x'))).toEqual(EVERYTHING);
	});
});

describe('a row of the list', () => {
	const text = createFormat('en-GB', 'Europe/Berlin');

	it('says when the flight was and with what', () => {
		expect(detail(flight(), text)).toBe('Sat, 15 Aug 2026 · Sample Wing');
		expect(detail(flight({ glider: undefined }), text)).toBe('Sat, 15 Aug 2026');
	});

	it('has the scored distance, or the track’s length for a flight too short to score', () => {
		expect(distance(flight(), text)).toBe('12.0');
		expect(distance(flight({ score: undefined }), text)).toBe('21.3');
	});

	it('marks a triangle, and says which kind', () => {
		const scored = (kind: 'freeFlight' | 'freeTriangle' | 'faiTriangle') =>
			flight({ score: { kind, closed: true, distanceM: 30_000, points: 42 } });
		expect(shape(scored('freeFlight'))).toBeUndefined();
		expect(shape(scored('freeTriangle'))).toBe('Triangle');
		expect(shape(scored('faiTriangle'))).toBe('FAI triangle');
		expect(shape(flight({ score: undefined }))).toBeUndefined();
	});

	it('says why a list is empty', () => {
		const known = { ...NOT_LISTED, known: true };
		expect(nothingListed(known, { from: '', to: '' }).title).toBe('No public flights yet');
		expect(nothingListed(known, { from: '2026-08-15', to: '2026-08-15' }).title).toBe(
			'No public flights on these days'
		);
		expect(nothingListed({ ...NOT_LISTED, absent: true }, { from: '', to: '' }).title).toBe(
			'Not connected to Venti'
		);
		expect(
			nothingListed({ ...NOT_LISTED, problem: 'The server is busy.' }, { from: '', to: '' })
		).toEqual({ title: 'The flights could not be listed', says: 'The server is busy.' });
	});
});
