/**
 * Helpers for unit tests that call the real core. They run in Node, where
 * the WebAssembly module is read from disk instead of fetched.
 */

import { readFileSync } from 'node:fs';
import { open } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { PMTiles, type RangeResponse, type Source } from 'pmtiles';
import { initSync } from './pkg/venti_core_wasm.js';
import type { GetTile } from './terrain-source';

let started = false;

/** Starts the core's WebAssembly module once for the test process. */
export function startCore(): void {
	if (started) return;
	const module = readFileSync(new URL('./pkg/venti_core_wasm_bg.wasm', import.meta.url));
	initSync({ module });
	started = true;
}

/**
 * The made-up sample flight, the one copy every app shares. It is written
 * by `cargo run -p venti-flight --example sample_flight`.
 */
export function sampleFlight(): string {
	const file = new URL('../../../../ios/Venti/Resources/sample.igc', import.meta.url);
	return readFileSync(file, 'utf8');
}

/** The sample flight on another day of July 2026, which makes it another flight. */
export function sampleFlightOn(day: number): string {
	return sampleFlight().replace('040726', `${String(day).padStart(2, '0')}0726`);
}

/**
 * Real elevation tiles: Mapterhorn's around the Tegernsee, where the sample
 * flight is, as the repository keeps them for the iPhone app's bundled
 * region. The archive is read here a piece at a time, and a tile cut from
 * it is what the tile server would send.
 */
const ARCHIVE = fileURLToPath(
	new URL('../../../../ios/Venti/Resources/MapDemo/terrain.pmtiles', import.meta.url)
);

class ArchiveFile implements Source {
	#file = open(ARCHIVE, 'r');

	getKey(): string {
		return ARCHIVE;
	}

	async getBytes(offset: number, length: number): Promise<RangeResponse> {
		const buffer = Buffer.alloc(length);
		const { bytesRead } = await (await this.#file).read(buffer, 0, length, offset);
		return { data: buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + bytesRead) };
	}
}

/** Opened by the first test that asks for a tile, not by every test that imports this file. */
let archive: PMTiles | undefined;

/** A tile server that answers from the archive and notes what it was asked. */
export function tileServer(
	instead: (address: string) => ReturnType<GetTile> | undefined = () => undefined
) {
	const asked: string[] = [];
	const get: GetTile = async (address) => {
		asked.push(address);
		const other = instead(address);
		if (other) return other;
		const [z, x, y] = /\/(\d+)\/(\d+)\/(\d+)\.webp$/.exec(address)!.slice(1).map(Number);
		const tile = await (archive ??= new PMTiles(new ArchiveFile())).getZxy(z, x, y);
		return tile
			? { status: 200, body: new Uint8Array(tile.data) }
			: { status: 404, body: new Uint8Array() };
	};
	return { asked, get };
}
