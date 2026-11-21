import { describe, expect, it } from 'vitest';
import type { RadarFrameView, RadarLayerView, RadarView } from '#lib/core/index.ts';
import { keyColor, NOT_FLYABLE_ON_MAP } from '#lib/design/palette.ts';
import { squareOpacity } from '#lib/forecast/thermal-layer.ts';
import { RADAR, radarLayer, radarOpacity } from './radar-layer';
import { RadarModel } from './radar.svelte';
import {
	ASK_EVERY_MS,
	chosenTime,
	colours,
	corners,
	frameOf,
	isDue,
	layerOf,
	pictureAddress,
	solid,
	stripCells,
	WORDS
} from './radar';

/** A frame every five minutes from 11:00 UTC on, as the core words them at `nowMin` past 11. */
function frames(count: number, nowMin = (count - 1) * 5 + 4): RadarFrameView[] {
	return Array.from({ length: count }, (_, index) => {
		const minute = index * 5;
		const clock = `${String(11 + Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`;
		const age = `${nowMin - minute} minutes ago`;
		return {
			timeUnixS: 1_795_258_800 + minute * 60,
			path: `/v1/radar/made-up/${index}.png`,
			clock,
			interval: '',
			ageS: (nowMin - minute) * 60,
			age,
			label: `Rain radar, ${clock}, ${age}`
		};
	});
}

function layer(count: number, more: Partial<RadarLayerView> = {}): RadarLayerView {
	const held = frames(count);
	return {
		source: 'made-up',
		west: 9.4,
		south: 46.3,
		east: 17.2,
		north: 48.5,
		frames: held,
		line: held.at(-1)?.label ?? 'No radar picture yet',
		stale: false,
		staleWords: '',
		credit: undefined,
		...more
	};
}

/** The key as the core gives it: from the most rain down, with its stand-in colours. */
const KEY: RadarView['key'] = [
	{ step: 6, words: 'Cloudburst', scale: 'wind', scaleStep: 7 },
	{ step: 5, words: 'Very heavy rain', scale: 'wind', scaleStep: 6 },
	{ step: 4, words: 'Heavy rain', scale: 'vario', scaleStep: 1 },
	{ step: 3, words: 'Rain', scale: 'vario', scaleStep: 2 },
	{ step: 2, words: 'Light rain', scale: 'vario', scaleStep: 3 },
	{ step: 1, words: 'A trace', scale: 'vario', scaleStep: 4 }
];

function view(count: number, more: Partial<RadarLayerView> = {}): RadarView {
	return { layers: [layer(count, more)], key: KEY, unseenWords: 'Not seen', note: 'A note.' };
}

describe('which frame is shown', () => {
	it('is the newest until the pilot chooses another, and the newest again when that one is gone', () => {
		const one = layer(3);
		expect(layerOf(view(3))?.source).toBe('made-up');
		expect(layerOf(undefined)).toBeUndefined();
		expect(frameOf(one, undefined)).toBe(one.frames[2]);
		expect(frameOf(one, one.frames[0].timeUnixS)).toBe(one.frames[0]);
		expect(frameOf(one, 1)).toBe(one.frames[2]);
		expect(frameOf(layer(0), undefined)).toBeUndefined();
	});

	it('remembers the newest as "the newest", so that the next frame is shown when it arrives', () => {
		const one = layer(3);
		expect(chosenTime(one, 0)).toBe(one.frames[0].timeUnixS);
		expect(chosenTime(one, 2)).toBeUndefined();
	});
});

describe('the strip of frames', () => {
	it('writes the minutes of each frame of a short strip, and says time and age to a screen reader', () => {
		expect(stripCells(frames(2))).toEqual([
			{ short: '00', label: '11:00, 9 minutes ago' },
			{ short: '05', label: '11:05, 4 minutes ago' }
		]);
	});

	it('writes every third of two hours of frames, counted from the newest', () => {
		const cells = stripCells(frames(25));
		expect(cells).toHaveLength(25);
		expect(cells.at(-1)?.short).toBe('00');
		expect(cells.at(-2)?.short).toBe('');
		expect(cells.at(-4)?.short).toBe('45');
		expect(cells.filter((cell) => cell.short !== '')).toHaveLength(9);
		// Every cell says its frame to a screen reader all the same.
		expect(cells.every((cell) => /^\d\d:\d\d, \d+ minutes ago$/.test(cell.label))).toBe(true);
	});
});

describe('the colours of a picture', () => {
	it('are the tokens the core’s key names, the first step first, and the forecast map’s veil', () => {
		const given = colours(KEY);
		expect(given.rain).toEqual([...KEY].reverse().map((line) => solid(keyColor(line))));
		expect(given.unseen).toBe(solid(NOT_FLYABLE_ON_MAP));
		expect(solid('#6C5F8C')).toBe(0x6c5f8cff);
		// Six different colours, none of them the veil's.
		expect(new Set([...given.rain, given.unseen]).size).toBe(7);
	});
});

describe('the picture on the map', () => {
	it('is pinned by its corners from the top left, clockwise', () => {
		expect(corners(layer(1))).toEqual([
			[9.4, 48.5],
			[17.2, 48.5],
			[17.2, 46.3],
			[9.4, 46.3]
		]);
	});

	it('is drawn unsmoothed, as see-through as the forecast’s squares, and half as strong when old', () => {
		const drawn = radarLayer(false);
		expect(drawn).toMatchObject({ id: RADAR, type: 'raster', source: RADAR });
		expect(drawn.paint?.['raster-resampling']).toBe('nearest');
		expect(drawn.paint?.['raster-fade-duration']).toBe(0);
		expect(drawn.paint?.['raster-opacity']).toEqual(squareOpacity());
		expect(radarOpacity(true)).toEqual(['interpolate', ['linear'], ['zoom'], 8, 0.25, 12, 0.15]);
	});

	it('reaches the map as an address that holds the picture itself', () => {
		const png = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0, 255]);
		expect(pictureAddress(png)).toBe('data:image/png;base64,iVBORwD/');
		// A picture larger than one call takes arguments comes through whole.
		const large = new Uint8Array(100_000).map((_, index) => index % 251);
		const back = Uint8Array.from(atob(pictureAddress(large).split(',')[1]), (c) => c.charCodeAt(0));
		expect(back).toEqual(large);
	});
});

describe('asking for the list', () => {
	it('is due at first and again five minutes after the last asking', () => {
		expect(isDue(undefined, 0)).toBe(true);
		expect(isDue(1000, 1000 + ASK_EVERY_MS - 1)).toBe(false);
		expect(isDue(1000, 1000 + ASK_EVERY_MS)).toBe(true);
	});
});

/** A core that has a list and a picture for every frame, and counts what it is asked. */
function core(list: () => RadarView | undefined) {
	const asked = { lists: 0, again: 0, pictures: [] as string[] };
	let fail: string | undefined;
	return {
		asked,
		failPicture: (path: string | undefined) => (fail = path),
		radar: async () => {
			asked.lists += 1;
			return list();
		},
		radarAgain: async () => {
			asked.again += 1;
			return list();
		},
		radarPicture: async (path: string) => {
			asked.pictures.push(path);
			if (path === fail) throw new Error('lost on the way');
			return new TextEncoder().encode(path);
		}
	};
}

const address = (path: string) => pictureAddress(new TextEncoder().encode(path));

describe('the radar as the map tab sees it', () => {
	it('opens on the newest frame and puts its picture on the map', async () => {
		const server = core(() => view(3));
		const model = new RadarModel(server);
		expect(model.keyOpen).toBe(false);
		await model.load();
		await model.showFrame();

		expect(model.frame?.clock).toBe('11:10');
		expect(model.shownFrame?.label).toBe('Rain radar, 11:10, 4 minutes ago');
		expect(model.onMap).toEqual({
			url: address('/v1/radar/made-up/2.png'),
			corners: corners(layer(3)),
			faded: false
		});
		expect(server.asked.pictures).toEqual(['/v1/radar/made-up/2.png']);
	});

	it('swaps the picture for another frame’s, and keeps each for the visit', async () => {
		const server = core(() => view(3));
		const model = new RadarModel(server);
		await model.load();
		await model.showFrame();

		model.choose(0);
		await model.showFrame();
		expect(model.onMap?.url).toBe(address('/v1/radar/made-up/0.png'));
		expect(model.shownFrame?.clock).toBe('11:00');

		model.choose(2);
		await model.showFrame();
		expect(model.chosenTimeUnixS).toBeUndefined();
		expect(model.onMap?.url).toBe(address('/v1/radar/made-up/2.png'));
		expect(server.asked.pictures).toEqual(['/v1/radar/made-up/2.png', '/v1/radar/made-up/0.png']);
	});

	it('leaves the picture before on the map, under its own time, when another cannot be had', async () => {
		const server = core(() => view(3));
		const model = new RadarModel(server);
		await model.load();
		await model.showFrame();

		server.failPicture('/v1/radar/made-up/1.png');
		model.choose(1);
		await model.showFrame();
		expect(model.frame?.clock).toBe('11:05');
		expect(model.shownFrame?.clock).toBe('11:10');
		expect(model.pictureProblem).toBe(`${WORDS.unreachable} lost on the way`);

		// "Try again" asks again: a failure is not kept.
		server.failPicture(undefined);
		await model.showFrame();
		expect(model.shownFrame?.clock).toBe('11:05');
		expect(model.pictureProblem).toBeUndefined();
	});

	it('goes on to a new frame only if the pilot was looking at the newest', async () => {
		let count = 3;
		const server = core(() => view(count));
		const model = new RadarModel(server);
		await model.load();
		await model.showFrame();

		count = 4;
		await model.load();
		await model.showFrame();
		expect(model.shownFrame?.clock).toBe('11:15');

		model.choose(1);
		await model.showFrame();
		count = 5;
		await model.load();
		await model.showFrame();
		expect(model.shownFrame?.clock).toBe('11:05');
	});

	it('draws an old picture fainter, as the core says it is old', async () => {
		const server = core(() => view(2, { stale: true, staleWords: 'Old.' }));
		const model = new RadarModel(server);
		await model.load();
		await model.showFrame();
		expect(model.onMap?.faded).toBe(true);
	});

	it('plays the frames once, the oldest first, and stops at the newest', async () => {
		const server = core(() => view(4));
		const seen: (string | undefined)[] = [];
		const waits: number[] = [];
		const model: RadarModel = new RadarModel(server, Date.now, async (ms) => {
			waits.push(ms);
			seen.push(model.frame?.clock);
		});
		await model.load();

		await model.play();

		expect(seen).toEqual(['11:00', '11:05', '11:10']);
		expect(waits).toEqual([250, 250, 250]);
		expect(model.frame?.clock).toBe('11:15');
		expect(model.chosenTimeUnixS).toBeUndefined();
		expect(model.playing).toBe(false);
		// Every picture was fetched before the first step.
		expect(server.asked.pictures).toHaveLength(4);
	});

	it('ends a play when the pilot chooses a frame', async () => {
		const server = core(() => view(4));
		const model: RadarModel = new RadarModel(server, Date.now, async () => {
			if (model.frame?.clock === '11:05') model.choose(0);
		});
		await model.load();
		await model.play();
		expect(model.playing).toBe(false);
		expect(model.frame?.clock).toBe('11:00');
	});

	it('says what the server says when it has no radar, and that a site has no server', async () => {
		const refusing = {
			...core(() => undefined),
			radar: async () => {
				const error = new Error('This server has no rain radar.');
				error.name = 'Server';
				throw error;
			}
		};
		const model = new RadarModel(refusing);
		await model.load();
		expect(model.problem).toBe('This server has no rain radar.');
		expect(model.view).toBeUndefined();

		const none = new RadarModel(core(() => undefined));
		await none.load();
		expect(none.problem).toBe(WORDS.noServer);
	});

	it('keeps the list it has when a later asking fails, and lets its frames age', async () => {
		let reachable = true;
		const server = core(() => view(2));
		const flaky = {
			...server,
			radar: async () => {
				if (!reachable) throw new Error('offline');
				return server.radar();
			}
		};
		const model = new RadarModel(flaky);
		await model.load();
		reachable = false;
		await model.load();
		expect(model.problem).toBe(`${WORDS.unreachable} offline`);
		expect(model.layer?.frames).toHaveLength(2);
		expect(server.asked.again).toBe(1);
	});

	it('asks at once when the layer is switched on, every five minutes after, and says the ages in between', async () => {
		const { vi } = await import('vitest');
		vi.useFakeTimers();
		try {
			const server = core(() => view(2));
			const model = new RadarModel(server, Date.now);
			const stop = model.keepCurrent();
			await vi.advanceTimersByTimeAsync(0);
			expect(server.asked).toMatchObject({ lists: 1, again: 0 });

			await vi.advanceTimersByTimeAsync(4 * 60_000);
			expect(server.asked).toMatchObject({ lists: 1, again: 4 });
			await vi.advanceTimersByTimeAsync(60_000);
			expect(server.asked).toMatchObject({ lists: 2, again: 4 });

			stop();
			await vi.advanceTimersByTimeAsync(10 * 60_000);
			expect(server.asked).toMatchObject({ lists: 2, again: 4 });
		} finally {
			vi.useRealTimers();
		}
	});
});
