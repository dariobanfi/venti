import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { type LiveFetched, type LiveGet, LiveSource } from './live-source';
import { startCore } from './testing';

beforeAll(startCore);

/**
 * The server's own recorded answers, which its tests keep current: the two
 * demo gliders over the Tegernsee on 18 November 2026 at 19:05 UTC, and the
 * end of the track of the one that is flying.
 */
function recorded(name: string): Uint8Array {
	return readFileSync(
		new URL(`../../../../../crates/server/tests/golden/${name}`, import.meta.url)
	);
}

const bytes = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));
const json = (name: string) => JSON.parse(new TextDecoder().decode(recorded(name)));

/** The valley of the sample flight. */
const VALLEY = { south: 47.6, west: 11.7, north: 47.75, east: 11.95 };

const FLYING = '11111111111111111111111111';
const LANDED = '22222222222222222222222222';

/** A server that answers as it is told to and notes what it was asked, validator included. */
function server(answer: (path: string, etag: string | undefined) => LiveFetched) {
	const asked: { path: string; etag: string | undefined }[] = [];
	const get: LiveGet = async (path, etag) => {
		asked.push({ path, etag });
		return answer(path, etag);
	};
	return { asked, get };
}

/**
 * A server like the real one: it answers in full, with a validator, and
 * "not modified" to whoever names that validator.
 */
function revalidating(body: () => Uint8Array, etag = 'W/"first"') {
	return server((_path, named) =>
		named === etag
			? { status: 304, body: new Uint8Array(), etag }
			: { status: 200, body: body(), etag }
	);
}

describe('who is in the air, from the server', () => {
	it('is asked for by the box of the map, and read with each age by the server’s clock', async () => {
		const { asked, get } = revalidating(() => recorded('live.json'));
		const live = new LiveSource(async () => true, get);

		const answer = (await live.gliders(VALLEY))!;

		expect(asked).toEqual([{ path: '/v1/live?bbox=11.7,47.6,11.95,47.75', etag: undefined }]);
		expect(answer.map.truncated).toBe(false);
		expect(answer.map.gliders).toHaveLength(2);
		expect(answer.map.gliders[0]).toMatchObject({
			id: FLYING,
			handle: 'demo',
			name: 'Demo Pilot',
			altitudeM: 1596,
			groundSpeedMs: 10.5,
			climbMs: -1,
			courseDeg: 20,
			landed: false,
			// The position is of 19:04:56 and the server answered at 19:05:00.
			ageS: 4,
			quiet: false,
			label: 'Demo Pilot · 1596 m',
			pagePath: `/live/${FLYING}`
		});
		// One that has landed stays listed for half an hour, and points nowhere.
		expect(answer.map.gliders[1]).toMatchObject({ id: LANDED, landed: true, ageS: 540 });
		expect(answer.map.gliders[1].courseDeg).toBeUndefined();
	});

	it('names the validator of its last answer, and keeps that answer while the server has nothing new', async () => {
		const { asked, get } = revalidating(() => recorded('live.json'));
		const live = new LiveSource(async () => true, get);

		const first = (await live.gliders(VALLEY))!;
		const again = (await live.gliders(VALLEY))!;

		expect(asked[1]).toEqual({ path: '/v1/live?bbox=11.7,47.6,11.95,47.75', etag: 'W/"first"' });
		// "Not modified" renews nothing: it is the same answer, by its number,
		// and the ages in it are still those of when it first came.
		expect(again.heard).toBe(first.heard);
		expect(again.map).toEqual(first.map);
	});

	it('counts a new answer when the server gives one in full', async () => {
		let etag = 'W/"first"';
		const { get } = server((_path, named) =>
			named === etag
				? { status: 304, body: new Uint8Array(), etag }
				: { status: 200, body: recorded('live.json'), etag }
		);
		const live = new LiveSource(async () => true, get);

		const first = (await live.gliders(VALLEY))!;
		// A glider has moved: the server's validator is another.
		etag = 'W/"second"';
		const second = (await live.gliders(VALLEY))!;

		expect(second.heard).toBeGreaterThan(first.heard);
	});

	it('names no validator for another part of the map', async () => {
		const { asked, get } = revalidating(() => recorded('live.json'));
		const live = new LiveSource(async () => true, get);

		await live.gliders(VALLEY);
		await live.gliders({ ...VALLEY, east: 12.1 });

		expect(asked[1]).toEqual({ path: '/v1/live?bbox=11.7,47.6,12.1,47.75', etag: undefined });
	});

	it('is not asked for on a site without a server', async () => {
		const { asked, get } = revalidating(() => recorded('live.json'));
		const live = new LiveSource(async () => false, get);

		expect(await live.gliders(VALLEY)).toBeUndefined();
		expect(await live.follow(FLYING)).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('reports a server in trouble in the core’s words', async () => {
		const { get } = server(() => ({
			status: 502,
			body: new TextEncoder().encode('Bad Gateway'),
			etag: undefined
		}));
		const live = new LiveSource(async () => true, get);

		await expect(live.gliders(VALLEY)).rejects.toMatchObject({
			name: 'Server',
			message: 'The server could not answer (status 502).'
		});
	});

	it('says how old a position is in the core’s words, and never calls silence a landing', () => {
		const live = new LiveSource();

		expect(
			live.ages([
				{ ageS: 4, landed: false },
				{ ageS: 45, landed: false },
				{ ageS: 420, landed: false },
				{ ageS: 540, landed: true }
			])
		).toEqual(['Now', '45 s ago', 'No position for 7 min', 'Landed 9 min ago']);
	});
});

describe('one flight, followed', () => {
	/** The recorded answer about the flying glider's track, with other fixes or another cursor. */
	const track = (changes: Record<string, unknown> = {}) =>
		bytes({ ...json('live_track.json'), ...changes });

	it('is asked for whole, then for what arrived since, then with the validator of that', async () => {
		const { asked, get } = server((path, named) => {
			if (!path.includes('since=')) return { status: 200, body: track(), etag: 'W/"whole"' };
			return named === 'W/"since"'
				? { status: 304, body: new Uint8Array(), etag: 'W/"since"' }
				: { status: 200, body: track({ fixes: [] }), etag: 'W/"since"' };
		});
		const live = new LiveSource(async () => true, get);

		const first = await live.follow(FLYING);
		const second = await live.follow(FLYING);
		const third = await live.follow(FLYING);

		expect(asked).toEqual([
			{ path: `/v1/live/${FLYING}`, etag: undefined },
			// Another address than the first: there is no answer to it yet.
			{ path: `/v1/live/${FLYING}?since=1201`, etag: undefined },
			{ path: `/v1/live/${FLYING}?since=1201`, etag: 'W/"since"' }
		]);
		if (!first || 'gone' in first || !second || 'gone' in second || !third || 'gone' in third) {
			throw new Error('the flight was not followed');
		}
		// The three positions the recorded answer holds, as a line with its climb.
		expect(first.view.glider).toMatchObject({ id: FLYING, altitudeM: 1596, ageS: 4 });
		expect(first.view.track).toHaveLength(3);
		expect(first.view.track[2]).toEqual({
			lat: 47.663773253525605,
			lon: 11.743167672151715,
			climbMs: -1
		});
		expect(first.view.highestM).toBe(1598);
		expect(first.view.durationS).toBe(2);
		// 18 November 2026, 18:44:56 UTC: a number, not a BigInt.
		expect(first.view.startedUnixS).toBe(1_795_027_496);
		expect(first.view.more).toBe(false);
		// Nothing new arrived: the track is as it was, in an answer of its own.
		expect(second.view.track).toEqual(first.view.track);
		expect(second.heard).toBeGreaterThan(first.heard);
		// "Not modified": the same answer once more.
		expect(third.heard).toBe(second.heard);
		expect(third.view).toEqual(second.view);
	});

	it('says in the core’s words that a track is gone, and does not throw it', async () => {
		const { get } = server(() => ({ status: 404, body: bytes({}), etag: undefined }));
		const live = new LiveSource(async () => true, get);

		expect(await live.follow(FLYING)).toEqual({
			gone: 'This flight is no longer shown: its pilot stopped sharing it, or it is more than two days old.'
		});
	});

	it('throws what is wrong with the server, to be asked again later', async () => {
		const { get } = server(() => ({ status: 503, body: bytes({}), etag: undefined }));
		const live = new LiveSource(async () => true, get);

		await expect(live.follow(FLYING)).rejects.toMatchObject({ name: 'Server' });
	});

	it('refuses text that is not the identifier of a track, in words', async () => {
		const { asked, get } = server(() => ({ status: 200, body: track(), etag: undefined }));
		const live = new LiveSource(async () => true, get);

		await expect(live.follow('../v1/account')).rejects.toMatchObject({
			name: 'Server',
			message: 'This is not the address of a live track.'
		});
		expect(asked).toEqual([]);
	});

	it('starts again from the whole track when another flight was followed in between', async () => {
		const { asked, get } = server((path) => ({
			status: 200,
			body: path.includes(LANDED)
				? track({ track: { ...json('live_track.json').track, id: LANDED } })
				: track(),
			etag: undefined
		}));
		const live = new LiveSource(async () => true, get);

		await live.follow(FLYING);
		await live.follow(LANDED);
		await live.follow(FLYING);

		expect(asked.map((request) => request.path)).toEqual([
			`/v1/live/${FLYING}`,
			`/v1/live/${LANDED}`,
			`/v1/live/${FLYING}`
		]);
	});

	it('has no answer for a flight the page let go of while the server was asked', async () => {
		let arrive: (fetched: LiveFetched) => void = () => {};
		const get: LiveGet = () => new Promise((resolve) => (arrive = resolve));
		const live = new LiveSource(async () => true, get);

		const asked = live.follow(FLYING);
		// The request is on its way once the question whether there is a server is settled.
		await Promise.resolve();
		await Promise.resolve();
		live.unfollow();
		arrive({ status: 200, body: track(), etag: undefined });

		expect(await asked).toBeUndefined();
	});
});
