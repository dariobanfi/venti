import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { sources } from './manifests';

const crates = new URL('../../../../../crates/', import.meta.url);

/** Every manifest in the repository, read by hand, not through the build. */
function manifestFiles(): string[] {
	return readdirSync(crates)
		.filter((name) => name.startsWith('sources-'))
		.flatMap((name) =>
			readdirSync(new URL(`${name}/`, crates))
				.filter((file) => /^manifest.*\.toml$/.test(file))
				.map((file) => `${name}/${file}`)
		);
}

describe('sources', () => {
	it('has every manifest of the repository, each with its provider, licence, credit and link', () => {
		expect(sources).toHaveLength(manifestFiles().length);
		for (const source of sources) {
			expect(source.id, source.provider).toMatch(/^[a-z0-9-]+$/);
			expect(source.provider, source.id).not.toBe('');
			expect(source.licence.name, source.id).not.toBe('');
			expect(source.licence.credit, source.id).not.toBe('');
			expect(source.licence.link, source.id).toMatch(/^https:\/\//);
			expect(source.licence.verifiedOn, source.id).toMatch(/^\d{4}-\d{2}-\d{2}$/);
		}
	});

	it('reads what a manifest says as it says it', () => {
		const file = readFileSync(new URL('sources-geosphere/manifest.toml', crates), 'utf8');
		const id = /^id = "([^"]+)"/m.exec(file)?.[1];
		const geosphere = sources.find((source) => source.id === id);
		expect(geosphere?.provider).toBe('GeoSphere Austria');
		expect(geosphere?.licence.name).toBe('CC BY 4.0');
		// The manifest's `attribution` is the credit the server says with every answer.
		expect(geosphere?.licence.credit).toBe(/^attribution = "([^"]+)"/m.exec(file)?.[1]);
	});

	it('keeps the licence of what an adapter is built with apart from the source’s', () => {
		const withAsset = sources.filter((source) => source.assets.length > 0);
		expect(withAsset.length).toBeGreaterThan(0);
		for (const source of withAsset) {
			for (const asset of source.assets) {
				expect(asset.provider, source.id).not.toBe('');
				expect(asset.licence.name, source.id).not.toBe('');
			}
		}
	});
});
