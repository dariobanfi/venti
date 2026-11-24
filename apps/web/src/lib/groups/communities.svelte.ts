/**
 * The pilot's groups as communities (owner decision 26): their own groups
 * with what is unread and the last message, their invites into private
 * groups, the public groups near a place or found by name, and joining,
 * accepting and declining. The requests and their answers are the core's
 * (`core/groups-source.ts`); this class gives them to the screens and puts
 * what went wrong into words. After a change the board of plans is asked
 * for again too, since a plan is posted to a group the pilot is in.
 */

import { untrack } from 'svelte';
import type { AccountModel } from '#lib/account/model.svelte.ts';
import type {
	CommunityDetail,
	CommunityDraft,
	CommunityInvite,
	CommunityView,
	Coordinate,
	CoreClient,
	FoundCommunity,
	GroupJoined
} from '#lib/core/index.ts';
import { words } from './board';

export class CommunitiesModel {
	/** The pilot's groups; nothing until the server has answered, and while signed out. */
	mine = $state.raw<CommunityView[]>();
	/** The pilot's invites into private groups. */
	invites = $state.raw<CommunityInvite[]>([]);
	/** Why the groups are not there or not current, in words. */
	problem = $state<string>();

	#core: CoreClient;
	#account: AccountModel;
	#changed: () => void;
	#asked = 0;
	#made: CommunityDetail | undefined;

	/** `changed` is told after the pilot joined, made or left a group: the board's groups changed. */
	constructor(core: CoreClient, account: AccountModel, changed: () => void = () => {}) {
		this.#core = core;
		this.#account = account;
		this.#changed = changed;
	}

	/** The core, for a group's chat. */
	get core(): CoreClient {
		return this.#core;
	}

	/** Asks for the pilot's groups and invites. Signed out: none. */
	load(): Promise<void> {
		return untrack(async () => {
			const asking = (this.#asked += 1);
			if (!this.#account.signedIn) {
				this.mine = undefined;
				this.invites = [];
				return;
			}
			try {
				const [mine, invites] = await Promise.all([
					this.#core.communities(),
					this.#core.communityInvites()
				]);
				if (asking !== this.#asked) return;
				this.mine = mine;
				this.invites = invites;
				this.problem = undefined;
			} catch (error) {
				if (asking === this.#asked) this.problem = words(error);
			}
		});
	}

	/**
	 * Keeps the groups current while a screen shows them: now, for whoever
	 * signs in, and when the tab comes back to the front. For an effect.
	 */
	keepCurrent(): () => void {
		void this.#account.signedIn;
		void this.#account.account?.handle;
		void this.load();
		const back = () => {
			if (document.visibilityState === 'visible') void this.load();
		};
		document.addEventListener('visibilitychange', back);
		return () => document.removeEventListener('visibilitychange', back);
	}

	/** Public groups near a place, by name or place, or both. Throws in words. */
	find(near: Coordinate | undefined, text: string): Promise<FoundCommunity[]> {
		return inWords(() => this.#core.findCommunities(near, text));
	}

	/** Takes the pilot into a public group, or one they were invited into. */
	join(id: string): Promise<GroupJoined> {
		return this.#change(() => this.#core.enterCommunity(id));
	}

	/** Declines an invite, by the pilot's own handle. */
	decline(invite: CommunityInvite): Promise<void> {
		const handle = this.#account.account?.handle ?? '';
		return this.#change(() => this.#core.dropCommunityInvite(invite.group.id, handle));
	}

	/**
	 * Makes a group with the pilot as its owner. The group as it was made
	 * is kept for its page, which then needs not ask for it (`takeMade`).
	 */
	async create(draft: CommunityDraft, isPublic: boolean): Promise<CommunityDetail> {
		const made = await this.#change(() => this.#core.createCommunity(draft, isPublic));
		this.#made = made;
		return made;
	}

	/** The group just made, once, for the page of that group. */
	takeMade(id: string): CommunityDetail | undefined {
		const made = this.#made;
		this.#made = undefined;
		return made?.group.id === id ? made : undefined;
	}

	/** After a change of the pilot's groups somewhere else: the group's page. */
	changed(): void {
		void this.load();
		this.#changed();
	}

	async #change<T>(work: () => Promise<T>): Promise<T> {
		const done = await inWords(work);
		this.changed();
		return done;
	}
}

async function inWords<T>(work: () => Promise<T>): Promise<T> {
	try {
		return await work();
	} catch (error) {
		throw new Error(words(error), { cause: error });
	}
}
