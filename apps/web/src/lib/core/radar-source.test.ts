import { readFileSync } from 'node:fs';
import { crc32, deflateSync } from 'node:zlib';
import { beforeAll, describe, expect, it } from 'vitest';
import { isInWords } from './index';
import { RADAR_UNREACHABLE, RadarSource } from './radar-source';
import { startCore } from './testing';
import type { Answer, Get } from './wind-source';

beforeAll(startCore);

/**
 * The server's own recorded answer, which its tests keep current: DWD's
 * composite over the first region with the frames of 11:35 and 11:40 UTC
 * on 21 November 2026, answered at 11:44.
 */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/radar.json', import.meta.url));

/** 11:44 UTC on 21 November 2026, the recording's own moment. */
const AT_1144 = Date.UTC(2026, 10, 21, 11, 44) / 1000;

/** How far the clock of this process is ahead of UTC at that moment, in minutes. */
const AHEAD_MIN = -new Date(AT_1144 * 1000).getTimezoneOffset();

/** A time of that day on the clock of this process: "11:40" in UTC. */
function onTheClock(hour: number, minute: number): string {
	const minutes = (((hour * 60 + minute + AHEAD_MIN) % 1440) + 1440) % 1440;
	const two = (value: number) => String(value).padStart(2, '0');
	return `${two(Math.floor(minutes / 60))}:${two(minutes % 60)}`;
}

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
 * step, not seen, the sixth step. Its palette is the server's plain one.
 */
function picture(): Uint8Array {
	return Buffer.concat([
		Buffer.from('\x89PNG\r\n\x1a\n', 'latin1'),
		chunk('IHDR', Uint8Array.from([0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0])),
		chunk('PLTE', new Uint8Array(24).fill(128)),
		chunk('tRNS', Uint8Array.from([0, 255, 255, 255, 255, 255, 255, 48])),
		chunk('IDAT', deflateSync(Uint8Array.from([0, 0, 3, 0, 7, 6]))),
		chunk('IEND', new Uint8Array())
	]);
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
const FRAME_1140 = '/v1/radar/dwd-radar-ry/20261121T1140Z.png';
const COLOURS = {
	rain: [0xaab4b8ff, 0x9ad8d2ff, 0x4db8acff, 0x1f8a80ff, 0x7a4fc9ff, 0x4b259eff],
	unseen: 0x6c5f8cff
};

describe('the rain radar from the server', () => {
	it('is asked for as a list, and read by the core with each frame’s time and age', async () => {
		const { asked, get } = server({ [LIST]: () => ({ status: 200, body: recorded() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT_1144,
			get
		);

		const view = (await source.radar())!;

		expect(asked).toEqual([LIST]);
		expect(view.layers).toHaveLength(1);
		const [layer] = view.layers;
		expect(layer).toMatchObject({
			source: 'dwd-radar-ry',
			west: 9.4,
			east: 17.2,
			north: 48.5,
			stale: false,
			staleWords: ''
		});
		expect(layer.frames.map((frame) => [frame.path, frame.clock, frame.age])).toEqual([
			['/v1/radar/dwd-radar-ry/20261121T1135Z.png', onTheClock(11, 35), '9 minutes ago'],
			[FRAME_1140, onTheClock(11, 40), '4 minutes ago']
		]);
		expect(layer.frames[1].label).toBe(`Rain radar, ${onTheClock(11, 40)}, 4 minutes ago`);
		expect(layer.line).toBe(layer.frames[1].label);
		expect(layer.credit?.text).toBe(
			'Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben'
		);
		// The key from the most rain down, each step with the scale its colour is of.
		expect(view.key.map((line) => line.step)).toEqual([6, 5, 4, 3, 2, 1]);
		expect(view.key[0]).toMatchObject({ scale: 'wind', scaleStep: 7 });
		expect(view.key[5]).toMatchObject({ words: 'A trace, under 0.2 mm an hour', scale: 'vario' });
		expect(view.unseenWords).toBe('Not seen by the radar');
		expect(view.note).toContain('It is no forecast');
	});

	it('says the ages again as time goes by, without asking the server', async () => {
		let now = AT_1144;
		const { asked, get } = server({ [LIST]: () => ({ status: 200, body: recorded() }) });
		const source = new RadarSource(
			async () => true,
			async () => now,
			get
		);
		expect(await source.again()).toBeUndefined();
		await source.radar();

		now += 6 * 60;
		const later = (await source.again())!;
		expect(later.layers[0].frames[1].age).toBe('10 minutes ago');
		expect(later.layers[0].stale).toBe(false);

		// Past a quarter of an hour the newest picture is not the rain of now.
		now += 15 * 60;
		const old = (await source.again())!;
		expect(old.layers[0].stale).toBe(true);
		expect(old.layers[0].staleWords).toBe(
			'The newest radar picture is 25 minutes old. It is not the rain of now.'
		);
		expect(asked).toEqual([LIST]);
	});

	it('hands a frame’s picture to the core for the app’s colours', async () => {
		const { asked, get } = server({ [FRAME_1140]: () => ({ status: 200, body: picture() }) });
		const source = new RadarSource(
			async () => true,
			async () => AT_1144,
			get
		);

		const png = await source.picture(FRAME_1140, COLOURS);

		expect(asked).toEqual([FRAME_1140]);
		const [before, after] = [chunks(picture()), chunks(png)];
		// No rain is clear, then the six steps and the veil, each as solid as given.
		expect(after.PLTE.slice(0, 6)).toEqual([0, 0, 0, 0xaa, 0xb4, 0xb8]);
		expect(after.PLTE.slice(21)).toEqual([0x6c, 0x5f, 0x8c]);
		expect(after.tRNS).toEqual([0, 255, 255, 255, 255, 255, 255, 255]);
		// The pixels are what the server sent.
		expect(after.IHDR).toEqual(before.IHDR);
		expect(after.IDAT).toEqual(before.IDAT);
	});

	it('says in the core’s words that a refusal is no picture', async () => {
		const refusal = new TextEncoder().encode('{"error":{"code":"not_found","message":"x"}}');
		const { get } = server({ [FRAME_1140]: () => ({ status: 404, body: refusal }) });
		const source = new RadarSource(
			async () => true,
			async () => AT_1144,
			get
		);

		const failure = await source.picture(FRAME_1140, COLOURS).catch((error: unknown) => error);

		expect(isInWords(failure)).toBe(true);
		expect((failure as Error).message).toBe('The radar picture could not be read.');
	});

	it('says so when the server collects no radar', async () => {
		const none = new TextEncoder().encode('{"error":{"code":"no_radar","message":"x"}}');
		const { get } = server({ [LIST]: () => ({ status: 501, body: none }) });
		const source = new RadarSource(
			async () => true,
			async () => AT_1144,
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
			async () => AT_1144,
			get
		);
		expect(await source.radar()).toBeUndefined();
		expect(asked).toEqual([]);
	});

	it('has the shell’s words for a server that gave no answer', () => {
		expect(RADAR_UNREACHABLE).toBe('Venti could not be reached for the rain radar.');
	});
});
