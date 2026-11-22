import { describe, expect, it } from 'vitest';
import type { Bounds, LiveAnswer, LiveFollowAnswer } from '#lib/core/index.ts';
import { aircraft, answer, flight, glider, others } from './testing';
import {
	ageNow,
	FlightFollow,
	FOLLOW_EVERY_MS,
	type Followed,
	type LiveState,
	LiveWatch,
	NO_LIVE,
	NOT_FOLLOWED,
	WIDEST_MAP_DEG
} from './watch';

/** The valley of the sample flight. */
const VALLEY: Bounds = { south: 47.6, west: 11.7, north: 47.8, east: 11.9 };

/** A watch over a server the test plays, with a clock the test sets. */
function watching(reply: () => Promise<LiveAnswer | undefined> = async () => answer(1)) {
	const server = { reply, asked: [] as Bounds[] };
	const clock = { now: 1_000_000 };
	const told: LiveState[] = [];
	const watch = new LiveWatch(
		(box) => {
			server.asked.push(box);
			return server.reply();
		},
		(state) => told.push(state),
		() => clock.now
	);
	return { watch, server, clock, told, state: () => told.at(-1) ?? NO_LIVE };
}

const unreachable = () => Promise.reject(new Error('Venti could not be reached.'));

describe('who is in the air over the part of the map in view', () => {
	it('is asked for a little more than is in view, so that a small drag needs no new request', async () => {
		const { watch, server, state } = watching();

		expect(watch.show(VALLEY)).toBe(true);
		expect(state()).toMatchObject({ close: true, known: false, gliders: [] });
		await watch.load();

		// A quarter as much again on every side.
		const [asked] = server.asked;
		expect(asked.south).toBeCloseTo(47.55);
		expect(asked.west).toBeCloseTo(11.65);
		expect(asked.north).toBeCloseTo(47.85);
		expect(asked.east).toBeCloseTo(11.95);
		expect(state()).toEqual({
			close: true,
			known: true,
			gliders: [glider()],
			answeredAt: 1_000_000,
			truncated: false,
			others: [],
			countWords: '',
			othersCredits: [],
			othersSaid: ''
		});

		// Dragged a little: inside what was asked for, and the next round
		// brings it up to date.
		expect(watch.show({ south: 47.58, west: 11.68, north: 47.78, east: 11.88 })).toBe(false);
		// Dragged out of it: asked at once.
		expect(watch.show({ south: 47.6, west: 12.2, north: 47.8, east: 12.4 })).toBe(true);
	});

	it('is not asked for on a map wider than six degrees, which forgets the gliders it had', async () => {
		const { watch, server, state } = watching();
		watch.show(VALLEY);
		await watch.load();
		expect(state().gliders).toHaveLength(1);

		// The Alps from end to end are a crowd, not pilots.
		const alps = { south: 44, west: 6, north: 49, east: 6 + WIDEST_MAP_DEG + 0.1 };
		expect(watch.show(alps)).toBe(false);
		expect(state()).toEqual(NO_LIVE);
		await watch.load();
		expect(server.asked).toHaveLength(1);

		// Exactly six degrees is not too wide.
		expect(watch.show({ south: 46, west: 9, north: 48, east: 9 + WIDEST_MAP_DEG })).toBe(true);
	});

	it('asks again for the same area, round after round', async () => {
		const { watch, server } = watching();
		watch.show(VALLEY);
		await watch.load();
		await watch.load();
		await watch.load();

		expect(server.asked).toHaveLength(3);
		expect(server.asked[1]).toEqual(server.asked[0]);
		expect(server.asked[2]).toEqual(server.asked[0]);
	});

	it('counts ages from the answer the server gave in full, not from one that says nothing is new', async () => {
		let heard = 1;
		const { watch, clock, state, told } = watching(async () => answer(heard));
		watch.show(VALLEY);
		await watch.load();
		const first = state();
		expect(first.answeredAt).toBe(1_000_000);

		// A quarter of a minute later the server says "not modified": the
		// worker hands the same answer out again, under the same number.
		clock.now += 15_000;
		const toldBefore = told.length;
		await watch.load();
		expect(state()).toBe(first);
		// Nothing has changed, and no screen is drawn again.
		expect(told).toHaveLength(toldBefore);
		// The position is fifteen seconds older by now.
		expect(ageNow(first.gliders[0], first.answeredAt, clock.now)).toBe(19);

		// Then a glider moves, and the server answers in full.
		clock.now += 15_000;
		heard = 2;
		await watch.load();
		expect(state().answeredAt).toBe(1_030_000);
		expect(ageNow(state().gliders[0], state().answeredAt, clock.now)).toBe(4);
	});

	it('leaves the gliders where they were when no answer comes, with their ages counting on', async () => {
		const { watch, server, clock, state } = watching();
		watch.show(VALLEY);
		await watch.load();
		const before = state();

		server.reply = unreachable;
		clock.now += 15_000;
		await watch.load();
		clock.now += 15_000;
		await watch.load();

		expect(state()).toBe(before);
		expect(ageNow(before.gliders[0], before.answeredAt, clock.now)).toBe(34);
		// It goes on asking.
		expect(server.asked).toHaveLength(3);
	});

	it('goes on asking a server that has not answered yet, round after round and not with every drag', async () => {
		const { watch, server, state } = watching(unreachable);
		expect(watch.show(VALLEY)).toBe(true);
		await watch.load();
		expect(state()).toMatchObject({ close: true, known: false, gliders: [] });

		// A small drag asks nothing new; the next round does.
		expect(watch.show({ south: 47.61, west: 11.71, north: 47.81, east: 11.91 })).toBe(false);
		server.reply = async () => answer(1);
		await watch.load();
		expect(state()).toMatchObject({ known: true, gliders: [glider()] });
	});

	it('has nobody on a site without a server, and stops asking', async () => {
		const { watch, server, state } = watching(async () => undefined);
		watch.show(VALLEY);
		await watch.load();

		expect(state().gliders).toEqual([]);
		expect(watch.show({ south: 46, west: 10, north: 46.2, east: 10.2 })).toBe(false);
		await watch.load();
		expect(server.asked).toHaveLength(1);
	});

	it('drops an answer that a later question has overtaken', async () => {
		const waiting: ((answer: LiveAnswer) => void)[] = [];
		const { watch, server, state } = watching(
			() => new Promise<LiveAnswer | undefined>((resolve) => waiting.push(resolve))
		);

		watch.show(VALLEY);
		const first = watch.load();
		// The map moves on before the answer is there.
		watch.show({ south: 47.6, west: 12.2, north: 47.8, east: 12.4 });
		const second = watch.load();
		waiting[1](answer(2, [glider({ id: 'second' })]));
		waiting[0](answer(1, [glider({ id: 'first' })]));
		await Promise.all([first, second]);

		expect(server.asked).toHaveLength(2);
		expect(state().gliders.map((glider) => glider.id)).toEqual(['second']);
	});

	it('does not ask the same question twice at once', async () => {
		const waiting: ((answer: LiveAnswer) => void)[] = [];
		const { watch, server } = watching(
			() => new Promise<LiveAnswer | undefined>((resolve) => waiting.push(resolve))
		);

		watch.show(VALLEY);
		const first = watch.load();
		// The next round comes while the server is still thinking.
		const second = watch.load();
		waiting[0](answer(1));
		await Promise.all([first, second]);

		expect(server.asked).toHaveLength(1);
	});

	it('drops an answer that arrives after the map has zoomed out too far', async () => {
		let arrive: (answer: LiveAnswer) => void = () => {};
		const { watch, state } = watching(
			() => new Promise<LiveAnswer | undefined>((resolve) => (arrive = resolve))
		);

		watch.show(VALLEY);
		const asked = watch.load();
		watch.show({ south: 40, west: 0, north: 55, east: 20 });
		arrive(answer(1));
		await asked;

		expect(state()).toEqual(NO_LIVE);
	});

	it('keeps the others beside the gliders, and the same others while their answer is the same', async () => {
		let reply = answer(1, [glider()], others(2));
		const { watch, state } = watching(async () => reply);
		watch.show(VALLEY);
		await watch.load();
		const first = state();
		expect(first.others[0].aircraft[0].id).toBe('4791c887c4a52d11');
		expect(first.countWords).toBe('1 pilot of Venti, 1 other in the air');
		expect(first.othersSaid).toBe('');

		// The same answer about the others, handed over again by the worker.
		reply = answer(1, [glider()], others(2));
		await watch.load();
		expect(state().others).toBe(first.others);

		// The server lists them no more: none are drawn.
		reply = answer(1, [glider()]);
		await watch.load();
		expect(state().others).toEqual([]);
	});

	it('says nothing on a map too wide for others: the gliders appear when it is close enough', async () => {
		let reply = answer(1, [glider()]);
		const { watch, state } = watching(async () => reply);
		const alps = { south: 44, west: 6, north: 49, east: 6 + WIDEST_MAP_DEG + 0.1 };

		// No network heard of yet: nothing to say.
		watch.show(alps);
		expect(state().othersSaid).toBe('');

		reply = answer(1, [glider()], others(2, [aircraft()]));
		watch.show(VALLEY);
		await watch.load();
		expect(state().othersSaid).toBe('');
		watch.show(alps);
		expect(state()).toMatchObject({ others: [], othersSaid: '' });

		// Close enough for Venti's pilots, still too tall a box for others.
		reply = { ...answer(3, [glider()]), othersTooWide: true };
		watch.show(VALLEY);
		await watch.load();
		expect(state().othersSaid).toBe('');
	});
});

describe('one flight, followed', () => {
	/** A follow over a server the test plays, with a clock the test sets. */
	function following(reply: () => Promise<LiveFollowAnswer | undefined>) {
		const server = { reply, asked: [] as string[] };
		const clock = { now: 2_000_000 };
		const told: Followed[] = [];
		const follow = new FlightFollow(
			'11111111111111111111111111',
			(id) => {
				server.asked.push(id);
				return server.reply();
			},
			(followed) => told.push(followed),
			() => clock.now
		);
		return { follow, server, clock, told, state: () => told.at(-1) ?? NOT_FOLLOWED };
	}

	it('says at once whose flight it is, before the server has answered', () => {
		const { state } = following(async () => ({ view: flight(), heard: 1 }));

		expect(state()).toEqual({
			id: '11111111111111111111111111',
			view: undefined,
			answeredAt: 0,
			problem: undefined
		});
	});

	it('asks again at once while the server has more of the track, and then every ten seconds', async () => {
		let more = true;
		const { follow, server, state } = following(async () => ({
			view: flight({ more }),
			heard: server.asked.length
		}));

		expect(await follow.step()).toBe(0);
		more = false;
		expect(await follow.step()).toBe(FOLLOW_EVERY_MS);
		expect(state().view?.more).toBe(false);
		expect(state().answeredAt).toBe(2_000_000);
	});

	it('counts the age from the answer the server gave in full', async () => {
		let heard = 1;
		const { follow, clock, told, state } = following(async () => ({ view: flight(), heard }));
		await follow.step();
		const first = state();

		// "Not modified": the same answer under the same number.
		clock.now += 10_000;
		const toldBefore = told.length;
		expect(await follow.step()).toBe(FOLLOW_EVERY_MS);
		expect(told).toHaveLength(toldBefore);
		expect(ageNow(first.view!.glider, first.answeredAt, clock.now)).toBe(14);

		clock.now += 10_000;
		heard = 2;
		await follow.step();
		expect(state().answeredAt).toBe(2_020_000);
	});

	it('leaves the track as it was when the server cannot be asked, and asks again later', async () => {
		const { follow, server, state } = following(async () => ({ view: flight(), heard: 1 }));
		await follow.step();
		const before = state();

		server.reply = unreachable;
		expect(await follow.step()).toBe(FOLLOW_EVERY_MS);
		expect(state()).toBe(before);
	});

	it('says in words that the track is gone, and stops asking', async () => {
		const { follow, server, state } = following(async () => ({ view: flight(), heard: 1 }));
		await follow.step();

		server.reply = async () => ({ gone: 'This flight is no longer shown.' });
		expect(await follow.step()).toBeUndefined();
		expect(state()).toEqual({
			id: '11111111111111111111111111',
			view: undefined,
			answeredAt: 0,
			problem: 'This flight is no longer shown.'
		});
	});

	it('has no next step when there is nobody to ask', async () => {
		const { follow } = following(async () => undefined);

		expect(await follow.step()).toBeUndefined();
	});
});

describe('the age of a position', () => {
	it('is its age when the server answered and the time since, by this device’s clock', () => {
		expect(ageNow(glider({ ageS: 4 }), 1_000_000, 1_045_500)).toBe(49.5);
	});

	it('never runs backwards when the device’s clock is set back', () => {
		expect(ageNow(glider({ ageS: 4 }), 1_000_000, 900_000)).toBe(4);
	});
});
