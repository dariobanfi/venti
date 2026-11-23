/**
 * Where each screen lives. Every link in the app is made here, so the app
 * can be served from a folder of a site as well as from its root.
 *
 * A flight is named by a query parameter, not by a part of the path. The
 * app is static files with no server behind it: `/settings/account/flights/flight/`
 * is one page that any web server can serve, where a path for each flight
 * would need a server that answers for every identifier.
 */

import { resolve } from '$app/paths';

/** Today: the take-offs around a place, and what the forecast says of each. */
export function todayHref(): string {
	return resolve('/today');
}

/** The home page: what is going on, on Venti and in the air. */
export function homeHref(): string {
	return resolve('/');
}

/**
 * The map; with a place, opened around it (`?lat=…&lon=…`), as a site's
 * page sends the pilot there; with a layer, with that layer on (`?show=pilots`).
 */
export function mapHref(around?: { lat: number; lon: number }, show?: 'pilots'): string {
	const params = new URLSearchParams();
	if (around) {
		params.set('lat', around.lat.toFixed(5));
		params.set('lon', around.lon.toFixed(5));
	}
	if (show) params.set('show', show);
	const query = params.toString();
	return query ? `${resolve('/map')}?${query}` : resolve('/map');
}

/** The place the map's address opens it around, where it names one. */
export function mapAroundIn(params: URLSearchParams): { lat: number; lon: number } | undefined {
	const lat = Number(params.get('lat') ?? NaN);
	const lon = Number(params.get('lon') ?? NaN);
	if (!Number.isFinite(lat) || !Number.isFinite(lon)) return undefined;
	if (Math.abs(lat) > 90 || Math.abs(lon) > 180) return undefined;
	return { lat, lon };
}

/**
 * The pilot's own flights, on the account (owner decision 20): reached
 * from the account and from everyone's flights. `/logbook/` leads here.
 */
export function myFlightsHref(): string {
	return resolve('/settings/account/flights');
}

/** What all the pilot's flights add up to, the private ones too. */
export function myStatisticsHref(): string {
	return resolve('/settings/account/flights/statistics');
}

export function settingsHref(): string {
	return resolve('/settings');
}

export function accountHref(): string {
	return resolve('/settings/account');
}

/** "Programs with access", under the account. */
export function programsHref(): string {
	return resolve('/settings/account/programs');
}

/** The pilot's alarms and what they fired, under the account. */
export function alertsHref(): string {
	return resolve('/settings/account/alerts');
}

/** What the pilot was told of follows, cheers and comments, under the account. */
export function notificationsHref(): string {
	return resolve('/settings/account/notifications');
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
 * the core offers it; without one the page shows the newest. `month` is a
 * month as the ranking's periods write one ("2026-05"): the ranking is
 * then of that month, which says its year itself.
 */
export function rankingHref(season = '', month = ''): string {
	const path = resolve('/logbook/ranking');
	if (month) return `${path}?month=${encodeURIComponent(month)}`;
	return season ? `${path}?season=${encodeURIComponent(season)}` : path;
}

/** The public flights of one pilot, by handle. */
export function pilotHref(handle: string): string {
	return `${resolve('/logbook/everyone/pilot')}?handle=${encodeURIComponent(handle)}`;
}

/**
 * A place pilots fly from: the flying site there and the flights that took
 * off around it. `site` names the site the page is about; without one it
 * is the take-off nearest to the place. `query` is what else the page asks
 * for, as `placeQuery` writes it.
 */
export function placeHref(at: { lat: number; lon: number }, site = '', query = ''): string {
	// Five decimals are a metre: the address stays short and the place where it is.
	const params = new URLSearchParams({ lat: at.lat.toFixed(5), lon: at.lon.toFixed(5) });
	if (site) params.set('site', site);
	const more = query.replace(/^\?/, '');
	return `${resolve('/logbook/everyone/place')}?${params}${more ? `&${more}` : ''}`;
}

/** Another pilot's flight, by its address on the server. */
export function publicFlightHref(id: string): string {
	return `${resolve('/logbook/everyone/flight')}?id=${encodeURIComponent(id)}`;
}

/**
 * Community: the plans of the pilot's groups for the coming days, the
 * flights of the pilots they follow, finding a pilot, and what they were
 * told. `/plans/`, where the board was, leads here.
 */
export function communityHref(): string {
	return resolve('/community');
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

/** One of the pilot's own flights, by its address on the server. */
export function myFlightHref(id: string): string {
	return `${resolve('/settings/account/flights/flight')}?id=${encodeURIComponent(id)}`;
}

/** What Venti stands for. */
export function aboutHref(): string {
	return resolve('/about');
}

/** Venti's news, the newest first. */
export function newsHref(): string {
	return resolve('/news');
}

/** One news post, by its file's name. */
export function newsPostHref(slug: string): string {
	return resolve('/news/[slug]', { slug });
}

/**
 * The pages for developers: where Venti's data is and who may use it, the
 * REST API's reference and its schemas, the connector, the sources and
 * their licences with the downloads, and how to add a source.
 */
export function developersHref(
	page: '' | 'api' | 'api/schemas' | 'connector' | 'data' | 'add-a-source' = ''
): string {
	switch (page) {
		case 'api':
			return resolve('/developers/api');
		case 'api/schemas':
			return resolve('/developers/api/schemas');
		case 'connector':
			return resolve('/developers/connector');
		case 'data':
			return resolve('/developers/data');
		case 'add-a-source':
			return resolve('/developers/add-a-source');
		default:
			return resolve('/developers');
	}
}

/** One schema of the API's description, on the page of the schemas. */
export function schemaHref(name: string): string {
	return `${resolve('/developers/api/schemas')}#schema-${name}`;
}

/** Where the code of Venti is kept. */
export const SOURCE_CODE = 'https://github.com/dariobanfi/venti';

/** The server's description of its API, at the root of the site's origin. */
export const API_DESCRIPTION = '/v1/openapi.json';
