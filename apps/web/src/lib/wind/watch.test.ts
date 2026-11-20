import { describe, expect, it } from 'vitest';
import type { Bounds, WindAnswer } from '#lib/core/index.ts';
import { station } from './testing';
import {
	contains,
	grown,
	KEPT_WITHOUT_ANSWER_MS,
	notice,
	NO_WIND,
	REFRESH_MS,
	type WindState,
	WindWatch
} from './watch';

const VALLEY: Bounds = { south: 47, west: 11, north: 48, east: 12 };

const CREDIT = {
	source: 'test',
	provider: 'Testwetter',
	text: 'Source: Testwetter',
	licence: 'CC BY 4.0',
	url: 'https://example.org/'
};

function answerWith(given: Partial<WindAnswer['map']> = {}): WindAnswer {
	return {
		map: { stations: [station()], credits: [CREDIT], truncated: false, ...given },
		lateProviders: []
	};
}

/** A watch over a server the test plays, with a clock the test sets. */
function watching(reply: () => Promise<WindAnswer | undefined> = async () => answerWith()) {
	const server = { reply, asked: [] as Bounds[] };
	const clock = { now: 1_000_000 };
	const told: WindState[] = [];
	const watch = new WindWatch(
		(box) => {
			server.asked.push(box);
			return server.reply();
		},
		(state) => told.push(state),
		() => clock.now
	);
	return { watch, server, clock, told, state: () => told.at(-1) ?? NO_WIND };
}

const unreachable = () => {
	const error = new Error('Venti could not be reached for the wind.');
	error.name = 'Unreachable';
	return Promise.reject(error);
};

describe('boxes on the map', () => {
	it('grow on every side by a share of their size', () => {
		expect(grown(VALLEY, 0.25)).toEqual({ south: 46.75, west: 10.75, north: 48.25, east: 12.25 });
	});

	it('contain what lies wholly inside them', () => {
		expect(contains(grown(VALLEY, 0.25), VALLEY)).toBe(true);
		expect(contains(VALLEY, { ...VALLEY, east: 12.1 })).toBe(false);
	});
});

describe('the wind for the part of the map in view', () => {
	it('is asked for with somewhat more than is in view', async () => {
		const { watch, server, state } = watching();

		expect(watch.show(VALLEY)).toBe(true);
		await watch.load();

		expect(server.asked).toEqual([grown(VALLEY, 0.25)]);
		expect(state()).toMatchObject({ available: true, problem: undefined });
		expect(state().stations).toHaveLength(1);
		expect(state().credits).toEqual([CREDIT]);
	});

	it('is not asked for again after a small drag', async () => {
		const { watch } = watching();
		watch.show(VALLEY);
		await watch.load();

		const nudged = { south: 47.1, west: 11.1, north: 48.1, east: 12.1 };

		expect(watch.show(nudged)).toBe(false);
	});

	it('is asked for again when the map leaves what was answered', async () => {
		const { watch } = watching();
		watch.show(VALLEY);
		await watch.load();

		expect(watch.show({ south: 46, west: 9, north: 47, east: 10 })).toBe(true);
	});

	it('is asked for again after a minute, wherever the map is', async () => {
		const { watch, clock } = watching();
		watch.show(VALLEY);
		await watch.load();
		expect(watch.due).toBe(false);

		clock.now += REFRESH_MS;

		expect(watch.due).toBe(true);
		expect(watch.show(VALLEY)).toBe(true);
	});

	it('is asked for again on every move while the answer leaves stations out', async () => {
		const { watch, state } = watching(async () => answerWith({ truncated: true }));
		watch.show(VALLEY);
		await watch.load();

		expect(state().truncated).toBe(true);
		// Zooming in stays inside what was answered, but may now fit.
		expect(watch.show({ south: 47.4, west: 11.4, north: 47.6, east: 11.6 })).toBe(true);
	});

	it('is not due before the map has said what it shows', () => {
		expect(watching().watch.due).toBe(false);
	});

	it('drops an answer that a later question has overtaken', async () => {
		const first = Promise.withResolvers<WindAnswer | undefined>();
		const { watch, server, state } = watching(() => first.promise);
		watch.show(VALLEY);
		const slow = watch.load();

		server.reply = async () => answerWith({ stations: [station({ name: 'Later' })] });
		await watch.load();
		first.resolve(answerWith({ stations: [station({ name: 'Earlier' })] }));
		await slow;

		expect(state().stations.map((station) => station.name)).toEqual(['Later']);
	});
});

describe('a site without a server', () => {
	it('has no wind, says nothing about it and stops asking', async () => {
		const { watch, server, state } = watching(async () => undefined);
		watch.show(VALLEY);
		await watch.load();

		expect(state()).toEqual({ ...NO_WIND, available: false });
		expect(notice(state(), (items) => items.join(' and '))).toBe('');
		expect(watch.show(VALLEY)).toBe(false);
		expect(watch.due).toBe(false);

		await watch.load();
		expect(server.asked).toHaveLength(1);
	});
});

describe('a server that does not answer', () => {
	it('is told to the pilot in words, and counts as a server', async () => {
		const { watch, state } = watching(unreachable);
		watch.show(VALLEY);
		await watch.load();

		expect(state().problem).toBe('Venti could not be reached for the wind.');
		expect(state().available).toBe(true);
		expect(state().stations).toEqual([]);
	});

	it('leaves the stations on the map for a quarter of an hour', async () => {
		const { watch, server, clock, state } = watching();
		watch.show(VALLEY);
		await watch.load();

		server.reply = unreachable;
		clock.now += KEPT_WITHOUT_ANSWER_MS;
		await watch.load();

		expect(state().problem).toBe('Venti could not be reached for the wind.');
		expect(state().stations).toHaveLength(1);
		expect(state().credits).toHaveLength(1);
	});

	it('takes the stations off the map after that: an arrow must not outlive its wind', async () => {
		const { watch, server, clock, state } = watching();
		watch.show(VALLEY);
		await watch.load();

		server.reply = unreachable;
		clock.now += KEPT_WITHOUT_ANSWER_MS + 1;
		await watch.load();

		expect(state().stations).toEqual([]);
		expect(state().credits).toEqual([]);
		expect(state().problem).toBe('Venti could not be reached for the wind.');
	});

	it('is forgotten with the next answer', async () => {
		const { watch, server, state } = watching(unreachable);
		watch.show(VALLEY);
		await watch.load();

		server.reply = async () => answerWith();
		await watch.load();

		expect(state().problem).toBeUndefined();
		expect(state().stations).toHaveLength(1);
	});

	it('passes on the core’s words as they are, and explains anything else', async () => {
		const refusal = new Error('The server could not answer (status 502).');
		refusal.name = 'Server';
		const first = watching(() => Promise.reject(refusal));
		first.watch.show(VALLEY);
		await first.watch.load();
		expect(first.state().problem).toBe('The server could not answer (status 502).');

		const second = watching(() => Promise.reject(new Error('The worker stopped.')));
		second.watch.show(VALLEY);
		await second.watch.load();
		expect(second.state().problem).toBe('The wind could not be shown. The worker stopped.');
	});
});

describe('what the pilot is told about the wind', () => {
	const list = (items: readonly string[]) => items.join(' and ');

	it('is nothing while all is well', () => {
		expect(notice({ ...NO_WIND, available: true }, list)).toBe('');
	});

	it('is the problem before anything else', () => {
		const state = { ...NO_WIND, problem: 'No answer.', truncated: true, lateProviders: ['A'] };

		expect(notice(state, list)).toBe('No answer.');
	});

	it('is to zoom in when stations were left out', () => {
		expect(notice({ ...NO_WIND, truncated: true, lateProviders: ['A'] }, list)).toBe(
			'There are more stations here than fit on the map. Zoom in to see them all.'
		);
	});

	it('names the providers that are late', () => {
		expect(notice({ ...NO_WIND, lateProviders: ['Wetterdienst', 'GeoSphere'] }, list)).toBe(
			'No new wind data from Wetterdienst and GeoSphere for a while.'
		);
	});
});
