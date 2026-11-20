/**
 * Other pilots' flights as a list that is read a page at a time: what is
 * asked for, what is kept while the server does not answer, and which days
 * a pilot means. The iPhone app's `CommunityModel.swift` follows the same
 * rules, without the days.
 *
 * What to ask the server and how to read the answer is the core's, in the
 * worker. This file knows nothing about the page: `model.svelte.ts`
 * connects it to the screens.
 */

import type { PublicFlight, PublicFlightsOrder } from '#lib/core/index.ts';
import { isInWords } from '#lib/core/index.ts';

/** One page of a list, as the worker hands it on. */
export interface FlightsPage {
	flights: PublicFlight[];
	/** What to ask the page after it with. Nothing on the last. */
	next: string | undefined;
}

/** What a screen is shown of a list. */
export interface ListState {
	/** Whether the server has answered what is asked. */
	known: boolean;
	/** The flights listed so far, in the order asked for. */
	flights: PublicFlight[];
	/** Whether the server has more than is listed. */
	more: boolean;
	/** Whether a page is being fetched. */
	loading: boolean;
	/** Why the list is missing, or stops short, in words for the pilot. */
	problem: string | undefined;
	/** Whether this site turned out to have no server to ask. */
	absent: boolean;
}

export const NOT_LISTED: ListState = {
	known: false,
	flights: [],
	more: false,
	loading: false,
	problem: undefined,
	absent: false
};

/**
 * One list, read from its top and then a page at a time. Another order,
 * other days or another pilot are another list: whoever holds this lets go
 * of it and makes a new one, and what the old one still hears from the
 * server reaches nobody.
 */
export class PagedFlights {
	#ask: (after: string | undefined) => Promise<FlightsPage | undefined>;
	#tell: (state: ListState) => void;
	#state = NOT_LISTED;
	/** What to ask the next page with. */
	#next: string | undefined;

	constructor(
		ask: (after: string | undefined) => Promise<FlightsPage | undefined>,
		tell: (state: ListState) => void
	) {
		this.#ask = ask;
		this.#tell = tell;
	}

	/** Asks for the list from its top. What is listed stays until the answer is there. */
	load(): Promise<void> {
		return this.#fetch(undefined);
	}

	/** Asks for the page after the last one, if the server has one. */
	async more(): Promise<void> {
		if (this.#state.more) await this.#fetch(this.#next);
	}

	async #fetch(after: string | undefined): Promise<void> {
		// One question at a time: a second page asked for twice would be
		// listed twice.
		if (this.#state.loading) return;
		this.#set({ ...this.#state, loading: true, problem: undefined });
		try {
			const page = await this.#ask(after);
			if (!page) {
				this.#set({ ...NOT_LISTED, absent: true });
				return;
			}
			this.#next = page.next;
			this.#set({
				known: true,
				flights: after === undefined ? page.flights : [...this.#state.flights, ...page.flights],
				more: page.next !== undefined,
				loading: false,
				problem: undefined,
				absent: false
			});
		} catch (error) {
			// What is listed stays: those flights were flown all the same.
			this.#set({ ...this.#state, loading: false, problem: words(error) });
		}
	}

	#set(state: ListState): void {
		this.#state = state;
		this.#tell(state);
	}
}

/** Why flights could not be fetched, for the pilot. */
export function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	return isInWords(error) ? said : `Other pilots’ flights could not be fetched. ${said}`;
}

/**
 * What a pilot asks of everyone's flights: their order, and the days they
 * were flown on. A day is written as a date field writes it, "2026-08-15",
 * and an empty one is no bound.
 */
export interface Asked {
	order: PublicFlightsOrder;
	/** The first day. */
	from: string;
	/** The last day, itself included. */
	to: string;
}

export const EVERYTHING: Asked = { order: 'newest', from: '', to: '' };

export function sameAsked(first: Asked, second: Asked): boolean {
	return first.order === second.order && first.from === second.from && first.to === second.to;
}

const DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * The first second of a day where the reader is, and of the day so many
 * days later, in seconds since 1970-01-01 UTC. The browser knows when its
 * clock is put forward or back, so a day of 23 or 25 hours ends where it
 * ends.
 */
function startOfDay(year: number, month: number, day: number): number {
	return new Date(year, month - 1, day).getTime() / 1000;
}

/**
 * The span of time a pilot means by "from this day to that day": from the
 * first second of the one to the first second of the day after the other,
 * by the pilot's own clock. This is what the server's API asks of a client
 * that means the pilots' own day, with the offset already inside the
 * moments. A day that is not written as a date is no bound.
 */
export function span(
	asked: Pick<Asked, 'from' | 'to'>,
	start: (year: number, month: number, day: number) => number = startOfDay
): { fromUnixS: number | undefined; toUnixS: number | undefined } {
	const from = DAY.exec(asked.from);
	const to = DAY.exec(asked.to);
	return {
		fromUnixS: from ? start(Number(from[1]), Number(from[2]), Number(from[3])) : undefined,
		toUnixS: to ? start(Number(to[1]), Number(to[2]), Number(to[3]) + 1) : undefined
	};
}

/**
 * The days after the pilot has set one of the two fields. Choosing a first
 * day alone means that day: the last day follows it, and so does a last day
 * that would lie before it. A last day chosen before the first moves the
 * first. Emptying a field removes that bound and leaves the other.
 */
export function withDay(
	asked: Asked,
	field: 'from' | 'to',
	value: string
): Pick<Asked, 'from' | 'to'> {
	const day = DAY.test(value) ? value : '';
	if (field === 'from') {
		// Dates written this way are in order when their text is.
		const follows = day !== '' && (asked.to === '' || asked.to < day);
		return { from: day, to: follows ? day : asked.to };
	}
	const leads = day !== '' && asked.from !== '' && asked.from > day;
	return { from: leads ? day : asked.from, to: day };
}

/** What is asked for, read from a page's address. Anything else in it is left out. */
export function askedIn(query: Pick<URLSearchParams, 'get'>): Asked {
	const day = (name: string) => {
		const value = query.get(name) ?? '';
		return DAY.test(value) ? value : '';
	};
	return {
		order: query.get('order') === 'best' ? 'best' : 'newest',
		from: day('from'),
		to: day('to')
	};
}

/** The same, written into an address: nothing at all for the newest flights of any day. */
export function queryOf(asked: Asked): string {
	const query = new URLSearchParams();
	if (asked.order === 'best') query.set('order', 'best');
	if (asked.from) query.set('from', asked.from);
	if (asked.to) query.set('to', asked.to);
	const text = query.toString();
	return text ? `?${text}` : '';
}

/** What a place's page asks for besides the place, read from its address. */
export function placeAskedIn(query: Pick<URLSearchParams, 'get'>): {
	at: { lat: number; lon: number } | undefined;
	site: string;
	order: PublicFlightsOrder;
	season: string;
} {
	const lat = Number(query.get('lat') ?? Number.NaN);
	const lon = Number(query.get('lon') ?? Number.NaN);
	const real = Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 89;
	return {
		at: real && Math.abs(lon) <= 180 ? { lat, lon } : undefined,
		site: query.get('site') ?? '',
		order: query.get('order') === 'best' ? 'best' : 'newest',
		season: query.get('season') ?? ''
	};
}

/** The order and the season, written into a place's address: nothing for the newest of this year. */
export function placeQuery(asked: { order: PublicFlightsOrder; season: string }): string {
	const query = new URLSearchParams();
	if (asked.order === 'best') query.set('order', 'best');
	if (asked.season) query.set('season', asked.season);
	const text = query.toString();
	return text ? `?${text}` : '';
}
