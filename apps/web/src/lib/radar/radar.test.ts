import { describe, expect, it } from 'vitest';
import type { RadarFrameView, RadarLayerView, RadarMoment, RadarView } from '#lib/core/index.ts';
import { keyColor, NOT_FLYABLE_ON_MAP } from '#lib/design/palette.ts';
import { isRadar, radarBefore, radarId, radarLayer, radarOpacity } from './radar-layer';
import { RadarModel } from './radar.svelte';
import {
	ASK_EVERY_MS,
	cautioned,
	chosenTime,
	CLEAR,
	colours,
	corners,
	drawn,
	drawnKey,
	isDue,
	momentOf,
	pictureAddress,
	pictureKey,
	solid,
	letThrough,
	TRACE_ALPHA,
	veiled,
	WORDS
} from './radar';

/**
 * Frames from 11:00 UTC on, a frame every `everyMin` minutes, as the core
 * words them at `nowMin` past 11. All of it is made up.
 */
function frames(
	count: number,
	nowMin = (count - 1) * 5 + 4,
	{ source = 'made-up', name = 'Rain radar', everyMin = 5, fromMin = 0 } = {}
): RadarFrameView[] {
	return Array.from({ length: count }, (_, index) => {
		const minute = fromMin + index * everyMin;
		const onClock = 11 * 60 + minute;
		const clock = `${String(Math.floor(onClock / 60)).padStart(2, '0')}:${String(onClock % 60).padStart(2, '0')}`;
		const age = `${nowMin - minute} minutes ago`;
		return {
			timeUnixS: 1_795_258_800 + minute * 60,
			path: `/v1/radar/${source}/${index}.png`,
			clock,
			interval: '',
			ageS: (nowMin - minute) * 60,
			age,
			label: `${name}, ${clock}, ${age}`
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
		what: 'A made-up radar, every 5 minutes',
		drawsUnseen: true,
		...more
	};
}

/** The key as the core gives it: from the most rain down, with its stand-in colours. */
const KEY: RadarView['key'] = [
	{ step: 6, words: 'Cloudburst', name: 'Cloudburst', amount: '', scale: 'wind', scaleStep: 7 },
	{
		step: 5,
		words: 'Very heavy rain',
		name: 'Very heavy rain',
		amount: '',
		scale: 'wind',
		scaleStep: 6
	},
	{ step: 4, words: 'Heavy rain', name: 'Heavy rain', amount: '', scale: 'vario', scaleStep: 1 },
	{ step: 3, words: 'Rain', name: 'Rain', amount: '', scale: 'vario', scaleStep: 2 },
	{ step: 2, words: 'Light rain', name: 'Light rain', amount: '', scale: 'vario', scaleStep: 3 },
	{ step: 1, words: 'A trace', name: 'A trace', amount: '', scale: 'vario', scaleStep: 4 }
];

/** One layer, as a server with one radar source has it: a moment for a frame, with the frame's own line. */
function view(count: number, more: Partial<RadarLayerView> = {}): RadarView {
	const one = layer(count, more);
	return {
		layers: [one],
		moments: one.frames.map((frame, index) => ({
			timeUnixS: frame.timeUnixS,
			clock: frame.clock,
			label: frame.label,
			line: `Rain, ${frame.clock}, measured`,
			frames: [{ layer: 0, frame: index, drawsUnseen: true }]
		})),
		firstMoment: Math.max(count - 1, 0),
		key: KEY,
		unseenWords: 'Not seen',
		note: 'A note.'
	};
}

const credit = (text: string) => ({
	source: 'made-up',
	provider: text,
	licence: 'CC BY 4.0',
	text,
	url: ''
});

/**
 * Two layers in the order to draw them, as the core gives them: an
 * analysis of every quarter of an hour at the bottom, with frames of
 * 10:45 and 11:00, and a radar of every five minutes on top, with `count`
 * frames from 11:00 on. The stops are the radar's; at each the analysis of
 * 11:00 lies under the radar's picture and draws the veil. Made up, after
 * the core's rule.
 */
function two(
	count = 3,
	more: { analysis?: Partial<RadarLayerView>; radars?: Partial<RadarLayerView> } = {}
): RadarView {
	const nowMin = (count - 1) * 5 + 4;
	const analysis = layer(0, {
		source: 'made-up-analysis',
		what: 'A made-up analysis, every 15 minutes',
		frames: frames(2, nowMin, {
			source: 'made-up-analysis',
			name: 'Rain analysis',
			everyMin: 15,
			fromMin: -15
		}),
		west: 9,
		credit: credit('Analysis by nobody'),
		...more.analysis
	});
	const radars = layer(count, {
		drawsUnseen: false,
		credit: credit('Radars of nobody'),
		...more.radars
	});
	return {
		layers: [analysis, radars],
		moments: radars.frames.map((frame, index) => ({
			timeUnixS: frame.timeUnixS,
			clock: frame.clock,
			label: index === 0 ? frame.label : `${frame.label}. The made-up analysis is of 11:00.`,
			line: `Rain, ${frame.clock}, measured`,
			frames: [
				{ layer: 0, frame: 1, drawsUnseen: true },
				{ layer: 1, frame: index, drawsUnseen: false }
			]
		})),
		firstMoment: count - 1,
		key: KEY,
		unseenWords: 'Not seen',
		note: 'A note.'
	};
}

/** What a moment draws, as path and whether the veil is drawn. */
const pictures = (of: RadarView, moment: RadarMoment | undefined) =>
	drawn(of, moment).map((picture) => [picture.path, picture.drawsUnseen]);

describe('which moment is shown', () => {
	it('is the one the core opens on until the pilot chooses another, and that one again when the chosen is gone', () => {
		const one = view(3);
		expect(momentOf(undefined, undefined)).toBeUndefined();
		expect(momentOf(one, undefined)).toBe(one.moments[2]);
		expect(momentOf(one, one.moments[0].timeUnixS)).toBe(one.moments[0]);
		expect(momentOf(one, 1)).toBe(one.moments[2]);
		expect(momentOf(view(0), undefined)).toBeUndefined();
		// Where the core opens is the core's to say, not "the last".
		expect(momentOf({ ...one, firstMoment: 1 }, undefined)).toBe(one.moments[1]);
	});

	it('remembers the newest as "the newest", so that the next moment is shown when it arrives', () => {
		const one = view(3);
		expect(chosenTime(one, 0)).toBe(one.moments[0].timeUnixS);
		expect(chosenTime(one, 2)).toBeUndefined();
	});
});

describe('what a moment draws', () => {
	it('is a picture for each of its frames, the lowest first, and the veil only in the one the core names', () => {
		const both = two();
		expect(pictures(both, both.moments[2])).toEqual([
			['/v1/radar/made-up-analysis/1.png', true],
			['/v1/radar/made-up/2.png', false]
		]);
		// Each is pinned by its own layer's corners and faded by its own layer's age.
		const stale = two(3, { radars: { stale: true, staleWords: 'Old.' } });
		expect(drawn(stale, stale.moments[0])).toEqual([
			{
				layer: 0,
				path: '/v1/radar/made-up-analysis/1.png',
				drawsUnseen: true,
				above: ['/v1/radar/made-up/0.png'],
				corners: corners(stale.layers[0]),
				faded: false
			},
			{
				layer: 1,
				path: '/v1/radar/made-up/0.png',
				drawsUnseen: false,
				above: [],
				corners: corners(stale.layers[1]),
				faded: true
			}
		]);
		expect(drawn(both, undefined)).toEqual([]);
	});

	it('names for each picture the frames that lie over it, the lowest first, and none for the topmost', () => {
		const both = two();
		expect(drawn(both, both.moments[2]).map((picture) => picture.above)).toEqual([
			['/v1/radar/made-up/2.png'],
			[]
		]);
		// Three at a moment: each under all that come after it.
		const three: RadarMoment = {
			...both.moments[2],
			frames: [
				{ layer: 0, frame: 0, drawsUnseen: true },
				{ layer: 0, frame: 1, drawsUnseen: false },
				{ layer: 1, frame: 2, drawsUnseen: false }
			]
		};
		expect(drawn(both, three).map((picture) => picture.above)).toEqual([
			['/v1/radar/made-up-analysis/1.png', '/v1/radar/made-up/2.png'],
			['/v1/radar/made-up/2.png'],
			[]
		]);
		// A frame the list does not have lies over nothing.
		const gone: RadarMoment = {
			...both.moments[2],
			frames: [
				{ layer: 0, frame: 1, drawsUnseen: true },
				{ layer: 1, frame: 9, drawsUnseen: false }
			]
		};
		expect(drawn(both, gone).map((picture) => picture.above)).toEqual([[]]);
	});

	it('is nothing of a layer that has no frame at the moment, and the app lays none of its own under it', () => {
		const both = two();
		// The analysis was too old for this moment: the radars' picture alone, with the veil.
		const alone: RadarMoment = {
			...both.moments[0],
			frames: [{ layer: 1, frame: 0, drawsUnseen: true }]
		};
		expect(pictures(both, alone)).toEqual([['/v1/radar/made-up/0.png', true]]);
		// A frame the list does not have is not drawn.
		const beyond: RadarMoment = { ...alone, frames: [{ layer: 1, frame: 9, drawsUnseen: true }] };
		expect(drawn(both, beyond)).toEqual([]);
	});

	it('keeps a frame’s picture with the veil apart from the same frame’s without', () => {
		const withVeil = { path: '/v1/radar/made-up/0.png', drawsUnseen: true, above: [] as string[] };
		const clear = { ...withVeil, drawsUnseen: false };
		expect(pictureKey(withVeil)).not.toBe(pictureKey(clear));
		expect(drawnKey([withVeil, clear])).not.toBe(drawnKey([clear, withVeil]));
		expect(drawnKey([withVeil])).toBe(drawnKey([{ ...withVeil }]));
	});

	it('keeps a frame’s picture under one frame apart from the same frame’s under another, and from its own alone', () => {
		const alone = { path: '/v1/radar/made-up-analysis/1.png', drawsUnseen: true, above: [] };
		const underOne = { ...alone, above: ['/v1/radar/made-up/1.png'] };
		const underOther = { ...alone, above: ['/v1/radar/made-up/2.png'] };
		expect(pictureKey(underOne)).toBe(
			'/v1/radar/made-up-analysis/1.png veiled under /v1/radar/made-up/1.png'
		);
		expect(new Set([alone, underOne, underOther].map(pictureKey)).size).toBe(3);
		expect(pictureKey(underOne)).toBe(pictureKey({ ...underOne, above: [...underOne.above] }));
		// The same analysis under two moments' radars: the moments do not draw the same.
		const both = two();
		expect(drawnKey(drawn(both, both.moments[1]))).not.toBe(drawnKey(drawn(both, both.moments[2])));
	});
});

describe('the caution', () => {
	it('is of the topmost layer that has frames', () => {
		const both = two();
		expect(cautioned(both)?.source).toBe('made-up');
		// The radars have no picture yet: the analysis under them speaks.
		const noRadar = two(3, { radars: { frames: [] } });
		expect(cautioned(noRadar)?.source).toBe('made-up-analysis');
		// No layer has one: the topmost says that there is none yet.
		const none = two(3, { radars: { frames: [] }, analysis: { frames: [] } });
		expect(cautioned(none)?.source).toBe('made-up');
		expect(cautioned(undefined)).toBeUndefined();
	});
});

describe('the colours of a picture', () => {
	it('are the tokens the core’s key names, the first step first, and the forecast map’s veil', () => {
		const given = colours(KEY, true);
		const lowestFirst = [...KEY].reverse();
		expect(given.rain).toEqual([
			letThrough(keyColor(lowestFirst[0]), TRACE_ALPHA),
			...lowestFirst.slice(1).map((line) => solid(keyColor(line)))
		]);
		// The rain solid, the veil let through: the map stays quiet where nothing is seen.
		expect(given.unseen).toBe(veiled(NOT_FLYABLE_ON_MAP));
		expect(solid('#6C5F8C')).toBe(0x6c5f8cff);
		expect(veiled('#6C5F8C')).toBe(0x6c5f8c66);
		// Six different colours, none of them the veil's.
		expect(new Set([...given.rain, given.unseen]).size).toBe(7);
	});

	it('have nothing at all for "not seen" in a picture that lies over another, and the same rain', () => {
		const upper = colours(KEY, false);
		expect(upper.unseen).toBe(CLEAR);
		expect(upper.unseen).toBe(0x00000000);
		expect(upper.rain).toEqual(colours(KEY, true).rain);
	});
});

describe('the pictures on the map', () => {
	it('are pinned by their corners from the top left, clockwise', () => {
		expect(corners(layer(1))).toEqual([
			[9.4, 48.5],
			[17.2, 48.5],
			[17.2, 46.3],
			[9.4, 46.3]
		]);
	});

	it('are drawn unsmoothed, nearly solid, and half as strong when old', () => {
		const drawnLayer = radarLayer(0, false);
		expect(drawnLayer).toMatchObject({ id: 'radar-0', type: 'raster', source: 'radar-0' });
		expect(drawnLayer.paint?.['raster-resampling']).toBe('nearest');
		expect(drawnLayer.paint?.['raster-fade-duration']).toBe(0);
		expect(drawnLayer.paint?.['raster-opacity']).toBe(0.85);
		expect(radarOpacity(true)).toBe(0.45);
		expect(radarLayer(1, true).paint?.['raster-opacity']).toEqual(radarOpacity(true));
	});

	it('have a source and a layer each, and a later layer of the radar lies above an earlier', () => {
		expect(radarLayer(1, false)).toMatchObject({ id: radarId(1), source: radarId(1) });
		expect(radarId(0)).not.toBe(radarId(1));
		expect(isRadar(radarId(3))).toBe(true);
		expect(isRadar('thermal-map')).toBe(false);
		// The first goes under what the forecast's squares go under.
		expect(radarBefore(0, ['ground', 'names'], 'names')).toBe('names');
		// The upper one too, which puts it over the lower one already there.
		expect(radarBefore(1, ['ground', radarId(0), 'names'], 'names')).toBe('names');
		// A lower one that comes later goes under the nearest of the radar's above it.
		expect(radarBefore(0, ['ground', radarId(1), radarId(2), 'names'], 'names')).toBe(radarId(1));
		expect(radarBefore(1, ['ground', radarId(0), radarId(2)], undefined)).toBe(radarId(2));
		expect(radarBefore(0, [], undefined)).toBeUndefined();
	});

	it('reach the map as addresses that hold the pictures themselves', () => {
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

/**
 * A core that has a list and a picture for every frame, and counts what it
 * is asked. A picture is its path, where the veil is drawn in it the word
 * for that, and the paths of the frames it was asked to be coloured
 * under: what `address` makes of them.
 */
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
		radarPicture: async (
			path: string,
			above: string[],
			given: { rain: number[]; unseen: number }
		) => {
			const veil = given.unseen === CLEAR ? '' : ' veiled';
			const picture = `${path}${veil}${above.map((over) => ` under ${over}`).join('')}`;
			asked.pictures.push(picture);
			if (path === fail) throw new Error('lost on the way');
			return new TextEncoder().encode(picture);
		}
	};
}

const address = (picture: string) => pictureAddress(new TextEncoder().encode(picture));

const RADARS = '/v1/radar/made-up';
const ANALYSIS = '/v1/radar/made-up-analysis';

describe('the radar as the map tab sees it', () => {
	it('opens on the newest moment and puts its picture on the map', async () => {
		const server = core(() => view(3));
		const model = new RadarModel(server);
		expect(model.onMap).toEqual([]);
		await model.load();
		await model.showMoment();

		expect(model.moment?.clock).toBe('11:10');
		expect(model.shownMoment?.label).toBe('Rain radar, 11:10, 4 minutes ago');
		expect(model.onMap).toEqual([
			{ layer: 0, url: address(`${RADARS}/2.png veiled`), corners: corners(layer(3)), faded: false }
		]);
		expect(server.asked.pictures).toEqual([`${RADARS}/2.png veiled`]);
	});

	it('draws both layers of a moment, the analysis coloured under the radars’ picture, whose "not seen" is clear', async () => {
		const list = two();
		const server = core(() => list);
		const model = new RadarModel(server);
		await model.load();
		await model.showMoment();

		expect(model.shownMoment?.label).toBe(
			'Rain radar, 11:10, 4 minutes ago. The made-up analysis is of 11:00.'
		);
		// The lowest first, each a layer of its own with its own corners.
		expect(model.onMap).toEqual([
			{
				layer: 0,
				url: address(`${ANALYSIS}/1.png veiled under ${RADARS}/2.png`),
				corners: corners(list.layers[0]),
				faded: false
			},
			{ layer: 1, url: address(`${RADARS}/2.png`), corners: corners(list.layers[1]), faded: false }
		]);
		// The veil is drawn once, by the lowest: the radars' would hide the analysis' rain.
		// The lower is asked for under the upper's picture as the server sent it, the topmost under none.
		expect(server.asked.pictures.toSorted()).toEqual([
			`${ANALYSIS}/1.png veiled under ${RADARS}/2.png`,
			`${RADARS}/2.png`
		]);
		// Both are credited while both are drawn.
		expect(model.layersDrawn.map((drawnLayer) => drawnLayer.credit?.text)).toEqual([
			'Analysis by nobody',
			'Radars of nobody'
		]);
	});

	it('keeps a picture by its frame and the frames over it: the same analysis is asked for anew under another moment’s radars, and each once', async () => {
		const server = core(() => two());
		const model = new RadarModel(server);
		await model.load();
		await model.showMoment();
		server.asked.pictures.length = 0;

		model.choose(1);
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:05');
		// The analysis of 11:00 again, but clear where the radars of 11:05 see, not those of 11:10.
		expect(model.onMap.map((picture) => picture.url)).toEqual([
			address(`${ANALYSIS}/1.png veiled under ${RADARS}/1.png`),
			address(`${RADARS}/1.png`)
		]);
		expect(server.asked.pictures.toSorted()).toEqual([
			`${ANALYSIS}/1.png veiled under ${RADARS}/1.png`,
			`${RADARS}/1.png`
		]);

		// Back at the newest, and at the one before again: both are at hand.
		model.choose(2);
		await model.showMoment();
		expect(model.chosenTimeUnixS).toBeUndefined();
		expect(model.shownMoment?.clock).toBe('11:10');
		expect(model.onMap.map((picture) => picture.url)).toEqual([
			address(`${ANALYSIS}/1.png veiled under ${RADARS}/2.png`),
			address(`${RADARS}/2.png`)
		]);
		model.choose(1);
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:05');
		expect(server.asked.pictures).toHaveLength(2);
	});

	it('lets a picture go when the list no longer draws it, and keeps those it still draws', async () => {
		let count = 3;
		const server = core(() => {
			const list = two(count);
			// The oldest stop falls out of a longer list, as the server's does.
			return count > 3 ? { ...list, moments: list.moments.slice(1) } : list;
		});
		const model = new RadarModel(server);
		await model.load();
		model.choose(0);
		await model.showMoment();
		model.choose(1);
		await model.showMoment();
		server.asked.pictures.length = 0;

		count = 4;
		await model.load();
		// 11:05 is still a stop: its pictures are at hand.
		model.choose(0);
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:05');
		expect(server.asked.pictures).toEqual([]);
	});

	it('draws a layer that is alone at a moment with the veil, and nothing of the other', async () => {
		// The analysis was too old for the first moment: the core gives it no frame there.
		const list = two();
		list.moments[0] = { ...list.moments[0], frames: [{ layer: 1, frame: 0, drawsUnseen: true }] };
		const server = core(() => list);
		const model = new RadarModel(server);
		await model.load();
		model.choose(0);
		await model.showMoment();

		expect(model.onMap).toEqual([
			{
				layer: 1,
				url: address(`${RADARS}/0.png veiled`),
				corners: corners(list.layers[1]),
				faded: false
			}
		]);
		expect(model.layersDrawn.map((drawnLayer) => drawnLayer.source)).toEqual(['made-up']);
		expect(server.asked.pictures).toEqual([`${RADARS}/0.png veiled`]);
	});

	it('leaves the moment before on the map whole, under its own time, when a picture cannot be had', async () => {
		const server = core(() => two());
		const model = new RadarModel(server);
		await model.load();
		await model.showMoment();
		const before = model.onMap;

		server.failPicture(`${RADARS}/1.png`);
		model.choose(1);
		await model.showMoment();
		expect(model.moment?.clock).toBe('11:05');
		expect(model.shownMoment?.clock).toBe('11:10');
		// Not the analysis of one moment under the radars of another.
		expect(model.onMap).toEqual(before);
		expect(model.pictureProblem).toBe(`${WORDS.unreachable} lost on the way`);

		// "Try again" asks again: a failure is not kept.
		server.failPicture(undefined);
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:05');
		expect(model.pictureProblem).toBeUndefined();
	});

	it('goes on to a new moment only if the pilot was looking at the newest', async () => {
		let count = 3;
		const server = core(() => view(count));
		const model = new RadarModel(server);
		await model.load();
		await model.showMoment();

		count = 4;
		await model.load();
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:15');

		model.choose(1);
		await model.showMoment();
		count = 5;
		await model.load();
		await model.showMoment();
		expect(model.shownMoment?.clock).toBe('11:05');
	});

	it('says a shown moment’s age as the newest list has it', async () => {
		let nowMin = 14;
		const aged = () => {
			const list = view(3);
			list.moments = list.moments.map((moment, index) => ({
				...moment,
				label: `Rain radar, ${moment.clock}, ${nowMin - index * 5} minutes ago`
			}));
			return list;
		};
		const model = new RadarModel(core(aged));
		await model.load();
		await model.showMoment();
		expect(model.shownMoment?.label).toBe('Rain radar, 11:10, 4 minutes ago');

		nowMin = 20;
		await model.load();
		expect(model.shownMoment?.label).toBe('Rain radar, 11:10, 10 minutes ago');
	});

	it('takes the pictures off the map when the list has no frame any more', async () => {
		let count = 2;
		const model = new RadarModel(core(() => view(count)));
		await model.load();
		await model.showMoment();
		expect(model.onMap).toHaveLength(1);

		count = 0;
		await model.load();
		await model.showMoment();
		expect(model.onMap).toEqual([]);
		expect(model.shownMoment).toBeUndefined();
	});

	it('draws an old layer fainter, each layer by its own age', async () => {
		const server = core(() => view(2, { stale: true, staleWords: 'Old.' }));
		const model = new RadarModel(server);
		await model.load();
		await model.showMoment();
		expect(model.onMap.map((picture) => picture.faded)).toEqual([true]);
		expect(model.caution).toBe('Old.');

		// The analysis is behind and the radars are not: the analysis alone is faint.
		const behind = new RadarModel(
			core(() => two(3, { analysis: { stale: true, staleWords: 'The analysis is old.' } }))
		);
		await behind.load();
		await behind.showMoment();
		expect(behind.onMap.map((picture) => picture.faded)).toEqual([true, false]);
		// The card's one caution is the topmost layer's, which has none.
		expect(behind.caution).toBeUndefined();
	});

	it('says the caution of the topmost layer that has frames', async () => {
		const radarsOld = new RadarModel(
			core(() => two(3, { radars: { stale: true, staleWords: 'The radars are old.' } }))
		);
		await radarsOld.load();
		expect(radarsOld.caution).toBe('The radars are old.');

		// No radar picture yet: the analysis is the topmost that has frames, and it is current.
		const noRadar = new RadarModel(
			core(() =>
				two(3, { radars: { frames: [], stale: true, staleWords: 'No radar picture yet' } })
			)
		);
		await noRadar.load();
		expect(noRadar.caution).toBeUndefined();

		const none = new RadarModel(
			core(() => view(0, { stale: true, staleWords: 'No radar picture yet' }))
		);
		await none.load();
		expect(none.caution).toBe('No radar picture yet');
	});

	it('plays the moments once, the oldest first, and stops at the newest', async () => {
		const server = core(() => two(4));
		const seen: (string | undefined)[] = [];
		const waits: number[] = [];
		const model: RadarModel = new RadarModel(server, Date.now, async (ms) => {
			waits.push(ms);
			seen.push(model.moment?.clock);
		});
		await model.load();

		await model.play();

		expect(seen).toEqual(['11:00', '11:05', '11:10']);
		expect(waits).toEqual([250, 250, 250]);
		expect(model.moment?.clock).toBe('11:15');
		expect(model.chosenTimeUnixS).toBeUndefined();
		expect(model.playing).toBe(false);
		// Every picture was fetched before the first step: the radars' four, and the analysis' one under each.
		expect(server.asked.pictures.toSorted()).toEqual([
			`${ANALYSIS}/1.png veiled under ${RADARS}/0.png`,
			`${ANALYSIS}/1.png veiled under ${RADARS}/1.png`,
			`${ANALYSIS}/1.png veiled under ${RADARS}/2.png`,
			`${ANALYSIS}/1.png veiled under ${RADARS}/3.png`,
			`${RADARS}/0.png`,
			`${RADARS}/1.png`,
			`${RADARS}/2.png`,
			`${RADARS}/3.png`
		]);
	});

	it('ends a play when the pilot chooses a moment', async () => {
		const server = core(() => view(4));
		const model: RadarModel = new RadarModel(server, Date.now, async () => {
			if (model.moment?.clock === '11:05') model.choose(0);
		});
		await model.load();
		await model.play();
		expect(model.playing).toBe(false);
		expect(model.moment?.clock).toBe('11:00');
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
		expect(model.moments).toHaveLength(2);
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
