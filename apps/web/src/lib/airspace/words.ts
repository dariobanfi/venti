/**
 * What the app says about the pilot's airspace: what a file gave and above
 * all what it left out, how old what is kept is, and whose it is.
 *
 * The numbers and the provider's own words come from the core. These are
 * the sentences around them, the iPhone app's (`AirspaceModel.swift`,
 * `AirspaceScreen.swift`) where the two apps say the same thing. Where the
 * iPhone speaks of warnings in the cockpit, the browser speaks of the map:
 * it has no cockpit, and an airspace it could not read is simply missing
 * from what it draws.
 */

import type {
	AirspaceFile,
	AirspaceImport,
	AirspaceShape,
	Credit,
	KeptAirspace
} from '#lib/core/index.ts';

/** How the screens write numbers, dates and lists: the reader's own way. */
export interface Writing {
	count(count: number): string;
	day(unixSeconds: number): string;
	list(items: readonly string[]): string;
}

/** "1 airspace", "1,018 airspaces" */
export function airspaces(count: number, text: Pick<Writing, 'count'>): string {
	return count === 1 ? '1 airspace' : `${text.count(count)} airspaces`;
}

/**
 * What bringing in one file did, to be read after the import. The first
 * line says what the file gave and what it did not; the core's own account
 * of the first problems follows, a line each.
 */
export function importSummary(
	result: AirspaceImport,
	file: string,
	text: Pick<Writing, 'count'>
): string {
	let said = `${file}: ${airspaces(result.airspaces, text)} read.`;
	if (result.leftOut > 0) {
		const one = result.leftOut === 1;
		said += ` ${airspaces(result.leftOut, text)} could not be read and ${one ? 'is' : 'are'} left out: ${one ? 'it is' : 'they are'} missing from the map.`;
	}
	if (result.assumed > 0) {
		said += ` In ${result.assumed === 1 ? '1 place' : `${text.count(result.assumed)} places`} something could not be read and the cautious reading was taken.`;
	}
	return [said, ...result.problems].join('\n');
}

/** What stays on a file's row for as long as the file is there, if it left airspace out. */
export function leftOutWarning(file: AirspaceFile): string | undefined {
	if (file.leftOut === 0) return undefined;
	return file.leftOut === 1
		? '1 airspace in this file could not be read. It is missing from the map.'
		: `${file.leftOut} airspaces in this file could not be read. They are missing from the map.`;
}

/** "2 airspaces · brought in Thu, 19 Nov 2026" */
export function fileDetail(file: AirspaceFile, text: Pick<Writing, 'count' | 'day'>): string {
	const count = airspaces(file.airspaces, text);
	return file.importedUnixS === undefined
		? count
		: `${count} · brought in ${text.day(file.importedUnixS)}`;
}

/** "88 airspaces · as of Wed, 18 Nov 2026 · fetched Thu, 19 Nov 2026" */
export function keptDetail(layer: KeptAirspace, text: Pick<Writing, 'count' | 'day'>): string {
	const parts = [airspaces(layer.airspaces, text)];
	if (layer.writtenUnixS !== undefined) parts.push(`as of ${text.day(layer.writtenUnixS)}`);
	parts.push(`fetched ${text.day(layer.keptUnixS)}`);
	return parts.join(' · ');
}

/**
 * Countries by their names: "Germany and Austria". A code the browser does
 * not know stays as it is.
 */
export function countryNames(
	codes: readonly string[],
	text: Pick<Writing, 'list'>,
	locale = 'en'
): string {
	let names: Intl.DisplayNames | undefined;
	try {
		names = new Intl.DisplayNames(locale, { type: 'region' });
	} catch {
		// A browser without the names: the codes will do.
	}
	return text.list(
		codes.map((code) => {
			try {
				return names?.of(code) ?? code;
			} catch {
				return code;
			}
		})
	);
}

/**
 * Whose airspace is on the map, in the line that stands on the map. What
 * the providers ask to be said beside it is said here in two words and in
 * full on the airspace screen.
 */
export function creditLine(credits: readonly Credit[], text: Pick<Writing, 'list'>): string {
	if (credits.length === 0) return '';
	const certified = credits.some((credit) => credit.notice !== undefined) ? ', not certified' : '';
	return `Airspace: ${text.list(credits.map((credit) => credit.provider))}${certified}`;
}

/** What the maps draw from, in a line for the settings: "openAIP, 88 airspaces · 1 file". */
export function inALine(
	kept: readonly KeptAirspace[],
	files: readonly AirspaceFile[],
	text: Pick<Writing, 'count'>
): string {
	const parts = kept.map((layer) => `${layer.provider}, ${airspaces(layer.airspaces, text)}`);
	if (files.length > 0) parts.push(files.length === 1 ? '1 file' : `${files.length} files`);
	return parts.length === 0 ? 'None' : parts.join(' · ');
}

/** An airspace's name with its floor and ceiling under it, as on a chart. */
export function label(shape: AirspaceShape): string {
	const limits = `${shape.lower} – ${shape.upper}`;
	return shape.name === '' ? limits : `${shape.name}\n${limits}`;
}
