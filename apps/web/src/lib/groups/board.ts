/**
 * The board of flying plans as a screen keeps it: when it is asked for,
 * from which day, and what is shown while the server answers or does not.
 * The iPhone app's `GroupsModel.load` has the same rules.
 *
 * - The board is asked for from the pilot's own today on: the date of the
 *   browser's clock where the pilot is. The server does not know where
 *   that is, and its own day is UTC's.
 * - Signed out there is no board, and the server is not asked.
 * - A board is one account's. When another pilot has signed in, the board
 *   from before is put away before the server is asked.
 * - Only the newest answer is shown: a board asked for before a change
 *   does not replace the one asked for after it.
 * - An answer that does not come leaves the board as it was, and says why.
 *
 * Nothing here draws: `model.svelte.ts` hands the screens what this holds.
 */

import { isInWords, type PlanBoard } from '#lib/core/index.ts';
import { dayOf } from './days';

/** What a screen shows of the board. */
export interface BoardState {
	/**
	 * The plans of the pilot's groups from today on, and the groups.
	 * Nothing until the server has answered once, and while signed out.
	 */
	board: PlanBoard | undefined;
	/** Whether the server is being asked. */
	loading: boolean;
	/** Why the board is not there or not current, in words for the pilot. */
	problem: string | undefined;
	/**
	 * The pilot's own today, written `2026-11-21`: the day the board was
	 * asked from, by which its days are called "Today" and "Tomorrow".
	 */
	today: string;
}

export class BoardWatch {
	#ask: (fromDay: string) => Promise<PlanBoard>;
	#pilot: () => string | undefined;
	#tell: (state: BoardState) => void;
	#now: () => Date;
	#state: BoardState;
	/** Whose board the state holds: the handle it was asked for. */
	#whose: string | undefined;
	/** Counts the questions: only the newest answer is shown. */
	#asked = 0;

	/**
	 * `pilot` says who is signed in, by handle, or nothing while nobody is.
	 * `ask` asks the server for the board from a day on.
	 */
	constructor(
		ask: (fromDay: string) => Promise<PlanBoard>,
		pilot: () => string | undefined,
		tell: (state: BoardState) => void,
		now: () => Date = () => new Date()
	) {
		this.#ask = ask;
		this.#pilot = pilot;
		this.#tell = tell;
		this.#now = now;
		this.#state = { board: undefined, loading: false, problem: undefined, today: dayOf(now()) };
	}

	get state(): BoardState {
		return this.#state;
	}

	/** Asks for the board: every plan of the pilot's groups from today on. */
	async load(): Promise<void> {
		const asked = (this.#asked += 1);
		const today = dayOf(this.#now());
		const pilot = this.#pilot();
		if (pilot !== this.#whose) {
			this.#whose = pilot;
			this.#set({ board: undefined, loading: false, problem: undefined, today });
		}
		if (pilot === undefined) return;
		this.#set({ ...this.#state, loading: true });
		try {
			const board = await this.#ask(today);
			if (asked === this.#asked) this.#set({ board, loading: false, problem: undefined, today });
		} catch (error) {
			if (asked === this.#asked) {
				this.#set({ ...this.#state, loading: false, problem: words(error) });
			}
		}
	}

	#set(state: BoardState): void {
		this.#state = state;
		this.#tell(state);
	}
}

/** Why something about groups and plans did not work, for the pilot. */
export function words(error: unknown): string {
	const said = error instanceof Error ? error.message : String(error);
	// The core's refusals and a server out of reach are worded already.
	return isInWords(error) ? said : `The groups and their plans could not be reached. ${said}`;
}
