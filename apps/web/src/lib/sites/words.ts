/**
 * What the app says about flying sites: what a sign on a map is worth,
 * whose the data is, and why a list is empty.
 *
 * What a site is and the wind it works in come from the core, in words.
 * These are the sentences around them, the iPhone app's
 * (`SiteScreens.swift`, `MapScreen.swift`) where the two apps say the same
 * thing.
 */

import type { Credit, SiteView } from '#lib/core/index.ts';
import type { SitesState } from './watch';

/** How the screens write numbers, dates and lists: the reader's own way. */
export interface Writing {
	metres(metres: number): string;
	day(unixSeconds: number): string;
	list(items: readonly string[]): string;
}

/** What a site on the map is worth, said wherever sites are listed. */
export const CAUTION =
	'A site on the map is not permission to fly there. Ask the club or the owner, and look at the place.';

/** One site in a line under its name: "Take-off · 1,560 m · wind S to SW". */
export function detail(site: SiteView, text: Pick<Writing, 'metres'>): string {
	const parts: string[] = [];
	// An unnamed site is called what it is already.
	if (site.isNamed) parts.push(site.kindInWords);
	if (site.elevationM !== undefined) parts.push(`${text.metres(site.elevationM)} m`);
	if (site.windInWords !== '') parts.push(`wind ${site.windInWords}`);
	return parts.length === 0 ? 'No more is known' : parts.join(' · ');
}

/** Something the source marks a site as. */
export interface Mark {
	icon: 'verified' | 'warning' | 'nosign';
	says: string;
	/** Whether it speaks against flying there. */
	warns: boolean;
}

/** What the source marks a site as, a line each. */
export function marks(site: SiteView): Mark[] {
	const marks: Mark[] = [];
	if (site.official === true) {
		marks.push({ icon: 'verified', says: 'Marked as an official site', warns: false });
	}
	if (site.discouraged) {
		marks.push({ icon: 'warning', says: 'Marked as unsafe or discouraged', warns: true });
	}
	if (site.paragliding === false) {
		marks.push({ icon: 'nosign', says: 'Marked as not for paragliders', warns: false });
	}
	return marks;
}

/** Whose sites are on the map, in the line that stands on the map. */
export function creditLine(credits: readonly Credit[], text: Pick<Writing, 'list'>): string {
	if (credits.length === 0) return '';
	return `Sites: ${text.list(credits.map((credit) => credit.provider))}`;
}

/** How old the sources' data is: "As of Wed, 18 Nov 2026." */
export function asOf(unixSeconds: number | undefined, text: Pick<Writing, 'day'>): string {
	return unixSeconds === undefined ? '' : `As of ${text.day(unixSeconds)}.`;
}

/** A piece of a sentence: plain words, or an address to follow. */
export interface Piece {
	text: string;
	href?: string;
}

/**
 * A source's notice with the web addresses in it made out, so that a page
 * can offer them as links: the licence's own text, and where to correct the
 * data. Only `https` addresses count, and a full stop or comma after one is
 * not part of it.
 */
export function withLinks(notice: string): Piece[] {
	const pieces: Piece[] = [];
	let rest = 0;
	for (const found of notice.matchAll(/https:\/\/[^\s<>"']+/g)) {
		const address = found[0].replace(/[.,;:)]+$/, '');
		if (found.index > rest) pieces.push({ text: notice.slice(rest, found.index) });
		pieces.push({ text: address, href: address });
		rest = found.index + address.length;
	}
	if (rest < notice.length) pieces.push({ text: notice.slice(rest) });
	return pieces;
}

/** Why a list of sites is empty. */
export function nothingListed(state: SitesState): string {
	if (state.problem) return state.problem;
	if (!state.close) {
		return 'Zoom the map in on an area, and the sites its source knows there are listed.';
	}
	if (!state.known) return 'Looking for flying sites…';
	return state.isWhole
		? 'The source knows none on this part of the map. A place without a sign may still be a site.'
		: 'The server has no flying sites for this part of the map.';
}
