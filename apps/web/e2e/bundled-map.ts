/**
 * Answers the map's requests for tiles from the small region stored in the
 * repository (the Tegernsee valley, where the sample flight takes place),
 * so the tests draw a real map without the network.
 *
 * The app is not told. It asks OpenFreeMap and Mapterhorn as it always
 * does; the test browser hands each request to this file instead. What the
 * tests see is therefore the app as it ships.
 *
 * The region is the one the iPhone app bundles for its own tests:
 * `apps/ios/Venti/Resources/MapDemo`, built by `tools/demo-map/build.sh`.
 */

import { open, type FileHandle } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import type { BrowserContext, Route } from '@playwright/test';
import { PMTiles, type RangeResponse, type Source } from 'pmtiles';

const REGION = new URL('../../ios/Venti/Resources/MapDemo/', import.meta.url);

/** A tile archive on disk, read a piece at a time as the library asks. */
class ArchiveFile implements Source {
	#path: string;
	#file: Promise<FileHandle>;

	constructor(name: string) {
		this.#path = fileURLToPath(new URL(name, REGION));
		this.#file = open(this.#path, 'r');
	}

	getKey(): string {
		return this.#path;
	}

	async getBytes(offset: number, length: number): Promise<RangeResponse> {
		const buffer = Buffer.alloc(length);
		const { bytesRead } = await (await this.#file).read(buffer, 0, length, offset);
		return { data: buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + bytesRead) };
	}
}

const basemap = new PMTiles(new ArchiveFile('basemap.pmtiles'));
const terrain = new PMTiles(new ArchiveFile('terrain.pmtiles'));

/** What OpenFreeMap says about its tiles, with its own credits, pointing at the address answered below. */
const BASEMAP_DESCRIPTION = {
	tilejson: '3.0.0',
	name: 'OpenFreeMap',
	tiles: ['https://tiles.openfreemap.org/planet/bundled/{z}/{x}/{y}.pbf'],
	minzoom: 0,
	maxzoom: 14,
	attribution:
		'<a href="https://openfreemap.org" target="_blank">OpenFreeMap</a> <a href="https://www.openmaptiles.org/" target="_blank">&copy; OpenMapTiles</a> Data from <a href="https://www.openstreetmap.org/copyright" target="_blank">OpenStreetMap</a>'
};

const ANY_PAGE = { 'access-control-allow-origin': '*' };

async function answerWithTile(route: Route, archive: PMTiles, contentType: string): Promise<void> {
	const match = /\/(\d+)\/(\d+)\/(\d+)\.\w+$/.exec(new URL(route.request().url()).pathname);
	const tile = match
		? await archive.getZxy(Number(match[1]), Number(match[2]), Number(match[3]))
		: undefined;
	if (!tile) {
		// Outside the stored region. "No content" is how a tile server says
		// that a tile is empty, and the map draws it as such.
		await route.fulfill({ status: 204, headers: ANY_PAGE });
		return;
	}
	await route.fulfill({
		status: 200,
		contentType,
		headers: ANY_PAGE,
		body: Buffer.from(tile.data)
	});
}

/**
 * Makes every page of `context` draw its map from the stored region. Any
 * other request that would leave the test machine is refused, so a test
 * cannot come to depend on the network by accident.
 */
export async function serveBundledMap(context: BrowserContext): Promise<void> {
	await context.route(/^https?:\/\/(?!localhost[:/])/, (route) => route.abort('blockedbyclient'));
	await context.route('https://tiles.openfreemap.org/planet', (route) =>
		route.fulfill({ json: BASEMAP_DESCRIPTION, headers: ANY_PAGE })
	);
	await context.route('https://tiles.openfreemap.org/planet/bundled/**', (route) =>
		answerWithTile(route, basemap, 'application/x-protobuf')
	);
	await context.route('https://tiles.mapterhorn.com/**', (route) =>
		answerWithTile(route, terrain, 'image/webp')
	);
}
