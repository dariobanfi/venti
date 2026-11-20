import { describe, expect, it } from 'vitest';
import type { Bounds, SitesMap } from '#lib/core/index.ts';
import { answer, CREDIT, site } from './testing';
import { KEPT_MS, NO_SITES, SITES_MIN_ZOOM, type SitesState, SitesWatch } from './watch';

/** A valley, and the zoom at which a map shows one. */
const VALLEY: Bounds = { south: 47.6, west: 11.7, north: 47.8, east: 11.9 };
const CLOSE = SITES_MIN_ZOOM + 1;

/** A watch over a server the test plays, with a clock the test sets. */
function watching(reply: () => Promise<SitesMap | undefined> = async () => answer()) {
	const server = { reply, asked: [] as Bounds[] };
	const clock = { now: 1_000_000 };
	const told: SitesState[] = [];
	const watch = new SitesWatch(
		(box) => {
			server.asked.push(box);
			return server.reply();
		},
		(state) => told.push(state),
		() => clock.now
	);
	return { watch, server, clock, told, state: () => told.at(-1) ?? NO_SITES };
}

const unreachable = () => {
	const error = new Error('Venti could not be reached for flying sites.');
	error.name = 'Unreachable';
	return Promise.reject(error);
};

describe('the flying sites for the part of the map in view', () => {
	it('are asked for once the map shows a valley and not a country', async () => {
		const { watch, server, state } = watching();

		// A map of the whole Alps: sites would be a carpet of dots.
		expect(watch.show({ south: 45, west: 6, north: 49, east: 16 }, SITES_MIN_ZOOM - 0.1)).toBe(
			false
		);
		await watch.load();
		expect(server.asked).toEqual([]);
		expect(state().close).toBe(false);

		expect(watch.show(VALLEY, SITES_MIN_ZOOM)).toBe(true);
		expect(state()).toMatchObject({ close: true, known: false, sites: [] });
		await watch.load();

		// Half as much again on every side, so that a drag needs no new request.
		expect(server.asked).toHaveLength(1);
		const [asked] = server.asked;
		expect(asked.south).toBeCloseTo(47.5);
		expect(asked.west).toBeCloseTo(11.6);
		expect(asked.north).toBeCloseTo(47.9);
		expect(asked.east).toBeCloseTo(12);
		expect(state()).toEqual({
			close: true,
			known: true,
			sites: [site()],
			credits: [CREDIT],
			asOfUnixS: 1_795_028_700,
			isWhole: true,
			problem: undefined
		});
	});

	it('are kept for an hour, and for a map dragged a little', async () => {
		const { watch, server, clock, state } = watching();
		watch.show(VALLEY, CLOSE);
		await watch.load();

		const nearby = { south: 47.62, west: 11.75, north: 47.82, east: 11.95 };
		clock.now += KEPT_MS - 1;
		expect(watch.show(nearby, CLOSE)).toBe(false);
		// A timer that fires after the answer came asks for nothing.
		await watch.load();
		expect(server.asked).toHaveLength(1);

		// Dragged out of what was asked for.
		const elsewhere = { south: 47.6, west: 12.3, north: 47.8, east: 12.5 };
		expect(watch.show(elsewhere, CLOSE)).toBe(true);
		expect(state().known).toBe(false);
		// The sites that are there stay drawn until the new ones arrive.
		expect(state().sites).toHaveLength(1);

		// Back, and an hour after the answer: asked again.
		clock.now += 1;
		expect(watch.show(VALLEY, CLOSE)).toBe(true);
	});

	it('leave the screens when the map is zoomed out, and come back without a request', async () => {
		const { watch, server, state } = watching();
		watch.show(VALLEY, CLOSE);
		await watch.load();

		watch.show({ south: 45, west: 6, north: 49, east: 16 }, 6);
		expect(state()).toEqual(NO_SITES);

		expect(watch.show(VALLEY, CLOSE)).toBe(false);
		expect(state().sites).toHaveLength(1);
		expect(state().known).toBe(true);
		expect(server.asked).toHaveLength(1);
	});

	it('say in words why they are missing, and keep what was drawn', async () => {
		const { watch, server, state } = watching();
		watch.show(VALLEY, CLOSE);
		await watch.load();

		server.reply = unreachable;
		watch.show({ south: 47.6, west: 12.3, north: 47.8, east: 12.5 }, CLOSE);
		await watch.load();
		expect(state()).toMatchObject({
			known: false,
			problem: 'Venti could not be reached for flying sites.'
		});
		expect(state().sites).toHaveLength(1);

		// An error that was not written for the pilot is introduced.
		server.reply = () => Promise.reject(new Error('out of memory'));
		await watch.load();
		expect(state().problem).toBe('The flying sites could not be shown. out of memory');

		// The next answer clears it.
		server.reply = async () => answer({ sites: [] });
		await watch.load();
		expect(state()).toMatchObject({ known: true, sites: [], problem: undefined });
	});

	it('are never asked for again once the site turns out to have no server', async () => {
		const { watch, server, state } = watching(async () => undefined);
		watch.show(VALLEY, CLOSE);
		await watch.load();
		expect(state()).toMatchObject({ sites: [], problem: undefined });

		expect(watch.show({ south: 47.6, west: 12.3, north: 47.8, east: 12.5 }, CLOSE)).toBe(false);
		await watch.load();
		expect(server.asked).toHaveLength(1);
	});

	it('drop an answer that a later question has overtaken', async () => {
		const answers: ((map: SitesMap) => void)[] = [];
		const { watch, state } = watching(() => new Promise((resolve) => answers.push(resolve)));

		watch.show(VALLEY, CLOSE);
		const first = watch.load();
		watch.show({ south: 47.6, west: 12.3, north: 47.8, east: 12.5 }, CLOSE);
		const second = watch.load();

		answers[1](answer({ sites: [site({ id: 'test:node/2' })] }));
		answers[0](answer());
		await Promise.all([first, second]);

		expect(state().sites.map((site) => site.id)).toEqual(['test:node/2']);
	});
});
