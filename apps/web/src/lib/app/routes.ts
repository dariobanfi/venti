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

export function flightHref(id: string): string {
	return `${resolve('/logbook/flight')}?id=${encodeURIComponent(id)}`;
}
