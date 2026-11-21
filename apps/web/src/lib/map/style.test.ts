import { existsSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';
import tokens from '../../../../../design/tokens/tokens.json';
import { fontLoader, fontTable, fontUrl } from './fonts';
import {
	AERIAL_LAYERS,
	appDomain,
	DRAWN_LAYERS,
	FONT_SCHEME,
	mapStyle,
	terrain3dSource,
	THERMAL_LAYERS
} from './style';

describe('the map style prepared for the browser', () => {
	it('has no placeholder left in it', () => {
		const style = mapStyle();

		expect(JSON.stringify(style)).not.toContain('__');
		expect(style.version).toBe(8);
		expect(Object.keys(style.sources).sort()).toEqual([
			'aerial-austria',
			'aerial-bavaria',
			'aerial-south-tyrol',
			'aerial-switzerland',
			'aerial-world',
			'basemap',
			'terrain'
		]);
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

	it('draws in the map colours of the design language, and in no others', () => {
		// The style is the light theme's map: `design/tokens`, map.light.
		const pack = new Set(Object.values(tokens.map.light).map((colour) => colour.$value));
		const drawn = new Set(JSON.stringify(mapStyle().layers).match(/#[0-9A-Fa-f]{3,8}/g));

		expect(drawn.size).toBeGreaterThan(5);
		expect([...drawn].filter((colour) => !pack.has(colour))).toEqual([]);
	});

	it('keeps the aerial view hidden until it is asked for, under every name', () => {
		const layers = mapStyle().layers as {
			id: string;
			type: string;
			source?: string;
			layout?: { visibility?: string };
		}[];
		const photographs = layers.filter((layer) => layer.type === 'raster');

		expect(AERIAL_LAYERS).toEqual([
			'aerial-world',
			'aerial-switzerland',
			'aerial-south-tyrol',
			'aerial-austria',
			'aerial-bavaria',
			'aerial-road',
			'aerial-aerialway-casing'
		]);
		expect(DRAWN_LAYERS).toEqual(['path', 'track', 'road-minor', 'road-major', 'rail']);
		expect(photographs.map((layer) => layer.id)).toEqual([
			'aerial-world',
			'aerial-switzerland',
			'aerial-south-tyrol',
			'aerial-austria',
			'aerial-bavaria'
		]);
		// The file stands in the drawn view: nothing of the photographs is fetched.
		for (const layer of layers) {
			expect(layer.layout?.visibility ?? 'visible', layer.id).toBe(
				AERIAL_LAYERS.includes(layer.id) ? 'none' : 'visible'
			);
		}
		// Shown, the photographs cover the drawn ground and nothing laid on it.
		const first = layers.findIndex((layer) => layer.type === 'raster');
		const last = layers.findLastIndex((layer) => layer.type === 'raster');
		expect(layers.slice(0, first).map((layer) => layer.id)).toEqual([
			'land',
			'landcover-grass',
			'landcover-wood',
			'landcover-rock',
			'landcover-ice',
			'landuse-residential',
			'hillshade',
			'waterway',
			'water',
			'building'
		]);
		expect([...new Set(layers.slice(last + 1).map((layer) => layer.type))].sort()).toEqual([
			'line',
			'symbol'
		]);
		// Every provider is named on the map while its photographs are shown.
		for (const layer of photographs) {
			const source = mapStyle().sources[layer.source ?? ''] as { attribution?: string };
			expect(source.attribution, layer.id).toBeTruthy();
		}
	});

	it('asks Venti itself for Austria’s photographs, which it cuts at the border', () => {
		const austria = mapStyle().sources['aerial-austria'] as {
			tiles: string[];
			attribution: string;
		};

		// Under the app's own address: here, where there is none, a path alone.
		expect(austria.tiles).toEqual(['/v1/imagery/basemap-at/{z}/{x}/{y}']);
		expect(austria.attribution).toContain('basemap.at');
	});

	it('has the thermal map only where the page has a domain to name to its author', () => {
		// Opened from this machine, or by a bare address: nobody to name,
		// so nothing of it is in the style and nothing is ever asked.
		for (const hostname of [undefined, 'localhost', '127.0.0.1']) {
			const style = mapStyle({ origin: 'http://127.0.0.1:8088', hostname });
			expect(style.sources.thermals, String(hostname)).toBeUndefined();
			expect(style.layers.some((layer) => layer.id === 'thermals')).toBe(false);
			expect(JSON.stringify(style)).not.toContain('__');
		}
		expect(appDomain('venti.example')).toBe('venti.example');

		// At a domain: every request says who asks, in the scheme and to the
		// zoom the author's server has, and the layer is hidden until the
		// pilot switches it on.
		const style = mapStyle({ origin: 'https://venti.example', hostname: 'venti.example' });
		expect(style.sources.thermals).toEqual({
			type: 'raster',
			scheme: 'tms',
			tiles: ['https://thermal.kk7.ch/tiles/thermals_all_all/{z}/{x}/{y}.png?src=venti.example'],
			tileSize: 256,
			maxzoom: 12,
			attribution: expect.stringContaining('thermal.kk7.ch')
		});
		expect(THERMAL_LAYERS).toEqual(['thermals']);
		const layer = style.layers.find((one) => one.id === 'thermals');
		expect(layer).toMatchObject({ type: 'raster', layout: { visibility: 'none' } });
		expect(JSON.stringify(style)).not.toContain('__');
		// It lies over the ground and the photographs and under every name.
		const at = style.layers.findIndex((one) => one.id === 'thermals');
		expect(style.layers.slice(at + 1).some((one) => one.type === 'fill')).toBe(false);
		expect(at).toBeGreaterThan(style.layers.findIndex((one) => one.id === 'aerial-bavaria'));
		// And Venti's own address is where Austria's photographs are asked for.
		expect((style.sources['aerial-austria'] as { tiles: string[] }).tiles).toEqual([
			'https://venti.example/v1/imagery/basemap-at/{z}/{x}/{y}'
		]);
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
	/** Every range of 256 characters up to 65,535, as the map names them. */
	const ranges = Array.from({ length: 256 }, (_, index) => `${index * 256}-${index * 256 + 255}`);

	it('are the files the iPhone app bundles, one for every range of characters', () => {
		const folder = new URL(
			'../../../../ios/Venti/Resources/MapDemo/fonts/noto-sans-regular/',
			import.meta.url
		);
		for (const range of ['0-255', '256-511', '8192-8447', '65280-65535']) {
			expect(existsSync(new URL(`${range}.pbf`, folder))).toBe(true);
		}
		expect([...fontTable().keys()].sort()).toEqual(
			ranges.map((range) => `noto-sans-regular/${range}.pbf`).sort()
		);
	});

	it('have a file for the dash between an airspace’s floor and its ceiling', () => {
		// "GND – 9000 ft": the en dash is character 8211.
		expect('–'.codePointAt(0)).toBe(8211);
		expect(fontUrl('venti-font://noto-sans-regular/8192-8447.pbf', fontTable())).toBeDefined();
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

	it('are simply absent for another font, a range beyond the files, or another address', () => {
		const table = fontTable();

		expect(fontUrl('venti-font://another-font/0-255.pbf', table)).toBeUndefined();
		expect(fontUrl('venti-font://noto-sans-regular/65536-65791.pbf', table)).toBeUndefined();
		expect(fontUrl('https://example.org/noto-sans-regular/0-255.pbf', table)).toBeUndefined();
	});
});

describe('what the map is answered when it asks for a font', () => {
	const table = fontTable({ '../fonts/noto-sans-regular/0-255.pbf': '/assets/0-255.abc.pbf' });
	const ask = (url: string) => fontLoader(table)({ url }, new AbortController());

	afterEach(() => vi.unstubAllGlobals());

	it('is the file, for a range the app has', async () => {
		const fetched: string[] = [];
		vi.stubGlobal('fetch', async (url: string) => {
			fetched.push(url);
			return new Response(new Uint8Array([1, 2, 3]));
		});

		const answer = await ask('venti-font://noto-sans-regular/0-255.pbf');

		expect(fetched).toEqual(['/assets/0-255.abc.pbf']);
		expect(new Uint8Array(answer.data)).toEqual(new Uint8Array([1, 2, 3]));
	});

	it('is no glyphs, and not an error, for a font or a range that is in no table', async () => {
		// Nothing is fetched for them either.
		vi.stubGlobal('fetch', async () => {
			throw new Error('Nothing should be fetched.');
		});

		for (const url of [
			'venti-font://another-font/0-255.pbf',
			'venti-font://noto-sans-regular/65536-65791.pbf',
			'venti-font://noto-sans-regular%2Cother/0-255.pbf'
		]) {
			expect((await ask(url)).data.byteLength, url).toBe(0);
		}
	});

	it('is an error when a file the app has cannot be fetched', async () => {
		vi.stubGlobal('fetch', async () => new Response('gone', { status: 404 }));

		await expect(ask('venti-font://noto-sans-regular/0-255.pbf')).rejects.toThrow(
			'The label font could not be loaded (404).'
		);
	});
});
