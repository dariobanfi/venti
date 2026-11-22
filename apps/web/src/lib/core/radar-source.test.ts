import { readFileSync } from 'node:fs';
import { crc32, deflateSync, inflateSync } from 'node:zlib';
import { beforeAll, describe, expect, it } from 'vitest';
import { isInWords, type RadarView } from './index';
import { RADAR_UNREACHABLE, RadarSource } from './radar-source';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: the
 * first region's two sources, DWD's radars and GeoSphere Austria's
 * analysis, recorded within minutes of each other on 21 November 2026.
 * Which frames it has and when it was answered are read from it, so that
 * a new recording needs no new figures here.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/radar.json', import.meta.url));

interface Recorded {
	time: string;
	layers: { source: string; frames: { time: string; path: string }[] }[];
}

const RECORDING = JSON.parse(new TextDecoder().decode(recorded())) as Recorded;

const seconds = (time: string) => Date.parse(time) / 1000;

/** The moment the recording was answered at, which the ages are counted from. */
const AT = seconds(RECORDING.time);

/** The recorded frames of a source, the oldest first. */
function recordedFrames(source: string) {
	return RECORDING.layers.find((layer) => layer.source === source)!.frames;
}

const RADARS = 'dwd-radar-ry';
const ANALYSIS = 'geosphere-inca-rr';

/** A moment on the clock of this process: "14:15" for 13:15 UTC in Berlin's winter. */
function onTheClock(unixS: number): string {
	const at = new Date(unixS * 1000);
	const two = (value: number) => String(value).padStart(2, '0');
	return `${two(at.getHours())}:${two(at.getMinutes())}`;
}

/** Whole minutes from a moment to another. */
const minutes = (from: number, to: number) => Math.floor((to - from) / 60);

function chunk(name: string, data: Uint8Array): Uint8Array {
	const body = Buffer.concat([Buffer.from(name, 'latin1'), data]);
	const out = Buffer.alloc(body.length + 8);
	out.writeUInt32BE(data.length, 0);
	body.copy(out, 4);
	out.writeUInt32BE(crc32(body), body.length + 4);
	return out;
}

/** The chunks of a PNG: each name with its bytes. */
function chunks(png: Uint8Array): Record<string, number[]> {
	const found: Record<string, number[]> = {};
	const bytes = Buffer.from(png);
	for (let at = 8; at < bytes.length;) {
		const length = bytes.readUInt32BE(at);
		found[bytes.toString('latin1', at + 4, at + 8)] = [...bytes.subarray(at + 8, at + 8 + length)];
		at += length + 12;
	}
	return found;
}

/**
 * A picture as the server sends one, two pixels by two: no rain, the third
 * step, not seen, the sixth step, unless other pixels are given. Its
 * palette is the server's plain one.
 */
function picture(pixels: [number, number, number, number] = [0, 3, 7, 6]): Uint8Array {
	const [one, two, three, four] = pixels;
	return Buffer.concat([
		Buffer.from('\x89PNG\r\n\x1a\n', 'latin1'),
		chunk('IHDR', Uint8Array.from([0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0])),
		chunk('PLTE', new Uint8Array(24).fill(128)),
		chunk('tRNS', Uint8Array.from([0, 255, 255, 255, 255, 255, 255, 48])),
		chunk('IDAT', deflateSync(Uint8Array.from([0, one, two, 0, three, four]))),
		chunk('IEND', new Uint8Array())
	]);
}

/** The pixels of a picture two by two, row by row, as long as its rows are written plainly. */
function pixels(png: Uint8Array): number[] {
	const [first, one, two, second, three, four] = inflateSync(Uint8Array.from(chunks(png).IDAT));
	expect([first, second]).toEqual([0, 0]);
	return [one, two, three, four];
}

function server(answers: Record<string, () => Answer>) {
	const asked: string[] = [];
	const get: Get = async (path) => {
		asked.push(path);
		const answer = answers[path];
		if (!answer) throw new Error(`not asked for in this test: ${path}`);
		return answer();
	};
	return { asked, get };
}

const LIST = '/v1/radar';
/** A frame's path, for the tests of a picture, which answer it themselves. */
const FRAME = '/v1/radar/dwd-radar-ry/20261121T1140Z.png';
/** The paths of a frame that lies under it at a moment, and of one that lies between the two. */
const UNDER = '/v1/radar/geosphere-inca-rr/20261121T1130Z.png';
const BETWEEN = '/v1/radar/made-up/20261121T1135Z.png';
const COLOURS = {
	rain: [0xaab4b8ff, 0x9ad8d2ff, 0x4db8acff, 0x1f8a80ff, 0x7a4fc9ff, 0x4b259eff],
	unseen: 0x6c5f8cff
};

describe('the rain radar from the server', () => {
	it('is asked for as a list, and read by the core with each layer’s frames, their times and ages', async () => {
		const { asked, get } = server({ [LIST]: () => ({ status: 200, body: recorded() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const view = (await source.radar())!;

		expect(asked).toEqual([LIST]);
		// A layer for each source, in the order to draw them: the analysis
		// at the bottom, which draws the veil, and the radars on top.
		expect(view.layers.map((layer) => [layer.source, layer.what, layer.drawsUnseen])).toEqual([
			[ANALYSIS, "Austria's analysis of radar and gauges, every 15 minutes", true],
			[RADARS, "Germany's radars, every 5 minutes", false]
		]);
		const [analysis, radars] = view.layers;
		expect(radars).toMatchObject({
			west: 9.4,
			east: 17.2,
			north: 48.5,
			stale: false,
			staleWords: ''
		});
		for (const layer of view.layers) {
			const frames = recordedFrames(layer.source);
			expect(layer.frames.map((frame) => [frame.path, frame.clock, frame.age])).toEqual(
				frames.map((frame) => [
					frame.path,
					onTheClock(seconds(frame.time)),
					`${minutes(seconds(frame.time), AT)} minutes ago`
				])
			);
			expect(layer.line).toBe(layer.frames.at(-1)!.label);
		}
		expect(radars.line).toMatch(/^Rain radar, \d\d:\d\d, \d+ minutes ago$/);
		expect(analysis.line).toMatch(/^Rain analysis, \d\d:\d\d, \d+ minutes ago$/);
		// Each with its own credit.
		expect(radars.credit?.text).toBe(
			'Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben'
		);
		expect(analysis.credit?.text).toBe(
			'Data source: GeoSphere Austria - https://data.hub.geosphere.at'
		);
		// The key from the most rain down, each step with the scale its colour is of.
		expect(view.key.map((line) => line.step)).toEqual([6, 5, 4, 3, 2, 1]);
		expect(view.key[0]).toMatchObject({ scale: 'wind', scaleStep: 7 });
		expect(view.key[5]).toMatchObject({ words: 'A trace, under 0.2 mm an hour', scale: 'vario' });
		expect(view.unseenWords).toBe('Not seen by the radar');
		expect(view.note).toContain('no forecast');
		expect(view.note).toContain("the radar's picture lies on top");
	});

	it('has one line of time for both layers: the radars’ stops, each with what lies on the map at it', async () => {
		const { get } = server({ [LIST]: () => ({ status: 200, body: recorded() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const view = (await source.radar())!;

		// A stop for each of the radars' frames, the oldest first, opening on the newest.
		const stops = recordedFrames(RADARS);
		expect(view.moments.map((moment) => [moment.timeUnixS, moment.clock])).toEqual(
			stops.map((frame) => [seconds(frame.time), onTheClock(seconds(frame.time))])
		);
		expect(view.firstMoment).toBe(stops.length - 1);
		const [analysis, radars] = view.layers;
		for (const [at, moment] of view.moments.entries()) {
			// The radars' own frame of the stop on top, its "not seen" clear.
			expect(moment.frames.at(-1)).toEqual({ layer: 1, frame: at, drawsUnseen: false });
			// Under it the analysis' newest frame that had ended by then, with the veil.
			const under = recordedFrames(ANALYSIS).findLastIndex(
				(frame) => seconds(frame.time) <= moment.timeUnixS
			);
			expect(moment.frames).toHaveLength(2);
			expect(moment.frames[0]).toEqual({ layer: 0, frame: under, drawsUnseen: true });
			// The line is the top picture's, and names the one under it where it is of another moment.
			const below = analysis.frames[under];
			expect(moment.label).toBe(
				below.timeUnixS === moment.timeUnixS
					? radars.frames[at].label
					: `${radars.frames[at].label}. Austria's analysis is of ${below.clock}.`
			);
		}
	});

	it('says the ages again as time goes by, without asking the server', async () => {
		let now = AT;
		const { asked, get } = server({ [LIST]: () => ({ status: 200, body: recorded() }) });
		const source = new RadarSource(
			async () => true,
			async () => now,
			get
		);
		expect(await source.again()).toBeUndefined();
		await source.radar();
		const newest = (source: string) => seconds(recordedFrames(source).at(-1)!.time);
		const radars = (view: RadarView) => view.layers.find((layer) => layer.source === RADARS)!;
		const analysis = (view: RadarView) => view.layers.find((layer) => layer.source === ANALYSIS)!;

		now = AT + 6 * 60;
		const later = (await source.again())!;
		expect(radars(later).frames.at(-1)!.age).toBe(`${minutes(newest(RADARS), now)} minutes ago`);
		expect(later.moments.at(-1)!.label).toContain(`${minutes(newest(RADARS), now)} minutes ago`);

		// Past a quarter of an hour the radars' newest picture is not the rain
		// of now; the analysis is judged by its own three quarters of an hour.
		now = Math.max(newest(RADARS) + 25 * 60, AT);
		const old = (await source.again())!;
		expect(radars(old).stale).toBe(true);
		expect(radars(old).staleWords).toBe(
			`The newest radar picture is ${minutes(newest(RADARS), now)} minutes old. It is not the rain of now.`
		);
		now = Math.max(newest(ANALYSIS) + 45 * 60, AT);
		expect(analysis((await source.again())!).stale).toBe(false);
		now = Math.max(newest(ANALYSIS) + 46 * 60, AT);
		expect(analysis((await source.again())!).staleWords).toBe(
			`The newest picture of the analysis is ${minutes(newest(ANALYSIS), now)} minutes old. It is not the rain of now.`
		);
		expect(asked).toEqual([LIST]);
	});

	it('hands a frame’s picture to the core for the app’s colours', async () => {
		const { asked, get } = server({ [FRAME]: () => ({ status: 200, body: picture() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const png = await source.picture(FRAME, [], COLOURS);

		expect(asked).toEqual([FRAME]);
		const [before, after] = [chunks(picture()), chunks(png)];
		// No rain is clear, then the six steps and the veil, each as solid as given.
		expect(after.PLTE.slice(0, 6)).toEqual([0, 0, 0, 0xaa, 0xb4, 0xb8]);
		expect(after.PLTE.slice(21)).toEqual([0x6c, 0x5f, 0x8c]);
		expect(after.tRNS).toEqual([0, 255, 255, 255, 255, 255, 255, 255]);
		// The pixels are what the server sent.
		expect(after.IHDR).toEqual(before.IHDR);
		expect(after.IDAT).toEqual(before.IDAT);
	});

	it('writes "not seen" fully clear into a picture that lies over another', async () => {
		const { get } = server({ [FRAME]: () => ({ status: 200, body: picture() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const png = await source.picture(FRAME, [], { ...COLOURS, unseen: 0 });

		const after = chunks(png);
		// The rain as solid as in any picture, and nothing where the source saw nothing.
		expect(after.tRNS).toEqual([0, 255, 255, 255, 255, 255, 255, 0]);
		expect(after.PLTE.slice(3, 6)).toEqual([0xaa, 0xb4, 0xb8]);
		expect(after.IDAT).toEqual(chunks(picture()).IDAT);
	});

	it('makes a picture clear wherever a picture over it sees, rain or dry, and leaves it where none does', async () => {
		// Under: rain, rain, not seen, rain. Over it: dry, not seen, rain, not seen.
		const { asked, get } = server({
			[UNDER]: () => ({ status: 200, body: picture([2, 3, 7, 6]) }),
			[FRAME]: () => ({ status: 200, body: picture([0, 7, 4, 7]) })
		});
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const png = await source.picture(UNDER, [FRAME], COLOURS);

		// Both are asked for as the server sends them.
		expect(asked.toSorted()).toEqual([FRAME, UNDER].toSorted());
		// Clear where the upper saw dry and where it saw rain, the veil of "not seen" too.
		expect(pixels(png)).toEqual([0, 3, 0, 6]);
		// The colours are those of any picture.
		const after = chunks(png);
		expect(after.PLTE.slice(0, 6)).toEqual([0, 0, 0, 0xaa, 0xb4, 0xb8]);
		expect(after.tRNS).toEqual([0, 255, 255, 255, 255, 255, 255, 255]);

		// The upper one itself, the topmost, is as the server sent it, and is not asked for again.
		const top = await source.picture(FRAME, [], COLOURS);
		expect(chunks(top).IDAT).toEqual(chunks(picture([0, 7, 4, 7])).IDAT);
		expect(asked).toHaveLength(2);
	});

	it('makes a picture clear under every picture over it', async () => {
		const { get } = server({
			[UNDER]: () => ({ status: 200, body: picture([2, 3, 4, 6]) }),
			[BETWEEN]: () => ({ status: 200, body: picture([1, 7, 7, 7]) }),
			[FRAME]: () => ({ status: 200, body: picture([7, 7, 0, 7]) })
		});
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		expect(pixels(await source.picture(UNDER, [BETWEEN, FRAME], COLOURS))).toEqual([0, 3, 0, 6]);
		// The one between is clear where the topmost saw dry: its own "not seen" there too.
		expect(pixels(await source.picture(BETWEEN, [FRAME], COLOURS))).toEqual([1, 7, 0, 7]);
	});

	it('asks again for a picture the server refused, and has no picture under a refusal', async () => {
		const refusal = new TextEncoder().encode('{"error":{"code":"not_found","message":"x"}}');
		let there = false;
		const { asked, get } = server({
			[UNDER]: () => ({ status: 200, body: picture() }),
			[FRAME]: () => (there ? { status: 200, body: picture() } : { status: 404, body: refusal })
		});
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const failure = await source.picture(UNDER, [FRAME], COLOURS).catch((error: unknown) => error);
		expect(isInWords(failure)).toBe(true);
		expect((failure as Error).message).toBe('The radar picture could not be read.');

		there = true;
		expect(pixels(await source.picture(UNDER, [FRAME], COLOURS))).toEqual([0, 0, 7, 0]);
		// The refused one was asked for again, the one that came was not.
		expect(asked).toEqual([UNDER, FRAME, FRAME]);
	});

	it('lets go of the pictures of frames the server no longer lists', async () => {
		const listed = recordedFrames(RADARS).at(-1)!.path;
		const { asked, get } = server({
			[LIST]: () => ({ status: 200, body: recorded() }),
			[listed]: () => ({ status: 200, body: picture() }),
			[FRAME]: () => ({ status: 200, body: picture() })
		});
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);
		await source.picture(listed, [], COLOURS);
		await source.picture(FRAME, [], COLOURS);

		await source.radar();
		await source.picture(listed, [], COLOURS);
		await source.picture(FRAME, [], COLOURS);

		expect(asked).toEqual([listed, FRAME, LIST, FRAME]);
	});

	it('says in the core’s words that a refusal is no picture', async () => {
		const refusal = new TextEncoder().encode('{"error":{"code":"not_found","message":"x"}}');
		const { get } = server({ [FRAME]: () => ({ status: 404, body: refusal }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const failure = await source.picture(FRAME, [], COLOURS).catch((error: unknown) => error);

		expect(isInWords(failure)).toBe(true);
		expect((failure as Error).message).toBe('The radar picture could not be read.');
	});

	it('says so when the server collects no radar', async () => {
		const none = new TextEncoder().encode('{"error":{"code":"no_radar","message":"x"}}');
		const { get } = server({ [LIST]: () => ({ status: 501, body: none }) });
		const source = new RadarSource(
			async () => true,
			async () => AT,
			get
		);

		const failure = await source.radar().catch((error: unknown) => error);

		expect(isInWords(failure)).toBe(true);
		expect((failure as Error).message).toBe('This server has no rain radar.');
		// Nothing was read, so there is nothing to say again.
		expect(await source.again()).toBeUndefined();
	});

	it('is nothing on a site without a server, which is not asked', async () => {
		const { asked, get } = server({});
		const source = new RadarSource(
			async () => false,
			async () => AT,
			get
		);
		expect(await source.radar()).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('has the shell’s words for a server that gave no answer', () => {
		expect(RADAR_UNREACHABLE).toBe('Venti could not be reached for the rain radar.');
	});
});
