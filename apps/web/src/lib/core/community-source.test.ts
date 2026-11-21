import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { CommunitySource } from './community-source';
import { sampleFlight, startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answers, which its tests keep current: the
 * public flights of its development mode, a pilot's page and one flight.
 */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const bytes = (text: string) => new TextEncoder().encode(text);
const json = (name: string) => JSON.parse(new TextDecoder().decode(recorded(name)));
const refusal = (code: string): Answer => ({
	status: 404,
	body: bytes(JSON.stringify({ error: { code, message: 'for developers' } }))
});

/** A server that answers as it is told to and notes what it was asked. */
function server(answer: (path: string) => Answer) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		return answer(path);
	};
	return { asked, get };
}

/** The address of the flight in `flight.json`. */
const FLIGHT = json('flight.json').flight.id as string;

describe('everyone’s public flights, from the server', () => {
	it('are asked for, near a place, by the square around it', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('public_flights.json') }));
		const community = new CommunitySource(async () => true, get);

		await community.flights({ order: 'best', near: { lat: 47.6642, lon: 11.7967 } });

		const query = new URL(asked[0], 'http://venti').searchParams;
		expect(query.get('sort')).toBe('score');
		const [west, south, east, north] = query.get('bbox')!.split(',').map(Number);
		expect(south).toBeLessThan(47.6642);
		expect(north).toBeGreaterThan(47.6642);
		// Degrees of longitude are shorter there: the square is wider in them.
		expect(east - west).toBeGreaterThan((north - south) * 1.4);
	});

	it('are asked for newest first, and read as rows for a list', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('public_flights.json') }));
		const community = new CommunitySource(async () => true, get);

		const page = (await community.flights({ order: 'newest' }))!;

		expect(asked).toEqual(['/v1/flights/public']);
		const raw = json('public_flights.json');
		expect(page.flights).toHaveLength(raw.flights.length);
		expect(page.flights[0]).toMatchObject({
			id: raw.flights[0].id,
			handle: raw.flights[0].pilot.handle,
			pilot: raw.flights[0].pilot.display_name,
			airtimeS: raw.flights[0].airtime_s,
			pagePath: `/f/${raw.flights[0].id}`,
			filePath: `/v1/flights/${raw.flights[0].id}/igc`
		});
		// A moment is a number, not a BigInt.
		expect(typeof page.flights[0].startUnixS).toBe('number');
		expect(page.flights[0].outline.length).toBeGreaterThan(1);
		expect(page.flights[0].score).toMatchObject({ kind: 'freeFlight' });
	});

	it('are asked for by points, for a span of days, and from where the page before ended', async () => {
		const { asked, get } = server(() => ({
			status: 200,
			body: recorded('public_flights_best.json')
		}));
		const community = new CommunitySource(async () => true, get);

		// 15 August 2026 as a pilot in Germany means it: from midnight to
		// midnight by a clock two hours ahead of UTC.
		await community.flights({
			order: 'best',
			fromUnixS: Date.UTC(2026, 7, 14, 22) / 1000,
			toUnixS: Date.UTC(2026, 7, 15, 22) / 1000,
			after: 'a mark'
		});

		expect(asked).toEqual([
			'/v1/flights/public?from=2026-08-14T22:00:00Z&to=2026-08-15T22:00:00Z&sort=score&after=a%20mark'
		]);
	});

	it('are not asked for on a site without a server', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('public_flights.json') }));
		const community = new CommunitySource(async () => false, get);

		expect(await community.flights({ order: 'newest' })).toBeUndefined();
		expect(await community.pilot('demo')).toBeUndefined();
		expect(await community.open(FLIGHT)).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('report a server in trouble in the core’s words', async () => {
		const { get } = server(() => ({ status: 502, body: bytes('Bad Gateway') }));
		const community = new CommunitySource(async () => true, get);

		await expect(community.flights({ order: 'newest' })).rejects.toMatchObject({
			name: 'Server',
			message: 'The server could not answer (status 502).'
		});
	});
});

describe('the ranking of a season', () => {
	/** The evening of 18 November 2026, seconds since 1970. */
	const NOW = Date.UTC(2026, 10, 18, 19, 5) / 1000;

	it('is asked for by the season’s span of time and read as places', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		const newest = (await community.ranking(undefined, undefined, NOW))!;

		expect(asked).toEqual(['/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z']);
		expect(newest.seasons.map((season) => season.name)).toEqual([
			'2026',
			'2025',
			'2024',
			'2023',
			'All years'
		]);
		expect(newest.season.name).toBe('2026');
		// A moment is a number, not a BigInt.
		expect(typeof newest.season.fromUnixS).toBe('number');
		expect(
			newest.ranking.pilots.map((pilot) => [pilot.place, pilot.name, pilot.points, pilot.counted])
		).toEqual([
			[1, 'Mara Muster', 23.42, '2 flights'],
			[2, 'Demo Pilot', 11.98, '1 flight']
		]);
		// The flights that count are rows like those of the list.
		const raw = json('ranking.json');
		expect(newest.ranking.pilots[0].flights).toHaveLength(2);
		expect(newest.ranking.pilots[0].flights[0]).toMatchObject({
			id: raw.pilots[0].flights[0].id,
			handle: 'mara',
			pagePath: `/f/${raw.pilots[0].flights[0].id}`
		});
		expect(newest.ranking.rule).toMatch(/best 3 flights shared with everyone/);
	});

	it('is asked for all years without a span, and for the newest season by any other name', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		expect((await community.ranking('All years', undefined, NOW))!.season.name).toBe('All years');
		expect((await community.ranking('2024', undefined, NOW))!.season.name).toBe('2024');
		// What names no season, as an address someone typed might.
		expect((await community.ranking('1999', undefined, NOW))!.season.name).toBe('2026');

		expect(asked).toEqual([
			'/v1/ranking',
			'/v1/ranking?from=2024-01-01T00:00:00Z&to=2025-01-01T00:00:00Z',
			'/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z'
		]);
	});

	it('counts, near a place, only the flights that took off around it', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		await community.ranking('All years', { lat: 47.6642, lon: 11.7967 }, NOW);

		const box = new URL(asked[0], 'http://venti').searchParams.get('bbox')!.split(',').map(Number);
		const [west, south, east, north] = box;
		// A kilometre and a half to each side.
		expect((north - south) * 111_320).toBeCloseTo(3000, 0);
		expect(west).toBeLessThan(11.7967);
		expect(east).toBeGreaterThan(11.7967);
	});

	it('offers the months of its year that have a ranking, as the core names them', async () => {
		const { get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		const newest = (await community.ranking(undefined, undefined, NOW))!;

		expect(newest.months.map((month) => [month.name, month.month])).toEqual([
			['September', '2026-09'],
			['August', '2026-08'],
			['July', '2026-07'],
			['June', '2026-06'],
			['May', '2026-05'],
			['April', '2026-04']
		]);
		expect(newest.month).toBeUndefined();
		expect(newest.words).toBe('2026: 10 flights');
		// All years have no months to choose from.
		const all = (await community.ranking('All years', undefined, NOW))!;
		expect(all.months).toEqual([]);
		expect(all.words).toBe('');
	});

	it('of a month is asked for by the month, and says its year and its words', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		const july = (await community.ranking(undefined, undefined, NOW, '2025-07'))!;

		expect(asked).toEqual(['/v1/ranking?month=2025-07']);
		expect(july.season.name).toBe('2025');
		expect(july.month).toMatchObject({ name: 'July', title: 'July 2025', month: '2025-07' });
		expect(july.months.map((month) => month.month)).toEqual(['2025-07']);
		expect(july.words).toBe('July 2025: 1 flight');

		// Near a place, of the flights from there.
		await community.ranking(undefined, { lat: 47.6642, lon: 11.7967 }, NOW, '2026-05');
		const query = new URL(asked[1], 'http://venti').searchParams;
		expect(query.get('month')).toBe('2026-05');
		expect(query.get('bbox')).not.toBeNull();
	});

	it('of something that is no month is refused in the core’s words, and nothing is asked', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => true, get);

		await expect(community.ranking(undefined, undefined, NOW, 'August')).rejects.toThrow();
		expect(asked).toEqual([]);
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('ranking.json') }));
		const community = new CommunitySource(async () => false, get);

		expect(await community.ranking(undefined, undefined, NOW)).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('reports a server in trouble in the core’s words', async () => {
		const { get } = server(() => ({ status: 503, body: bytes('') }));
		const community = new CommunitySource(async () => true, get);

		await expect(community.ranking(undefined, undefined, NOW)).rejects.toThrow(/server/i);
	});
});

describe('one pilot’s public flights', () => {
	it('are asked for by handle and read like the list, with the pilot’s name', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilot.json') }));
		const community = new CommunitySource(async () => true, get);

		const pilot = (await community.pilot('demo', 'a mark'))!;

		expect(asked).toEqual(['/v1/pilots/demo?after=a%20mark']);
		expect(pilot).toMatchObject({ handle: 'demo', name: 'Demo Pilot' });
		expect(pilot.flights.length).toBeGreaterThan(0);
		expect(pilot.flights.every((flight) => flight.handle === 'demo')).toBe(true);
	});

	it('say in the core’s words that the server knows no pilot of that handle', async () => {
		const { get } = server(() => refusal('pilot_not_found'));
		const community = new CommunitySource(async () => true, get);

		await expect(community.pilot('nobody')).rejects.toMatchObject({
			name: 'Server',
			message: 'The server knows no pilot by this handle.'
		});
	});

	it('are never asked for with text that is no handle', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilot.json') }));
		const community = new CommunitySource(async () => true, get);

		await expect(community.pilot('../account')).rejects.toMatchObject({
			name: 'Server',
			message: 'This is not the handle of a pilot.'
		});
		expect(asked).toEqual([]);
	});
});

describe('another pilot’s flight', () => {
	/** A server with one public flight, whose file is the sample flight. */
	const withFlight = (file: Answer = { status: 200, body: bytes(sampleFlight()) }) =>
		server((path) =>
			path.endsWith('/igc') ? file : { status: 200, body: recorded('flight.json') }
		);

	it('is fetched by its address, then its file, and shown like one of the logbook', async () => {
		const { asked, get } = withFlight();
		const community = new CommunitySource(async () => true, get);

		const opened = (await community.open(FLIGHT))!;

		expect(asked).toEqual([`/v1/flights/${FLIGHT}`, `/v1/flights/${FLIGHT}/igc`]);
		// Whose it is and where its page is come from the server's row.
		expect(opened.row).toMatchObject({ id: FLIGHT, handle: 'demo', pagePath: `/f/${FLIGHT}` });
		// The flight itself is the file's, read by the code that reads the
		// logbook's. It is in no logbook: it has no identifier there.
		expect(opened.flight.id).toBe('');
		expect(opened.flight.track.length).toBeGreaterThan(1000);
		expect(opened.flight.glider).toBe('Sample Wing');
		expect(opened.analysis?.thermals).toHaveLength(2);
	});

	it('is said to be no longer public when the server has none for the asker', async () => {
		const gone = server(() => refusal('flight_not_found'));
		await expect(
			new CommunitySource(async () => true, gone.get).open(FLIGHT)
		).rejects.toMatchObject({
			name: 'Server',
			message: 'This flight is no longer public: its pilot took it back, or deleted it.'
		});

		// Taken back between the two requests: the file is refused the same way.
		const taken = withFlight(refusal('flight_not_found'));
		await expect(
			new CommunitySource(async () => true, taken.get).open(FLIGHT)
		).rejects.toMatchObject({
			message: 'This flight is no longer public: its pilot took it back, or deleted it.'
		});
	});

	it('is not shown when what the server hands out is no flight file', async () => {
		const { get } = withFlight({ status: 200, body: bytes('<html>an error page</html>') });
		const community = new CommunitySource(async () => true, get);

		await expect(community.open(FLIGHT)).rejects.toMatchObject({ name: 'InvalidFlight' });
	});

	it('is never asked for with text that is no address', async () => {
		const { asked, get } = withFlight();
		const community = new CommunitySource(async () => true, get);

		await expect(community.open('public?limit=100')).rejects.toMatchObject({
			name: 'Server',
			message: 'This is not the address of a flight.'
		});
		expect(asked).toEqual([]);
	});
});

describe('pilots, found by name', () => {
	it('are asked for by the text and read as the core’s lines', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilots.json') }));
		const community = new CommunitySource(async () => true, get);

		const found = (await community.pilots(' le '))!;

		expect(asked).toEqual(['/v1/pilots?q=le']);
		expect(found.words).toBe('1 pilot');
		expect(found.pilots).toEqual([
			{
				handle: 'lena',
				name: 'Lena Probe',
				flights: 3,
				words: 'Lena Probe (@lena), 3 public flights'
			}
		]);
	});

	it('say in the core’s line that nobody was found', async () => {
		const { get } = server(() => ({ status: 200, body: bytes('{"pilots":[]}') }));
		const community = new CommunitySource(async () => true, get);

		const found = (await community.pilots('xyz'))!;

		expect(found.pilots).toEqual([]);
		expect(found.words).toBe('No pilot by this name has shared a flight with everyone.');
	});

	it('are not asked for by fewer than two letters: the core says why', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilots.json') }));
		const community = new CommunitySource(async () => true, get);

		await expect(community.pilots('l')).rejects.toThrow(/at least two letters/);
		expect(asked).toEqual([]);
	});

	it('are not asked for on a site without a server', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilots.json') }));
		const community = new CommunitySource(async () => false, get);

		expect(await community.pilots('le')).toBeUndefined();
		expect(asked).toEqual([]);
	});
});

describe('what a pilot’s public flights add up to', () => {
	it('is asked for by the handle and read as the core’s rows', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilot_statistics.json') }));
		const community = new CommunitySource(async () => true, get);

		const statistics = (await community.statistics('lena'))!;

		expect(asked).toEqual(['/v1/pilots/lena/statistics']);
		expect(statistics.name).toBe('Lena Probe');
		expect(statistics.overall.name).toBe('All years');
		expect(statistics.overall.rows.map((row) => [row.label, row.value])).toEqual([
			['Flights', '3'],
			['Distance scored', '28.2 km'],
			['Points, best 3 flights', '28.19'],
			['Airtime', '1 h 04 min'],
			['Longest flight', '12.0 km'],
			['Highest point', '2117 m']
		]);
		// A row about one flight names it, for a link; a sum names none.
		const raw = json('pilot_statistics.json');
		expect(statistics.overall.rows[4].flightId).toBe(raw.overall.longest.id);
		expect(statistics.overall.rows[0].flightId).toBe('');
		expect(statistics.overall.sites.map((site) => site.words)).toEqual([
			'Musterkogel West, 2 flights'
		]);
		expect(statistics.overall.months).toHaveLength(12);
		expect(statistics.overall.months[4]).toMatchObject({ name: 'May', flights: 1, share: 1 });
		expect(statistics.seasons.map((season) => [season.name, season.year])).toEqual([
			['2026', 2026]
		]);
		expect(statistics.note).toMatch(/shared with everyone/);
		expect(statistics.terms.text).toMatch(/their pilots/);
	});

	it('of a pilot the server does not know is refused in the core’s words', async () => {
		const { get } = server(() => refusal('not_found'));
		const community = new CommunitySource(async () => true, get);

		await expect(community.statistics('nobody')).rejects.toThrow();
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = server(() => ({ status: 200, body: recorded('pilot_statistics.json') }));
		const community = new CommunitySource(async () => false, get);

		expect(await community.statistics('lena')).toBeUndefined();
		expect(asked).toEqual([]);
	});
});
