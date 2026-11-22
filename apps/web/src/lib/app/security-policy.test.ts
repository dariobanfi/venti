import { describe, expect, it } from 'vitest';
import { mapStyle, terrain3dSource } from '#lib/map/style.ts';
import { contentSecurityPolicy, TILE_HOSTS } from './security-policy';

const HASH = 'sha256-S8qMpvofolR8Mpjy4kQvEm7m1q8clzU4dfDH0AmvZjo=';
const policy = contentSecurityPolicy([HASH]);
const everySource = Object.values(policy).flat();

/** Every `https` address something names, however deep inside it. */
function addresses(thing: unknown): string[] {
	if (typeof thing === 'string') return thing.startsWith('https://') ? [thing] : [];
	if (thing && typeof thing === 'object') return Object.values(thing).flatMap(addresses);
	return [];
}

describe('the site’s content security policy', () => {
	it('allows nothing it does not name', () => {
		expect(policy['default-src']).toEqual(['none']);
	});

	it('runs no script but the app’s own files, and none made from text', () => {
		expect(policy['script-src']).toEqual(['self']);
		expect(everySource).not.toContain('unsafe-eval');
		expect(everySource).not.toContain('unsafe-inline');
	});

	it('names every host, and none by a pattern', () => {
		for (const source of everySource) {
			expect(source).not.toContain('*');
			// No scheme as a whole but `data:`, which reaches nobody.
			if (source.endsWith(':')) expect(source).toBe('data:');
		}
	});

	it('lets the map ask every address its style names, and no other host', () => {
		// At a domain, where the style has every source it can have.
		const style = mapStyle({ origin: 'https://venti.example' });
		const tiles = [style.sources, terrain3dSource()]
			.flatMap(addresses)
			.filter((address) => !address.startsWith('https://venti.example'));
		const asked = new Set(tiles.map((address) => new URL(address.replaceAll(/[{}]/g, '')).origin));

		expect([...asked].sort()).toEqual([...TILE_HOSTS].sort());
		expect(policy['connect-src']).toEqual(['self', 'data:', ...TILE_HOSTS]);
		expect(TILE_HOSTS).toEqual([
			'https://tiles.openfreemap.org',
			'https://tiles.mapterhorn.com',
			'https://tiles.maps.eox.at',
			'https://wmts.geo.admin.ch',
			'https://geoservices.buergernetz.bz.it',
			'https://owsproxy.lgl-bw.de',
			'https://wmtsod1.bayernwolke.de',
			'https://thermal.kk7.ch'
		]);
	});

	it('names nothing of Google’s: signing in with Google is a navigation', () => {
		expect(everySource.filter((source) => source.includes('google'))).toEqual([]);
		expect(policy).not.toHaveProperty('frame-src');
	});

	it('lets one inline style through, by its hash', () => {
		expect(policy['style-src']).toEqual(['self']);
		expect(policy['style-src-attr']).toEqual(['unsafe-hashes', HASH]);
	});

	it('sends no form and takes no base address', () => {
		expect(policy['form-action']).toEqual(['none']);
		expect(policy['base-uri']).toEqual(['none']);
	});
});
