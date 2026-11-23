import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { readToml, sources } from './manifests';

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

describe('readToml', () => {
	it('reads strings, numbers, true and false, lists over lines, tables and lists of tables', () => {
		expect(
			readToml(
				'id = "a" # a comment\nmax = 6_144\n[licence]\nname = "CC BY 4.0"\ncommercial = false\n' +
					'notice = "say \\"so\\""\nlist = [\n  "one", # first\n  \'two\',\n]\n' +
					'[[asset]]\nname = "x"\n[asset.licence]\nname = "y"\n'
			)
		).toEqual({
			id: 'a',
			max: 6144,
			licence: { name: 'CC BY 4.0', commercial: false, notice: 'say "so"', list: ['one', 'two'] },
			asset: [{ name: 'x', licence: { name: 'y' } }]
		});
	});
});

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
