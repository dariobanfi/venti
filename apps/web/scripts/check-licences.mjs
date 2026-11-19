// Checks that every npm package the web app installs carries a licence that
// is compatible with distributing Venti under AGPL-3.0-or-later. It is the
// web app's counterpart of `cargo deny check licenses` (see deny.toml).
//
//   npm run licences
//
// Reads package-lock.json, so it needs no network and no installed packages.

import { readFileSync } from 'node:fs';

/** Permissive licences: use, change and redistribute, with the notice kept. */
const ALLOWED = new Set([
	'MIT',
	'ISC',
	'0BSD',
	'BSD-2-Clause',
	'BSD-3-Clause',
	'Apache-2.0',
	'(MIT OR Apache-2.0)'
]);

/**
 * Packages allowed under another licence, each with the reason.
 *
 * lightningcss is Vite's own CSS minifier and arrives with Vite. It is
 * MPL-2.0: copyleft for its own files only, and already accepted for the
 * Rust side (deny.toml). It runs when the site is built; none of it is in
 * the built site.
 */
const EXCEPTIONS = [{ name: /^lightningcss(-|$)/, licence: 'MPL-2.0' }];

const lock = JSON.parse(readFileSync(new URL('../package-lock.json', import.meta.url), 'utf8'));

const problems = [];
for (const [path, entry] of Object.entries(lock.packages)) {
	if (path === '') continue;
	const name = path.replace(/^.*node_modules\//, '');
	const licence = entry.license;
	if (ALLOWED.has(licence)) continue;
	if (EXCEPTIONS.some((allowed) => allowed.name.test(name) && allowed.licence === licence))
		continue;
	problems.push(`${name}: ${licence ?? 'no licence stated'}`);
}

if (problems.length > 0) {
	console.error('These packages have a licence that is not on the list:\n' + problems.join('\n'));
	process.exit(1);
}
console.log(`${Object.keys(lock.packages).length - 1} packages, every licence on the list.`);
