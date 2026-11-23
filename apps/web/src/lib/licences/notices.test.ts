import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { BUNDLED_FROM_DEV, notices } from '../../../scripts/notices.mjs';

/** A folder of packages, as file name to text under each package's path. */
function folder(files: Record<string, Record<string, string>>) {
	return {
		list: (path: string) => Object.keys(files[path] ?? {}),
		read: (path: string) => {
			const at = path.lastIndexOf('/');
			return files[path.slice(0, at)][path.slice(at + 1)];
		}
	};
}

const MIT = 'MIT License\r\n\r\nCopyright (c) Someone\r\n';

describe('the notices of the site’s packages', () => {
	it('takes production packages and the named ones, each licence text once', () => {
		const lock = {
			packages: {
				'': { version: '0.1.0' },
				'node_modules/map': { version: '1.0.0', license: 'BSD-3-Clause' },
				'node_modules/@scope/a': { version: '2.0.0', license: 'MIT' },
				'node_modules/map/node_modules/b': { version: '3.0.0', license: 'MIT' },
				'node_modules/tool': { version: '4.0.0', license: 'MIT', dev: true },
				'node_modules/runtime': { version: '5.0.0', license: 'MIT', dev: true },
				'node_modules/native-other-system': { version: '6.0.0', license: 'MIT', optional: true },
				'node_modules/native-dev': { version: '7.0.0', license: 'MIT', devOptional: true }
			}
		};
		const { list, read } = folder({
			'node_modules/map': { 'LICENSE.txt': 'Copyright (c) Map\n\nBSD terms', 'README.md': '# Map' },
			'node_modules/@scope/a': { LICENSE: MIT },
			'node_modules/map/node_modules/b': { license: 'MIT License\n\nCopyright (c) Someone' },
			'node_modules/runtime': {
				'README.md':
					'# Runtime\n\nHow to use it.\n\n## License (MIT)\n\nCopyright Runtime\n\nTerms.\n\n## Thanks\n\nTo all.'
			}
		});
		const result = notices(lock, list, read, ['runtime']);
		expect(result.packages).toEqual([
			{ name: '@scope/a', version: '2.0.0', licence: 'MIT', text: 1 },
			{ name: 'b', version: '3.0.0', licence: 'MIT', text: 1 },
			{ name: 'map', version: '1.0.0', licence: 'BSD-3-Clause', text: 0 },
			{ name: 'runtime', version: '5.0.0', licence: 'MIT', text: 2 }
		]);
		expect(result.texts).toEqual([
			'Copyright (c) Map\n\nBSD terms',
			'MIT License\n\nCopyright (c) Someone',
			'Copyright Runtime\n\nTerms.'
		]);
	});

	it('refuses a package without a licence text, and a named one the lock file lacks', () => {
		const lock = { packages: { 'node_modules/bare': { version: '1.0.0', license: 'MIT' } } };
		const { list, read } = folder({ 'node_modules/bare': { 'index.js': '' } });
		expect(() => notices(lock, list, read, ['gone'])).toThrow(
			/bare 1\.0\.0: no licence file[\s\S]*gone: not in package-lock\.json/
		);
	});

	it('covers every production package the lock file installs, MapLibre among them', () => {
		const web = new URL('../../../', import.meta.url);
		const lock = JSON.parse(readFileSync(new URL('package-lock.json', web), 'utf8')) as {
			packages: Record<string, { dev?: boolean; devOptional?: boolean; optional?: boolean }>;
		};
		const at = (path: string) => new URL(path, web);
		const result = notices(
			lock,
			(path) => (existsSync(at(path)) ? readdirSync(at(path)) : []),
			(path) => readFileSync(at(path), 'utf8')
		);
		const production = Object.entries(lock.packages)
			.filter(([path, entry]) => path !== '' && !entry.dev && !entry.devOptional)
			.filter(([path, entry]) => !entry.optional || existsSync(at(path)))
			.map(([path]) => path.replace(/^.*node_modules\//, ''));
		const named = result.packages.map((each) => each.name);
		expect(named).toEqual(expect.arrayContaining([...production, ...BUNDLED_FROM_DEV]));
		const maplibre = result.packages.find((each) => each.name === 'maplibre-gl');
		expect(maplibre?.licence).toBe('BSD-3-Clause');
		expect(result.texts[maplibre!.text]).toMatch(/^Copyright \(c\) \d{4}, MapLibre contributors/);
	});
});
