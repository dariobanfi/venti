/**
 * The thermal map as the map tab sees it, as Android's and the iPhone's
 * `ThermalMapModel`: asks the core in its worker for the map, keeps the
 * newest one of this visit, and remembers which day the pilot looks at.
 *
 * A map that was fetched stays when a later asking fails: the card shows
 * its run's time, so an old map is never shown as new. `thermal-map.ts`
 * holds the rules.
 */

import type {
	Coordinate,
	CoreClient,
	HoursAsked,
	ThermalMap,
	ThermalMapHoursView,
	ThermalMapWind,
	ThermalStripDay
} from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { forecastChoice, keepForecastChoice } from '#lib/map/layers.ts';
import {
	dayOf,
	hoursAsked,
	isDue,
	sameAsked,
	shownHour,
	type WindLevel,
	WORDS
} from './thermal-map';

/** What the map draws of one hour: the squares and the arrows. */
export interface HourShapes {
	asked: HoursAsked;
	hour: number;
	squares: GeoJSON.FeatureCollection;
	arrows: GeoJSON.FeatureCollection;
}

type Core = Pick<
	CoreClient,
	| 'thermalMap'
	| 'thermalHours'
	| 'thermalHourShapes'
	| 'thermalHourSquare'
	| 'thermalCellAt'
	| 'thermalWindsAt'
	| 'thermalStrip'
>;

/** How often the layer looks whether the map is due, in milliseconds. */
const CHECK_MS = 60_000;

/** The shell's words for a server that gave no answer at all. */
const UNREACHABLE = 'Venti could not be reached for the forecast map.';

export class ThermalMapModel {
	/** The newest map read in this visit. */
	map = $state.raw<ThermalMap>();
	/** Whether the server is being asked. */
	asking = $state(false);
	/** Why the last asking brought no map, in words for the pilot. */
	problem = $state<string>();
	/** The day the pilot chose, by its date; the map's first without one. */
	chosenDate = $state<string>();

	/** The day shown. */
	readonly day = $derived(dayOf(this.map, this.chosenDate));

	/** The layer the pilot chose, by the core's name, and the height of the wind: kept. */
	layer = $state('thermals');
	heightM = $state<number>();
	/** The height of the wind above the model's ground, for a level that is one. */
	aboveGroundM = $state<number>();
	/** The hour the pilot chose on the line of hours; "day" for the day as a whole. */
	chosenHour = $state<number | 'day'>();

	/** What the hours are asked for: nothing where the map has none. */
	readonly asked = $derived(
		hoursAsked(this.map, this.day, this.layer, this.heightM, this.aboveGroundM)
	);
	/** The hours of what is asked, once read. */
	hours = $state.raw<{ asked: HoursAsked; view: ThermalMapHoursView }>();
	/** Whether the hours are being asked for. */
	hoursAsking = $state(false);
	/** Why the hours could not be had, in words: the layer then shows the day. */
	hoursProblem = $state<string>();
	/** The hours that belong to what is asked, if they are read. */
	readonly view = $derived(
		this.hours && sameAsked(this.hours.asked, this.asked) ? this.hours.view : undefined
	);
	/** The hour shown, by its number; nothing for the day as a whole. */
	readonly hour = $derived(this.view ? shownHour(this.view, this.chosenHour) : undefined);
	/** What the map draws of the hour shown. */
	shapes = $state.raw<HourShapes>();
	/**
	 * The hours of every day of the map, for the layer and the height
	 * chosen: the stops of the map's line of time. A day whose hours could
	 * not be had has none.
	 */
	line = $state.raw<{
		map: ThermalMap;
		layer: string;
		heightM: number | undefined;
		aboveGroundM: number | undefined;
		days: (ThermalMapHoursView | undefined)[];
	}>();

	#core: Core;
	#clock: () => number;
	#triedMs: number | undefined;
	#fetchedMs: number | undefined;
	/** Counts the questions about hours, so that a slow answer is dropped. */
	#hoursAsked = 0;
	#shapesAsked = 0;
	#lineAsked = 0;

	constructor(core: Core, clock: () => number = Date.now) {
		this.#core = core;
		this.#clock = clock;
		const kept = forecastChoice(globalThis.localStorage);
		this.layer = kept.layer;
		this.heightM = kept.heightM;
		this.aboveGroundM = kept.aboveGroundM;
	}

	/** Shows another layer; on another layer the hours start again where the core says. */
	chooseLayer(layer: string): void {
		if (layer === this.layer) return;
		this.layer = layer;
		this.chosenHour = undefined;
		keepForecastChoice(globalThis.localStorage, {
			layer,
			heightM: this.heightM,
			aboveGroundM: this.aboveGroundM
		});
	}

	/** Shows the wind at another level, or at the ground; the hour stays. */
	chooseHeight(level: WindLevel): void {
		this.heightM = level.heightM;
		this.aboveGroundM = level.aboveGroundM;
		keepForecastChoice(globalThis.localStorage, {
			layer: this.layer,
			heightM: level.heightM,
			aboveGroundM: level.aboveGroundM
		});
	}

	/** Shows another day; its hours start again where the core says. */
	chooseDay(date: string): void {
		if (date === this.chosenDate) return;
		this.chosenDate = date;
		this.chosenHour = undefined;
	}

	/**
	 * Asks for the hours of what is asked now, unless they are at hand or
	 * being asked for. Call it again whenever the day, the layer or the
	 * height changes.
	 */
	async loadHours(): Promise<void> {
		const asked = this.asked;
		if (!asked || sameAsked(this.hours?.asked, asked)) return;
		const mine = ++this.#hoursAsked;
		this.hoursAsking = true;
		this.hoursProblem = undefined;
		try {
			const view = await this.#core.thermalHours(asked);
			if (mine !== this.#hoursAsked) return;
			this.hours = { asked, view };
		} catch (error) {
			if (mine !== this.#hoursAsked) return;
			const said = error instanceof Error ? error.message : String(error);
			this.hoursProblem = isInWords(error) ? said : `${WORDS.hoursUnreachable} ${said}`;
		} finally {
			if (mine === this.#hoursAsked) this.hoursAsking = false;
		}
	}

	/**
	 * Asks for the hours of every day of the map, for the layer and the
	 * height chosen, unless they are at hand. The worker keeps each answer,
	 * so the day shown is not asked for twice. Call it again whenever the
	 * map, the layer or the height changes.
	 */
	async loadLine(): Promise<void> {
		const map = this.map;
		const { layer, heightM, aboveGroundM } = this;
		const line = this.line;
		if (
			line &&
			line.map === map &&
			line.layer === layer &&
			line.heightM === heightM &&
			line.aboveGroundM === aboveGroundM
		) {
			return;
		}
		const mine = ++this.#lineAsked;
		if (!map || map.layers.length === 0) {
			this.line = undefined;
			return;
		}
		const days = await Promise.all(
			map.days.map((day) => {
				const asked = hoursAsked(map, day, layer, heightM, aboveGroundM);
				return asked
					? this.#core.thermalHours(asked).catch(() => undefined)
					: Promise.resolve(undefined);
			})
		);
		if (mine === this.#lineAsked) this.line = { map, layer, heightM, aboveGroundM, days };
	}

	/** Asks for what the map draws of the hour shown; nothing to draw for the day. */
	async loadShapes(): Promise<void> {
		const hours = this.view ? this.hours : undefined;
		const hour = this.hour;
		const mine = ++this.#shapesAsked;
		if (!hours || hour === undefined) {
			this.shapes = undefined;
			return;
		}
		if (this.shapes && sameAsked(this.shapes.asked, hours.asked) && this.shapes.hour === hour) {
			return;
		}
		const shapes = await this.#core.thermalHourShapes(hours.asked, hour);
		if (mine !== this.#shapesAsked) return;
		this.shapes = {
			asked: hours.asked,
			hour,
			squares: JSON.parse(shapes.squares),
			arrows: JSON.parse(shapes.arrows)
		};
	}

	/** The number of the square a place lies in, in the hours shown. */
	cellAt(place: { lat: number; lon: number }): Promise<number | undefined> {
		const hours = this.view ? this.hours : undefined;
		return hours
			? this.#core.thermalCellAt(hours.asked, place.lat, place.lon)
			: Promise.resolve(undefined);
	}

	/**
	 * The forecast wind at each place in the hour drawn, as its shapes were
	 * asked for: the take-offs' fans. Nothing while no hour is drawn.
	 */
	windsAt(places: Coordinate[]): Promise<(ThermalMapWind | undefined)[] | undefined> {
		const shapes = this.shapes;
		return shapes && places.length > 0
			? this.#core.thermalWindsAt(shapes.asked, shapes.hour, places)
			: Promise.resolve(undefined);
	}

	/** A square of the hours shown in the hour shown, in words. */
	square(cell: number) {
		const hours = this.view ? this.hours : undefined;
		const hour = this.hour;
		return hours && hour !== undefined
			? this.#core.thermalHourSquare(hours.asked, cell, hour)
			: Promise.resolve(undefined);
	}

	/** Every day of the map at the square of a place. */
	strip(place: { lat: number; lon: number }): Promise<ThermalStripDay[]> {
		return this.#core.thermalStrip(place);
	}

	/** Asks the server whenever the map is due, while the layer is on. Returns the stop. */
	keepCurrent(): () => void {
		const look = () => {
			if (
				!this.asking &&
				isDue({ triedMs: this.#triedMs, fetchedMs: this.#fetchedMs }, this.#clock())
			)
				void this.load();
		};
		look();
		const timer = setInterval(look, CHECK_MS);
		return () => clearInterval(timer);
	}

	/** Asks the server for the map now. */
	async load(): Promise<void> {
		if (this.asking) return;
		this.asking = true;
		const at = this.#clock();
		this.#triedMs = at;
		try {
			const read = await this.#core.thermalMap();
			if (read) {
				this.map = read;
				// The hours are asked again of the worker, which keeps them
				// while the map is the same and lets them go when it is not.
				this.hours = undefined;
				this.shapes = undefined;
				this.#fetchedMs = at;
				this.problem = undefined;
			} else {
				this.problem = WORDS.noServer;
			}
		} catch (error) {
			const said = error instanceof Error ? error.message : String(error);
			this.problem = isInWords(error) ? said : `${UNREACHABLE} ${said}`;
		} finally {
			this.asking = false;
		}
	}
}
