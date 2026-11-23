/**
 * The sources Venti's server is built with, each with its licence and
 * credit, read at build time from their manifests (`manifest.toml` in each
 * `crates/sources-*`), which the server reads to credit every answer. The
 * manifests are TOML of a small kind: comments, tables (`[licence]`,
 * `[[asset]]`, `[asset.licence]`), and keys whose values are strings,
 * numbers, true or false, and lists of strings, a list over several lines.
 * This reads that kind and no more; a test reads every manifest with it.
 */

const files = import.meta.glob('../../../../../crates/sources-*/manifest*.toml', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

type Value = string | number | boolean | string[];
type Table = { [key: string]: Value | Table | Table[] };

/** A string as TOML writes it, quoted with `"` (with escapes) or `'` (without). */
function stringAt(text: string, at: number): [string, number] {
	const quote = text[at];
	let value = '';
	for (let i = at + 1; i < text.length; i++) {
		const sign = text[i];
		if (sign === quote) return [value, i + 1];
		if (quote === '"' && sign === '\\') {
			const next = text[++i];
			if (next === 'u' || next === 'U') {
				const length = next === 'u' ? 4 : 8;
				value += String.fromCodePoint(parseInt(text.slice(i + 1, i + 1 + length), 16));
				i += length;
			} else value += { n: '\n', t: '\t', '"': '"', '\\': '\\' }[next] ?? next;
		} else value += sign;
	}
	throw new Error('A string in a manifest is not closed.');
}

/** A value from `at` on, and where it ends. */
function valueAt(text: string, at: number): [Value, number] {
	const sign = text[at];
	if (sign === '"' || sign === "'") return stringAt(text, at);
	if (sign === '[') {
		const list: string[] = [];
		let i = at + 1;
		for (;;) {
			// Spaces, line ends, commas and comments between the entries.
			while (i < text.length && /[\s,]/.test(text[i])) i++;
			if (text[i] === '#') {
				while (i < text.length && text[i] !== '\n') i++;
				continue;
			}
			if (text[i] === ']') return [list, i + 1];
			const [entry, end] = stringAt(text, i);
			list.push(entry);
			i = end;
		}
	}
	const word = /^[^\s#,\]]+/.exec(text.slice(at))?.[0] ?? '';
	if (word === 'true' || word === 'false') return [word === 'true', at + word.length];
	const number = Number(word.replaceAll('_', ''));
	if (word === '' || Number.isNaN(number))
		throw new Error(`A manifest has a value it cannot read: ${word}`);
	return [number, at + word.length];
}

/** A manifest's tables and keys. */
export function readToml(text: string): Table {
	const root: Table = {};
	let table = root;
	let at = 0;
	while (at < text.length) {
		const rest = text.slice(at);
		const space = /^\s+/.exec(rest);
		if (space) {
			at += space[0].length;
			continue;
		}
		if (rest.startsWith('#')) {
			at += rest.indexOf('\n') === -1 ? rest.length : rest.indexOf('\n');
			continue;
		}
		const header = /^\[(\[)?\s*([\w.-]+)\s*\]\]?/.exec(rest);
		if (header) {
			const names = header[2].split('.');
			let into = root;
			for (const [index, name] of names.entries()) {
				const last = index === names.length - 1;
				if (last && header[1]) {
					const list = (into[name] ??= []) as Table[];
					list.push({});
					into = list[list.length - 1];
				} else {
					const found = (into[name] ??= {}) as Table | Table[];
					into = Array.isArray(found) ? found[found.length - 1] : found;
				}
			}
			table = into;
			at += header[0].length;
			continue;
		}
		const key = /^([\w-]+)\s*=\s*/.exec(rest);
		if (!key) throw new Error(`A manifest has a line it cannot read: ${rest.split('\n')[0]}`);
		const [value, end] = valueAt(text, at + key[0].length);
		table[key[1]] = value;
		at = end;
	}
	return root;
}

/** A licence as a manifest gives it. */
export interface Licence {
	name: string;
	/** The credit line, worded as the provider asks. */
	credit: string;
	link: string;
	notice: string | undefined;
	commercial: boolean;
	shareAlike: boolean;
	/** The day somebody last read the provider's terms. */
	verifiedOn: string;
}

export interface Source {
	id: string;
	provider: string;
	kinds: string[];
	countries: string[];
	licence: Licence;
	/** Data of others an adapter is built with, each under its own licence. */
	assets: { what: string; provider: string; licence: Licence }[];
}

function text(table: Table, key: string): string {
	const value = table[key];
	return typeof value === 'string' ? value : '';
}

function list(table: Table, key: string): string[] {
	const value = table[key];
	return Array.isArray(value) ? value.filter((entry) => typeof entry === 'string') : [];
}

function licenceOf(table: Table | undefined): Licence {
	const licence = table ?? {};
	return {
		name: text(licence, 'name'),
		credit: text(licence, 'attribution'),
		link: text(licence, 'link'),
		notice: text(licence, 'notice') || undefined,
		commercial: licence.commercial === true,
		shareAlike: licence.share_alike === true,
		verifiedOn: text(licence, 'verified_on')
	};
}

/** A source of a manifest. */
export function sourceOf(manifest: Table): Source {
	const assets = Array.isArray(manifest.asset) ? (manifest.asset as Table[]) : [];
	return {
		id: text(manifest, 'id'),
		provider: text(manifest, 'provider'),
		kinds: list(manifest, 'kinds'),
		countries: list(manifest, 'countries'),
		licence: licenceOf(manifest.licence as Table | undefined),
		assets: assets.map((asset) => ({
			what: text(asset, 'what') || text(asset, 'name') || text(asset, 'file'),
			provider: text(asset, 'provider'),
			licence: licenceOf(asset.licence as Table | undefined)
		}))
	};
}

/** Every source the server is built with, by its provider's name. */
export function sourcesOf(manifests: Record<string, string>): Source[] {
	return Object.values(manifests)
		.map((file) => sourceOf(readToml(file)))
		.sort((a, b) => a.provider.localeCompare(b.provider) || a.id.localeCompare(b.id));
}

export const sources = sourcesOf(files);
