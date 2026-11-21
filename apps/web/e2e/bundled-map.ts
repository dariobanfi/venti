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

/**
 * What stands in for a photograph of the aerial view: a small square of
 * one green, as a JPEG. The tests show that the map asks for photographs
 * and lays them where they belong, not what the providers' look like.
 */
const PHOTOGRAPH = Buffer.from(
	'/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAoHBwgHBgoICAgLCgoLDhgQDg0NDh0VFhEYIx8lJCIfIiEmKzcvJik0KSEiMEExNDk7Pj4+JS5ESUM8SDc9Pjv/2wBDAQoLCw4NDhwQEBw7KCIoOzs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozv/wAARCAAIAAgDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDBoooriOU//9k=',
	'base64'
);

/** The hosts of the aerial view's photographs, as the shared style names them. */
export const AERIAL_HOSTS = [
	'tiles.maps.eox.at',
	'wmts.geo.admin.ch',
	'geoservices.buergernetz.bz.it',
	'wmtsod1.bayernwolke.de'
];

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
	for (const host of AERIAL_HOSTS) {
		await context.route(`https://${host}/**`, (route) =>
			route.fulfill({ status: 200, contentType: 'image/jpeg', headers: ANY_PAGE, body: PHOTOGRAPH })
		);
	}
}
