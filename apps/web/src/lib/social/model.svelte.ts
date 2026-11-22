/**
 * Pilots among each other, as the screens see it: the core's words that no
 * answer carries, and the feed of the pilots one follows, which the home
 * page and the list of everyone's flights both show.
 *
 * Every word and line is the core's (`crates/core/src/social.rs`); the
 * worker sends the requests (`core/social-source.ts`). What is of one
 * flight or one pilot (cheers, comments, the follow button) is asked by
 * the component that shows it and kept nowhere. The feed lives as long as
 * the app is open and is put away when another pilot signs in.
 */

import { untrack } from 'svelte';
import { type ListState, words } from '#lib/community/pages.ts';
import type { CoreClient, FeedRow, SocialWords } from '#lib/core/index.ts';

/** The flights of the pilots followed, as far as they are listed. */
export interface FeedState {
	/** Whether the server has answered. */
	known: boolean;
	rows: FeedRow[];
	/** What the core says of a feed with nothing in it. */
	emptyLine: string;
	/** Whether the server has more below. */
	more: boolean;
	loading: boolean;
	/** Why it is missing, in words. */
	problem: string | undefined;
}

export const NO_FEED: FeedState = {
	known: false,
	rows: [],
	emptyLine: '',
	more: false,
	loading: false,
	problem: undefined
};

/** A feed as the list of public flights is drawn, and the core's lines for each of its flights. */
export function feedAsList(feed: FeedState): ListState {
	return {
		known: feed.known,
		flights: feed.rows.map((row) => row.flight),
		more: feed.more,
		loading: feed.loading,
		problem: feed.problem,
		absent: false
	};
}

/** The core's lines of cheers and comments for each flight of a feed, by the flight's address. */
export function saidOf(
	rows: FeedRow[]
): Record<string, { cheers: string; comments: string } | undefined> {
	return Object.fromEntries(
		rows.map((row) => [row.flight.id, { cheers: row.cheersLine, comments: row.commentsLine }])
	);
}

export class SocialModel {
	/** The core's words, once asked for. */
	words = $state.raw<SocialWords>();

	feed = $state.raw<FeedState>(NO_FEED);

	#core: CoreClient;
	#next: string | undefined;
	/** Whose feed is held, by handle. */
	#whose: string | undefined;
	/** The question last asked: an answer to an older one is dropped. */
	#asking: object | undefined;

	constructor(core: CoreClient) {
		this.#core = core;
		void core.socialWords().then(
			(said) => (this.words = said),
			() => {}
		);
	}

	get core(): CoreClient {
		return this.#core;
	}

	/**
	 * Shows the feed of the pilot signed in as `handle`, from its top. One
	 * that is there for the same pilot is left as it is; nothing is asked
	 * for nobody.
	 */
	showFeed(handle: string | undefined): void {
		untrack(() => {
			if (handle === this.#whose && (this.feed.known || this.feed.loading)) return;
			this.#whose = handle;
			this.#next = undefined;
			this.#asking = undefined;
			this.feed = NO_FEED;
			if (handle) void this.#fetch(undefined);
		});
	}

	/** The page below. */
	moreFeed(): void {
		if (this.feed.more && !this.feed.loading) void this.#fetch(this.#next);
	}

	/** From the top again: after a failure, or after the pilot followed someone. */
	againFeed(): void {
		if (this.#whose) void this.#fetch(undefined);
	}

	async #fetch(before: string | undefined): Promise<void> {
		const question = {};
		this.#asking = question;
		this.feed = { ...this.feed, loading: true, problem: undefined };
		try {
			const page = await this.#core.feed(before);
			if (this.#asking !== question) return;
			if (!page) {
				this.feed = NO_FEED;
				return;
			}
			this.#next = page.next;
			this.feed = {
				known: true,
				rows: before === undefined ? page.rows : [...this.feed.rows, ...page.rows],
				emptyLine: page.emptyLine,
				more: page.next !== undefined,
				loading: false,
				problem: undefined
			};
		} catch (error) {
			if (this.#asking !== question) return;
			this.feed = { ...this.feed, loading: false, problem: words(error) };
		}
	}
}
