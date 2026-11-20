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

/** The public flights of one pilot, by handle. */
export function pilotHref(handle: string): string {
	return `${resolve('/logbook/everyone/pilot')}?handle=${encodeURIComponent(handle)}`;
}

/** Another pilot's flight, by its address on the server. */
export function publicFlightHref(id: string): string {
	return `${resolve('/logbook/everyone/flight')}?id=${encodeURIComponent(id)}`;
}

export function flightHref(id: string): string {
	return `${resolve('/logbook/flight')}?id=${encodeURIComponent(id)}`;
}
