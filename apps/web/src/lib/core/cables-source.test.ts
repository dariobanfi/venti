import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { CablesSource } from './cables-source';
import { startCore } from './testing';
import type { Get } from './wind-source';

beforeAll(startCore);

/** The server's own recorded answer: eight cables around the Tegernsee. */
const recorded = (): Uint8Array =>
	readFileSync(new URL('../../../../../crates/server/tests/golden/cables.json', import.meta.url));

describe('the cables around a place, from the server', () => {
	it('are asked for in a box around the place, and read as lines with their words', async () => {
		const asked: string[] = [];
		const get: Get = async (path) => {
			asked.push(path);
			return { status: 200, body: recorded() };
		};
		const layer = (await new CablesSource(async () => true, get).around({
			lat: 47.7,
			lon: 11.76
		}))!;

		expect(asked).toHaveLength(1);
		expect(asked[0]).toMatch(/^\/v1\/cables\?bbox=/);
		expect(layer.lines).toHaveLength(8);
		expect(layer.lines.map((line) => line.words)).toContain(
			'Power line, 110 kV, height not mapped'
		);
		expect(layer.lines.every((line) => !line.heightMapped)).toBe(true);
		expect(layer.lines.find((line) => line.name === 'Wallbergbahn')?.kind).toBe('gondola');
		expect(layer.credits.map((credit) => credit.text)).toEqual(['© OpenStreetMap contributors']);
		expect(layer.limits).toBe('Not every cable is mapped, and few have a height.');
	});

	it('are not asked for where the site has no server', async () => {
		const get: Get = async () => {
			throw new Error('asked');
		};
		expect(await new CablesSource(async () => false, get).around({ lat: 47.7, lon: 11.76 })).toBe(
			undefined
		);
	});
});
