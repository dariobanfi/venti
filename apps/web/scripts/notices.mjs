// Writes the notices of the npm packages whose code goes into the built site:
// each with its version, its licence and the full text of its licence, as
// the package itself carries it. The "Open-source licences" page shows them
// (src/routes/developers/data/open-source); the build stops if a package in
// the built site is not among them (vite.config.ts, `noticesCover`).
//
//   node scripts/notices.mjs      (`npm run build` and `npm run dev` run it first)
//
// The packages are those package-lock.json installs for the site's own
// `dependencies`, and the few that are devDependencies only because they
// compile the site, while their runtime goes into it (BUNDLED_FROM_DEV).
// Node's standard library only.

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * devDependencies whose code the built site carries all the same: Svelte's
 * and SvelteKit's runtime, and what that imports. The build names any that
 * is missing here.
 */
export const BUNDLED_FROM_DEV = ['svelte', '@sveltejs/kit', 'clsx'];

/** Where the page reads the notices from. Not committed: written before every build. */
export const OUT = 'src/lib/licences/npm-notices.json';

/**
 * The notices of the packages a lock file installs for production, and of
 * those named in `extra`.
 *
 * @param {{ packages: Record<string, { version?: string, license?: string, dev?: boolean, devOptional?: boolean, optional?: boolean }> }} lock
 * @param {(path: string) => string[]} list the names of the files in a package's folder
 * @param {(path: string) => string} read a file's text
 * @param {string[]} [extra] packages to take although the lock file has them for development only
 * @returns {{ packages: { name: string, version: string, licence: string, text: number }[], texts: string[] }}
 */
export function notices(lock, list, read, extra = BUNDLED_FROM_DEV) {
	/** @type {string[]} */
	const texts = [];
	/** @type {{ name: string, version: string, licence: string, text: number }[]} */
	const packages = [];
	/** @type {string[]} */
	const problems = [];
	for (const [path, entry] of Object.entries(lock.packages)) {
		if (path === '') continue;
		const name = path.replace(/^.*node_modules\//, '');
		if ((entry.dev || entry.devOptional) && !extra.includes(name)) continue;
		const files = list(path);
		// An optional package for another system is not installed, and not in the site.
		if (files.length === 0 && entry.optional) continue;
		const text = licenceText(path, files, read);
		if (text === undefined) {
			problems.push(`${name} ${entry.version}: no licence file and no licence in its README`);
			continue;
		}
		let index = texts.indexOf(text);
		if (index < 0) index = texts.push(text) - 1;
		packages.push({
			name,
			version: entry.version ?? '',
			licence: entry.license ?? 'no licence stated',
			text: index
		});
	}
	const missing = extra.filter((name) => !packages.some((each) => each.name === name));
	for (const name of missing) problems.push(`${name}: not in package-lock.json`);
	if (problems.length > 0) throw new Error(problems.join('\n'));
	packages.sort((a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version));
	return { packages, texts };
}

/**
 * A package's licence text: its LICENSE (or LICENCE, COPYING) file, or else
 * the "License" section of its README, as some small packages keep it.
 *
 * @param {string} path
 * @param {string[]} files the names of the files in the package's folder
 * @param {(path: string) => string} read
 * @returns {string | undefined}
 */
function licenceText(path, files, read) {
	const licence = files.filter((file) => /^(licen[cs]e|copying)/i.test(file)).sort()[0];
	if (licence) return tidy(read(`${path}/${licence}`));
	const readme = files.find((file) => /^readme(\.md)?$/i.test(file));
	if (!readme) return undefined;
	const lines = tidy(read(`${path}/${readme}`)).split('\n');
	const start = lines.findIndex((line) => /^#+\s*licen[cs]e\b/i.test(line));
	if (start < 0) return undefined;
	const level = /^#+/.exec(lines[start])?.[0].length ?? 1;
	const after = lines.slice(start + 1);
	const end = after.findIndex((line) => {
		const heading = /^#+(?=\s)/.exec(line);
		return heading !== null && heading[0].length <= level;
	});
	const section = tidy((end < 0 ? after : after.slice(0, end)).join('\n'));
	return section === '' ? undefined : section;
}

/** @param {string} text */
function tidy(text) {
	return text.replace(/\r\n?/g, '\n').trim();
}

// Run as a script: from the lock file and the installed packages.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const web = join(dirname(fileURLToPath(import.meta.url)), '..');
	const lock = JSON.parse(readFileSync(join(web, 'package-lock.json'), 'utf8'));
	const at = (/** @type {string} */ path) => join(web, path);
	try {
		const result = notices(
			lock,
			(path) => (existsSync(at(path)) ? readdirSync(at(path)) : []),
			(path) => readFileSync(at(path), 'utf8')
		);
		mkdirSync(dirname(at(OUT)), { recursive: true });
		writeFileSync(at(OUT), JSON.stringify(result) + '\n');
		console.log(
			`${result.packages.length} packages' notices, ${result.texts.length} licence texts: ${OUT}`
		);
	} catch (error) {
		console.error(
			'The notices of the site’s packages could not be written. Run `npm ci` first.\n' +
				(error instanceof Error ? error.message : String(error))
		);
		process.exit(1);
	}
}
