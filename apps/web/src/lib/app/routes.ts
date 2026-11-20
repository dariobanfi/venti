/**
 * Where each screen lives. Every link in the app is made here, so the app
 * can be served from a folder of a site as well as from its root.
 *
 * A flight is named by a query parameter, not by a part of the path. The
 * app is static files with no server behind it: `/logbook/flight/` is one
 * page that any web server can serve, where `/logbook/<id>/` would need a
 * server that answers for every identifier.
 */

import { resolve } from '$app/paths';

export function mapHref(): string {
	return resolve('/');
}

export function logbookHref(): string {
	return resolve('/logbook');
}

export function statisticsHref(): string {
	return resolve('/logbook/statistics');
}

export function settingsHref(): string {
	return resolve('/settings');
}

export function accountHref(): string {
	return resolve('/settings/account');
}

export function airspaceHref(): string {
	return resolve('/settings/airspace');
}

/** Everyone's public flights. `query` is what is asked of them, as `queryOf` writes it. */
export function everyoneHref(query = ''): string {
	return `${resolve('/logbook/everyone')}${query}`;
}

/**
 * The ranking of the server's pilots. `season` is the name of a season as
 * the core offers it; without one the page shows the newest.
 */
export function rankingHref(season = ''): string {
	const path = resolve('/logbook/ranking');
	return season ? `${path}?season=${encodeURIComponent(season)}` : path;
}

/** The public flights of one pilot, by handle. */
export function pilotHref(handle: string): string {
	return `${resolve('/logbook/everyone/pilot')}?handle=${encodeURIComponent(handle)}`;
}

/** Another pilot's flight, by its address on the server. */
export function publicFlightHref(id: string): string {
	return `${resolve('/logbook/everyone/flight')}?id=${encodeURIComponent(id)}`;
}

/** The board: who means to fly where, in the pilot's groups. */
export function plansHref(): string {
	return resolve('/plans');
}

/** One plan, by its identifier on the server. */
export function planHref(id: string): string {
	return `${resolve('/plans/plan')}?id=${encodeURIComponent(id)}`;
}

/** The pilot's groups. */
export function groupsHref(): string {
	return resolve('/plans/groups');
}

/** One group, by its identifier on the server. */
export function groupHref(id: string): string {
	return `${resolve('/plans/groups/group')}?id=${encodeURIComponent(id)}`;
}

export function flightHref(id: string): string {
	return `${resolve('/logbook/flight')}?id=${encodeURIComponent(id)}`;
}
