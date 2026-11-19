import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { fontTable, fontUrl } from './fonts';
import { FONT_SCHEME, mapStyle, terrain3dSource } from './style';

describe('the map style prepared for the browser', () => {
	it('has no placeholder left in it', () => {
		const style = mapStyle();

		expect(JSON.stringify(style)).not.toContain('__');
		expect(style.version).toBe(8);
		expect(Object.keys(style.sources).sort()).toEqual(['basemap', 'terrain']);
	});

	it('fetches tiles from the services the iPhone app uses and keeps the terrain settings', () => {
		const { basemap, terrain } = mapStyle().sources;

		expect(basemap).toMatchObject({ type: 'vector', url: 'https://tiles.openfreemap.org/planet' });
		expect(terrain).toEqual({
			type: 'raster-dem',
			tiles: ['https://tiles.mapterhorn.com/{z}/{x}/{y}.webp'],
			maxzoom: 12,
			tileSize: 512,
			encoding: 'terrarium',
			attribution: expect.stringContaining('Mapterhorn')
		});
	});

	it('builds the 3D terrain from the same elevation tiles as the shading', () => {
		expect(terrain3dSource()).toEqual(mapStyle().sources.terrain);
	});

	it('does not change the shared style for whoever prepares it next', () => {
		const first = mapStyle();
		(first.sources.basemap as { url: string }).url = 'changed';

		expect(mapStyle().sources.basemap).toMatchObject({
			url: 'https://tiles.openfreemap.org/planet'
		});
	});

	it('asks the app for its label fonts, not the network', () => {
		expect(mapStyle().glyphs).toBe(`${FONT_SCHEME}://{fontstack}/{range}.pbf`);
	});

	it('names only fonts the app has', () => {
		const table = fontTable();
		const fonts = new Set<string>();
		for (const layer of mapStyle().layers) {
			const stack = (layer.layout as { 'text-font'?: string[] } | undefined)?.['text-font'];
			for (const font of stack ?? []) fonts.add(font);
		}

		expect([...fonts]).toEqual(['noto-sans-regular']);
		for (const font of fonts) {
			for (const range of ['0-255', '256-511']) {
				expect(table.has(`${font}/${range}.pbf`), `${font}/${range}`).toBe(true);
			}
		}
	});
});

describe('label fonts', () => {
	it('are the files the iPhone app bundles', () => {
		const folder = new URL(
			'../../../../ios/Venti/Resources/MapDemo/fonts/noto-sans-regular/',
			import.meta.url
		);
		for (const range of ['0-255', '256-511']) {
			expect(existsSync(new URL(`${range}.pbf`, folder))).toBe(true);
		}
		expect([...fontTable().keys()].sort()).toEqual([
			'noto-sans-regular/0-255.pbf',
			'noto-sans-regular/256-511.pbf'
		]);
	});

	it('are found by what the map asks for', () => {
		const table = fontTable({
			'../../ios/fonts/noto-sans-regular/0-255.pbf': '/_app/assets/0-255.abc123.pbf'
		});

		expect(fontUrl('venti-font://noto-sans-regular/0-255.pbf', table)).toBe(
			'/_app/assets/0-255.abc123.pbf'
		);
		// MapLibre writes a list of fonts with an encoded comma.
		expect(fontUrl('venti-font://noto-sans-regular%2Cother/0-255.pbf', table)).toBeUndefined();
	});

	it('are simply absent for a script the app has no file for', () => {
		const table = fontTable();

		expect(fontUrl('venti-font://noto-sans-regular/1024-1279.pbf', table)).toBeUndefined();
		expect(fontUrl('https://example.org/noto-sans-regular/0-255.pbf', table)).toBeUndefined();
	});
});
