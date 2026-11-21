/**
 * The filter of the flying sites as the page holds it: what the pilot
 * chose, how this browser remembers it, and what is drawn and listed while
 * the core's answer is on its way.
 *
 * Which sites a filter leaves, which stand apart and every word about it
 * are the core's (`filterSites`, `sitesFilterChoices`), asked through the
 * worker. Nothing here decides whether a site fits.
 */

import type {
	Coordinate,
	SitesFilter,
	SitesFiltered,
	SitesShown,
	SitesWindIs,
	SiteView,
	WindStation
} from '#lib/core/index.ts';

/**
 * What the pilot chose. A direction is kept only for one the pilot chose
 * from the compass: the wind now is read anew wherever the map is.
 */
export interface SitesChoice {
	shown: SitesShown;
	windIs: SitesWindIs;
	/** The compass point chosen, in degrees, with `windIs` as `chosen`. Nothing asks about no wind. */
	windFromDeg: number | undefined;
	/** Take-offs from this height up, in metres. Nothing asks about no height. */
	takeoffsFromM: number | undefined;
	/**
	 * Whether the places from pilots are on the map and in the list: the
	 * crowdsourced layer, which nothing shows until its pilot switched it
	 * on. It is no part of the filter the core is asked with: it decides
	 * what the server is asked for.
	 */
	fromPilots: boolean;
}

/** Nothing chosen: every site. */
export const NO_CHOICE: SitesChoice = {
	shown: 'all',
	windIs: 'chosen',
	windFromDeg: undefined,
	takeoffsFromM: undefined,
	fromPilots: false
};

const KEY = 'venti.map-sites-filter';
const SHOWN: readonly SitesShown[] = ['all', 'takeoffs', 'landings'];
const WIND_IS: readonly SitesWindIs[] = ['chosen', 'forecastNow', 'measuredNow'];

const number = (kept: unknown): number | undefined =>
	typeof kept === 'number' && Number.isFinite(kept) ? kept : undefined;

/**
 * The filter as this browser remembers it, as it does the map's layers.
 * Nothing chosen wherever that cannot be read.
 */
export function keptChoice(storage: Pick<Storage, 'getItem'> | undefined): SitesChoice {
	try {
		const kept: unknown = JSON.parse(storage?.getItem(KEY) ?? 'null');
		if (typeof kept !== 'object' || kept === null) return NO_CHOICE;
		const { shown, windIs, windFromDeg, takeoffsFromM, fromPilots } = kept as Record<
			string,
			unknown
		>;
		const is = WIND_IS.find((one) => one === windIs) ?? 'chosen';
		return {
			shown: SHOWN.find((one) => one === shown) ?? 'all',
			windIs: is,
			windFromDeg: is === 'chosen' ? number(windFromDeg) : undefined,
			takeoffsFromM: number(takeoffsFromM),
			// Off unless the pilot left it on.
			fromPilots: fromPilots === true
		};
	} catch {
		return NO_CHOICE;
	}
}

/** Remembers the filter. Nothing happens where the browser keeps nothing. */
export function keepChoice(
	storage: Pick<Storage, 'setItem'> | undefined,
	choice: SitesChoice
): void {
	try {
		storage?.setItem(KEY, JSON.stringify(choice));
	} catch {
		// A private window, or no storage: the choice lasts for the visit.
	}
}

/**
 * The wind now, as the page can hand it to the core: the direction a wind
 * station measured, where the map has one. A forecast for the current hour
 * the page does not have at hand, so "Forecast now" is not offered.
 */
export interface WindNow {
	measuredDeg: number | undefined;
}

/**
 * The filter the core is asked with. The wind now is the direction the
 * page has for it; without one, no wind is asked about.
 */
export function asked(choice: SitesChoice, now: WindNow): SitesFilter {
	const windFromDeg =
		choice.windIs === 'chosen'
			? choice.windFromDeg
			: choice.windIs === 'measuredNow'
				? now.measuredDeg
				: undefined;
	return {
		shown: choice.shown,
		windIs: choice.windIs,
		windFromDeg,
		takeoffsFromM: choice.takeoffsFromM
	};
}

/** Whether two filters ask the core the same. */
export function sameAsked(first: SitesFilter, second: SitesFilter): boolean {
	return (
		first.shown === second.shown &&
		first.windIs === second.windIs &&
		first.windFromDeg === second.windFromDeg &&
		first.takeoffsFromM === second.takeoffsFromM
	);
}

/**
 * The direction "Measured now" stands for: that of the station nearest to
 * a place, of those that have a reading that is not old and names a
 * direction. One direction for every take-off, as the core's note says.
 */
export function measuredNow(
	stations: readonly WindStation[],
	place: Coordinate | undefined
): number | undefined {
	if (!place) return undefined;
	// Near enough for telling stations of one map apart: degrees of
	// longitude shortened by the latitude.
	const shorter = Math.cos((place.lat * Math.PI) / 180);
	const away = ({ position }: WindStation) =>
		Math.hypot(position.lat - place.lat, (position.lon - place.lon) * shorter);
	return stations
		.filter((station) => !station.isOld && station.latest?.directionDeg !== undefined)
		.toSorted((first, second) => away(first) - away(second))[0]?.latest?.directionDeg;
}

/** The core's answer, with what it was asked. */
export interface Answered {
	sites: readonly SiteView[];
	filter: SitesFilter;
	left: SitesFiltered;
}

/** What the map draws and the list shows of the sites. */
export interface SitesShownNow {
	/** The sites the filter leaves, drawn as sites are. */
	fitting: readonly SiteView[];
	/** The take-offs whose source does not say what was asked, drawn apart. */
	unsaid: readonly SiteView[];
	/** The core's words for them, once it has answered for these sites and this filter. */
	left: SitesFiltered | undefined;
}

const NONE: readonly SiteView[] = [];

/**
 * What to show of `sites` under `filter`: the core's answer where it is
 * for these sites and this filter. Until it arrives, every site where
 * nothing is chosen, and else what the last answer left, so that a map
 * that was moved does not show for a moment what the filter leaves out.
 */
export function shownNow(
	sites: readonly SiteView[],
	filter: SitesFilter,
	choice: SitesChoice,
	answered: Answered | undefined
): SitesShownNow {
	if (answered && answered.sites === sites && sameAsked(answered.filter, filter)) {
		return { fitting: answered.left.fitting, unsaid: answered.left.unsaid, left: answered.left };
	}
	const nothing =
		choice.shown === 'all' &&
		filter.windFromDeg === undefined &&
		filter.takeoffsFromM === undefined;
	if (nothing) return { fitting: sites, unsaid: NONE, left: undefined };
	// Of the last answer, the sites that are still held.
	const held = new Set(sites.map((site) => site.id));
	const still = (list: readonly SiteView[]) => list.filter((site) => held.has(site.id));
	return {
		fitting: answered ? still(answered.left.fitting) : NONE,
		unsaid: answered ? still(answered.left.unsaid) : NONE,
		left: undefined
	};
}
