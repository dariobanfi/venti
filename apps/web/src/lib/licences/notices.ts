/**
 * The notices of the code of others that the site carries, as the
 * "Open-source licences" page shows them: the Rust core's, which the
 * repository generates into `licences/core-notices.txt`, and the npm
 * packages', which `scripts/notices.mjs` writes before every build. Both
 * are read at build time; nothing of them is written here.
 */

import core from '../../../../../licences/core-notices.txt?raw';
import npm from './npm-notices.json?raw';
import type { Notices } from '../../../scripts/notices.d.mts';

/** The core's notices whole, and how many crates they name. */
export const coreNotices = { text: core, crates: Number(/^(\d+) crates under/m.exec(core)?.[1]) };

/** Packages that share one licence text, with the text. */
export interface PackageGroup {
	packages: { name: string; version: string; licence: string }[];
	text: string;
}

const notices = JSON.parse(npm) as Notices;

/** One package's notice, as `name` and its version, its licence and the text. */
export function packageNotice(
	name: string
): { version: string; licence: string; text: string } | undefined {
	const found = notices.packages.find((each) => each.name === name);
	return (
		found && { version: found.version, licence: found.licence, text: notices.texts[found.text] }
	);
}

/** The packages but those named, one group to each licence text, in the order of their names. */
export function packageGroups(except: string[] = []): PackageGroup[] {
	const groups = new Map<number, PackageGroup>();
	for (const { name, version, licence, text } of notices.packages) {
		if (except.includes(name)) continue;
		const group = groups.get(text) ?? { packages: [], text: notices.texts[text] };
		group.packages.push({ name, version, licence });
		groups.set(text, group);
	}
	return [...groups.values()];
}
