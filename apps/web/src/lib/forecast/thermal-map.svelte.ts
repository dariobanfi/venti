/**
 * The thermal map as the map tab sees it, as Android's and the iPhone's
 * `ThermalMapModel`: asks the core in its worker for the map, keeps the
 * newest one of this visit, and remembers which day the pilot looks at.
 *
 * A map that was fetched stays when a later asking fails: the card shows
 * its run's time, so an old map is never shown as new. `thermal-map.ts`
 * holds the rules.
 */

import type { CoreClient, ThermalMap } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { dayOf, isDue, WORDS } from './thermal-map';

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

	#core: Pick<CoreClient, 'thermalMap'>;
	#clock: () => number;
	#triedMs: number | undefined;
	#fetchedMs: number | undefined;

	constructor(core: Pick<CoreClient, 'thermalMap'>, clock: () => number = Date.now) {
		this.#core = core;
		this.#clock = clock;
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
