/**
 * What public flights say about where pilots take off, as the screens see
 * it: a site's flights for its page, the map's layer of take-offs without
 * a site with what the core says over the map, and the offer to add such
 * a place to a pilot who took off there.
 *
 * Every word, the refusal of a part of the map too large, who is offered
 * what and the draft are the core's (`crates/core/src/takeoffs.rs`), asked
 * through the worker (`core/takeoffs-source.ts`). This file holds what the
 * screens watch, whether the layer is on (kept in this browser), and a
 * timer that waits for the map to rest.
 */

import type {
	Box,
	CandidateOffer,
	Coordinate,
	CoreClient,
	SiteCandidates,
	SiteCandidateView,
	SiteFlights,
	SiteFlightsWords
} from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';
import { candidatesOn, keepCandidates } from '#lib/map/layers.ts';

/** How long the map has to rest before the server is asked, in milliseconds. */
const SETTLE_MS = 400;

/** How far the browser's clock is ahead of UTC now, in seconds. */
export function utcOffsetS(at = new Date()): number {
	return -at.getTimezoneOffset() * 60;
}

/** The offer the core gives for one candidate, matched by its place. */
export function offerFor(
	offers: readonly CandidateOffer[],
	candidate: Pick<SiteCandidateView, 'position'>
): CandidateOffer | undefined {
	return offers.find(
		(offer) =>
			offer.candidate.position.lat === candidate.position.lat &&
			offer.candidate.position.lon === candidate.position.lon
	);
}

/** A box around places, as the core asks about one; nothing without a place. */
export function boxAround(places: readonly Coordinate[]): Box | undefined {
	if (places.length === 0) return undefined;
	const lats = places.map((place) => place.lat);
	const lons = places.map((place) => place.lon);
	return {
		south: Math.min(...lats),
		west: Math.min(...lons),
		north: Math.max(...lats),
		east: Math.max(...lons)
	};
}

/**
 * How far from Today's place a take-off may lie for its box, in degrees:
 * half the most the core asks about (2 degrees), so that a kept take-off
 * far away does not make the box one the core refuses.
 */
const TODAY_REACH_DEG = 1;

/** The box around Today's take-offs near its place, with the place itself in it. */
export function todayBox(places: readonly Coordinate[], middle: Coordinate): Box {
	const near = places.filter(
		(place) =>
			Math.abs(place.lat - middle.lat) <= TODAY_REACH_DEG &&
			Math.abs(place.lon - middle.lon) <= TODAY_REACH_DEG
	);
	return boxAround([middle, ...near])!;
}

export class TakeoffsModel {
	/** The words around it all, the core's, once they were asked for. */
	words = $state.raw<SiteFlightsWords>();

	/** Whether the map draws the take-offs without a site: off until the pilot switches it on. */
	layerOn = $state(candidatesOn(globalThis.localStorage));

	/** The candidates of the part of the map last asked about. */
	candidates = $state.raw<SiteCandidateView[]>([]);
	/** The answer they came in, for the offers. */
	answer = $state.raw<SiteCandidates>();
	/**
	 * What the core says over the map instead: "Zoom in to see …" for a part
	 * too large, or why the server could not be asked. Empty otherwise.
	 */
	over = $state('');
	/** The core's words for a part of the map that holds more than one answer lists. */
	more = $state('');
	/** What the core offers this pilot for the candidates of the answer. */
	offers = $state.raw<CandidateOffer[]>([]);

	#core: CoreClient;
	#timer: ReturnType<typeof setTimeout> | undefined;
	/** Counts the questions, so that an answer overtaken by a later one is dropped. */
	#asked = 0;
	#box: Box | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
		void core.siteFlightsWords().then(
			(words) => (this.words = words),
			() => {}
		);
	}

	/** What the public flights that began at a site say about it. Throws in the core's words. */
	site(id: string): Promise<SiteFlights> {
		return this.#core.siteFlights(id);
	}

	/** Switches the layer on or off, and remembers it in this browser. */
	switchLayer(on: boolean): void {
		this.layerOn = on;
		keepCandidates(globalThis.localStorage, on);
		if (on && this.#box) this.show(this.#box);
		else this.#clear();
	}

	/** The part of the map in view came to rest: asked about once it has rested, while the layer is on. */
	show(box: Box): void {
		this.#box = box;
		clearTimeout(this.#timer);
		if (!this.layerOn) {
			this.#clear();
			return;
		}
		this.#timer = setTimeout(() => void this.#load(box), SETTLE_MS);
	}

	/** The candidates of a part of the map and what is offered for them, for Today. */
	async offersIn(box: Box): Promise<CandidateOffer[]> {
		const answer = await this.#core.siteCandidates(box);
		return this.#core.candidateOffers(answer, utcOffsetS());
	}

	/** "Not this one": the place is not offered again in this browser. */
	async setAside(offer: CandidateOffer): Promise<void> {
		await this.#core.setCandidateAside(offer.candidate.position);
		this.offers = this.offers.filter((one) => one !== offer);
	}

	async #load(box: Box): Promise<void> {
		const asked = ++this.#asked;
		try {
			const answer = await this.#core.siteCandidates(box);
			if (asked !== this.#asked || !this.layerOn) return;
			this.answer = answer;
			this.candidates = answer.candidates;
			this.more = answer.moreWords;
			this.over = '';
			const offers = await this.#core.candidateOffers(answer, utcOffsetS()).catch(() => []);
			if (asked === this.#asked) this.offers = offers;
		} catch (error) {
			if (asked !== this.#asked) return;
			this.#clear();
			// The core's words for a part too large, or a server out of reach.
			this.over = isInWords(error) && error instanceof Error ? error.message : '';
		}
	}

	#clear(): void {
		this.#asked += 1;
		this.answer = undefined;
		this.candidates = [];
		this.offers = [];
		this.over = '';
		this.more = '';
	}
}
