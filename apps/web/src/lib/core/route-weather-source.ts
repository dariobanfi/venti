/**
 * A planned route with the weather along it, as the worker finds it out:
 * when the pilot would be at each place, what the server's forecast has
 * there then, each leg's wind, and the height to leave each place at.
 *
 * The core says what to ask the server for (`routeWeatherPath`), reads its
 * answer once (`RouteForecast`) and works out the rest (`along`, or
 * `routeAlong` without a forecast). This file keeps the newest answer and
 * the elevation tiles under the route, so that a change of the start, the
 * speed or the glide asks the core again and fetches nothing. A route or a
 * day of its own lets go of both: a tile's heights are half a megabyte.
 */

import { ONLINE } from '../map/online';
import {
	type Bounds,
	routeAlong,
	type RouteAlong,
	RouteForecast,
	type RoutePlace,
	type RouteSetting,
	routeWeatherPath,
	Terrain
} from './pkg/venti_core_wasm.js';
import { messageOf, unreachable } from './protocol';
import { fetchTiles, type GetTile, getTile } from './terrain-source';
import { type Get, getFromThisOrigin } from './wind-source';

/** A route as the pilot means to fly it, and on which day of the thermal map. */
export interface RouteWeatherAsked {
	setting: RouteSetting;
	/** The thermal map's day the route is flown on. Without one, no forecast is asked for. */
	date?: string;
}

/** What the core says of the route; with why there is no forecast, where there is none. */
export interface RouteWeatherAnswer {
	along: RouteAlong;
	/** Why the forecast could not be had, in words for the pilot. */
	problem?: string;
}

const getRouteWeather: Get = async (path) => {
	try {
		return await getFromThisOrigin(path);
	} catch {
		throw unreachable('Venti could not be reached for the weather along the route.');
	}
};

/** How far this browser's clock is ahead of UTC at a moment, in seconds. */
const offsetAt = (unixS: number) => -new Date(unixS * 1000).getTimezoneOffset() * 60;

/** The box a route's places lie in: its legs lie in it too. */
function boxOf(places: readonly RoutePlace[]): Bounds | undefined {
	if (places.length < 2) return undefined;
	const lats = places.map((place) => place.lat);
	const lons = places.map((place) => place.lon);
	return {
		south: Math.min(...lats),
		west: Math.min(...lons),
		north: Math.max(...lats),
		east: Math.max(...lons)
	};
}

export class RouteWeatherSource {
	#present: () => Promise<boolean>;
	#get: Get;
	#getTile: GetTile;
	#tiles: string;
	#offsetAt: (unixS: number) => number;

	/** The newest answer of the server, by the path it answered. */
	#forecast: { path: string; forecast: RouteForecast } | undefined;
	/** The elevation tiles under the newest route, by its box. */
	#terrain: { box: string; terrain: Terrain; filled: Promise<void> } | undefined;

	constructor(
		present: () => Promise<boolean> = async () => true,
		get: Get = getRouteWeather,
		tile: GetTile = getTile,
		tiles: string = ONLINE.terrainTiles,
		offset: (unixS: number) => number = offsetAt
	) {
		this.#present = present;
		this.#get = get;
		this.#getTile = tile;
		this.#tiles = tiles;
		this.#offsetAt = offset;
	}

	/**
	 * The route with the weather along it. Without a server, a day or an
	 * answer, the times and the heights in still air, with the reason.
	 * Throws only for a setting the core cannot read.
	 */
	async along({ setting, date }: RouteWeatherAsked): Promise<RouteWeatherAnswer> {
		const { terrain, stale } = await this.#terrainUnder(setting.places);
		const utcOffsetS = this.#offsetAt(setting.startUnixS);
		let problem: string | undefined;
		let forecast: RouteForecast | undefined;
		if (date && setting.places.length >= 2 && (await this.#present())) {
			try {
				forecast = await this.#forecastFor(setting.places, date);
			} catch (error) {
				problem = messageOf(error);
			}
		}
		// An answer for a route asked about since: the page drops it, and
		// the forecast it was read with may be let go of by now.
		if (forecast && this.#forecast?.forecast !== forecast) forecast = undefined;
		try {
			const along = forecast
				? forecast.along(setting, terrain, utcOffsetS)
				: routeAlong(setting, terrain, utcOffsetS);
			return problem ? { along, problem } : { along };
		} finally {
			if (stale) terrain.free();
		}
	}

	async #forecastFor(places: RoutePlace[], date: string): Promise<RouteForecast> {
		const path = routeWeatherPath(places, date);
		if (this.#forecast?.path === path) return this.#forecast.forecast;
		const answer = await this.#get(path);
		const forecast = new RouteForecast(answer.status, answer.body);
		// Another route may have been asked for meanwhile: the newest wins.
		this.#forecast?.forecast.free();
		this.#forecast = { path, forecast };
		return forecast;
	}

	/**
	 * The tiles under a route, kept for the next question. A route asked
	 * about since has the kept ones by the time these are filled: it gets
	 * an empty terrain of its own, `stale`, to free.
	 */
	async #terrainUnder(places: RoutePlace[]): Promise<{ terrain: Terrain; stale: boolean }> {
		const bounds = boxOf(places);
		const box = bounds ? [bounds.south, bounds.west, bounds.north, bounds.east].join() : '';
		if (this.#terrain?.box !== box) {
			this.#terrain?.terrain.free();
			const terrain = new Terrain();
			const filled = bounds
				? fetchTiles(terrain, bounds, this.#getTile, this.#tiles, () => this.#terrain?.box === box)
				: Promise.resolve();
			this.#terrain = { box, terrain, filled };
		}
		const kept = this.#terrain;
		await kept.filled;
		return this.#terrain === kept
			? { terrain: kept.terrain, stale: false }
			: { terrain: new Terrain(), stale: true };
	}
}
