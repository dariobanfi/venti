/**
 * The thermal forecast by region as the map tab sees it: the regions'
 * outlines, read once, and the days asked for as the pilot goes through
 * them. Every level, word and credit is the core's; `regions.ts` holds
 * what is drawn of them.
 */

import { messageOf } from '#lib/core/index.ts';
import type { CoreClient, RegionDay, RegionOutlines } from '#lib/core/index.ts';

type Core = Pick<CoreClient, 'regionOutlines' | 'regionDay'>;

export class RegionForecastModel {
	/** The regions, once read, with their outlines parsed for the map. */
	regions = $state.raw<RegionOutlines & { shapes: GeoJSON.FeatureCollection }>();
	/** The days read in this visit, by date. A day that could not be had is not kept. */
	days = $state.raw<ReadonlyMap<string, RegionDay>>(new Map());
	/** Why the last asking brought nothing, in words: the grid then stands alone. */
	problem = $state<string>();

	#core: Core;
	#asking = new Set<string>();

	constructor(core: Core) {
		this.#core = core;
	}

	/**
	 * Asks for a day, and for the regions where they are not at hand or
	 * the day names another set of them. Nothing is asked twice at once.
	 */
	async load(date: string): Promise<void> {
		if (this.days.has(date) || this.#asking.has(date)) return;
		this.#asking.add(date);
		try {
			const day = await this.#core.regionDay(date);
			if (!day) return;
			const version = day.view.regionsVersion;
			if (this.regions?.version !== version) {
				const regions = await this.#core.regionOutlines(version);
				if (!regions) return;
				this.regions = { ...regions, shapes: JSON.parse(regions.outlines) };
			}
			this.days = new Map([...this.days, [date, day]]);
			this.problem = undefined;
		} catch (error) {
			this.problem = messageOf(error);
		} finally {
			this.#asking.delete(date);
		}
	}
}
