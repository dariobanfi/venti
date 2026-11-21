/**
 * The route planner of the map tab: the places the pilot taps, in order,
 * and what the core says the route comes to. Every change is kept in the
 * browser (`kept.ts`) and the core is asked again; an answer for a route
 * that has changed since is dropped.
 */

import type { Coordinate, RoutePlan } from '#lib/core/index.ts';
import { type KeptRoute, keepRoute, keptRoute, type RoutePlace } from './kept';

/** Asks the core what a route through these places comes to. */
export type AskPlan = (places: Coordinate[]) => Promise<RoutePlan | undefined>;

type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem' | 'removeItem'> | undefined;

export class RoutePlanner {
	/** The places of the route, in order. */
	places = $state.raw<RoutePlace[]>([]);
	/** Whether the planner is open on the map. */
	open = $state(false);
	/** What the core says of the route as it stands, once it has answered. */
	plan = $state.raw<RoutePlan>();
	/** Why the core could not be asked, in its words. */
	problem = $state.raw<string>();

	#storage: Storage;
	#ask: AskPlan;
	#asked = 0;

	constructor(storage: Storage, ask: AskPlan) {
		this.#storage = storage;
		this.#ask = ask;
		const kept = keptRoute(storage);
		this.places = kept.places;
		this.open = kept.open;
		void this.#again();
	}

	/** Opens the planner on the route kept, or closes it. The route stays until it is cleared. */
	toggle(): void {
		this.open = !this.open;
		this.#keep();
	}

	close(): void {
		this.open = false;
		this.#keep();
	}

	/** A new route that begins at a place: a site whose page the pilot came from. */
	startAt(place: RoutePlace): void {
		this.open = true;
		this.#change([place]);
	}

	/** The next place of the route; the place it is at already adds nothing. */
	add(place: RoutePlace): void {
		const last = this.places.at(-1);
		if (last && last.lat === place.lat && last.lon === place.lon) return;
		this.#change([...this.places, place]);
	}

	/**
	 * A place of the route moved to somewhere else; it is no longer the
	 * site it was. Where the route comes back to the place, as a triangle
	 * to its start, it comes back to where the place went.
	 */
	move(index: number, to: Coordinate): void {
		const moved = this.places[index];
		if (!moved) return;
		this.#change(
			this.places.map((place) =>
				place.lat === moved.lat && place.lon === moved.lon ? { lat: to.lat, lon: to.lon } : place
			)
		);
	}

	/** Whether "Back to the start" closes the route: three places at least, not closed yet. */
	get canClose(): boolean {
		const first = this.places[0];
		const last = this.places.at(-1);
		return (
			this.places.length >= 3 &&
			Boolean(first && last) &&
			(first.lat !== last?.lat || first.lon !== last?.lon)
		);
	}

	/** "Back to the start": the route ends where it began. */
	backToStart(): void {
		if (this.canClose) this.add(this.places[0]);
	}

	/** "Undo": the last place goes. */
	undo(): void {
		if (this.places.length > 0) this.#change(this.places.slice(0, -1));
	}

	/** "Clear": no places. The planner stays open. */
	clear(): void {
		this.#change([]);
	}

	#change(places: RoutePlace[]): void {
		this.places = places;
		this.#keep();
		void this.#again();
	}

	#keep(): void {
		const route: KeptRoute = { places: this.places, open: this.open };
		keepRoute(this.#storage, route);
	}

	async #again(): Promise<void> {
		const mine = ++this.#asked;
		const places = this.places.map(({ lat, lon }) => ({ lat, lon }));
		try {
			const plan = await this.#ask(places);
			if (mine !== this.#asked) return;
			this.plan = plan;
			this.problem = undefined;
		} catch (error) {
			if (mine !== this.#asked) return;
			this.problem = error instanceof Error ? error.message : String(error);
		}
	}
}
