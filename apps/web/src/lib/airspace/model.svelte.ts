/**
 * The pilot's airspace as the screens see it.
 *
 * It comes from two places: the Venti server this site is served from,
 * which the app asks for the airspace around what the map shows and whose
 * answer the core keeps in the browser, and files the pilot brings in. The
 * core keeps both, reads them and answers every question about them. This
 * class asks for the results, decides when the server is asked, and hands
 * over the files the pilot picked. It is the browser's counterpart of the
 * iPhone app's `AirspaceModel.swift`.
 */

import {
	type AirspaceFile,
	type AirspaceHeightFilter,
	type AirspaceOverPlace,
	type AirspaceShape,
	type Bounds,
	type Coordinate,
	type CoreClient,
	type Credit,
	type KeptAirspace
} from '#lib/core/index.ts';
import type { Notice } from '#lib/logbook/import-report.ts';
import { reasonFor } from '#lib/logbook/model.svelte.ts';
import { ceilingKept, keepCeiling } from './height';
import { importSummary, type Writing } from './words';

/** After a request that failed, the server is left alone for this long, in milliseconds. */
export const RETRY_AFTER_MS = 5 * 60_000;

/** Whether the pilot has switched the server's airspace off: a choice this browser remembers. */
export interface ServerChoice {
	read(): boolean;
	write(switchedOff: boolean): void;
}

const CHOICE_KEY = 'venti.airspace-without-server';

/** The choice as the browser keeps it. A browser that keeps nothing forgets it with the page. */
export const browserChoice: ServerChoice = {
	read() {
		try {
			return localStorage.getItem(CHOICE_KEY) === '1';
		} catch {
			return false;
		}
	},
	write(switchedOff) {
		try {
			if (switchedOff) localStorage.setItem(CHOICE_KEY, '1');
			else localStorage.removeItem(CHOICE_KEY);
		} catch {
			// Kept for this visit only.
		}
	}
};

export class AirspaceModel {
	/** The files the pilot has brought in. */
	files = $state.raw<AirspaceFile[]>([]);

	/** The airspace kept from the server, one entry for each source. */
	kept = $state.raw<KeptAirspace[]>([]);

	/**
	 * Counts the changes to the pilot's airspace. A map draws again when it
	 * goes up.
	 */
	revision = $state(0);

	/**
	 * Set when there is airspace but it could not be read: the maps would
	 * show none, and the pilot must know.
	 */
	problem = $state.raw<string>();

	/** Why the server's airspace could not be fetched, in words. */
	serverProblem = $state.raw<string>();

	/** Whether the server is asked for airspace at all. The pilot's choice, remembered. */
	usesServer = $state(true);

	/**
	 * The height the maps' airspace is filtered by, in metres: airspace that
	 * certainly begins above it is not drawn. Nothing for all heights.
	 */
	ceilingM = $state<number>();

	#core: CoreClient;
	#choice: ServerChoice;
	#now: () => number;
	#fetching = false;
	#failedAt: number | undefined;

	private constructor(core: CoreClient, choice: ServerChoice, now: () => number) {
		this.#core = core;
		this.#choice = choice;
		this.#now = now;
		this.usesServer = !choice.read();
		this.ceilingM = ceilingKept(globalThis.localStorage);
	}

	/** Opens the airspace the browser holds for this site. */
	static async open(
		core: CoreClient,
		choice: ServerChoice = browserChoice,
		now: () => number = () => Date.now()
	): Promise<AirspaceModel> {
		const model = new AirspaceModel(core, choice, now);
		await model.reload();
		return model;
	}

	/** Whether there is any airspace to draw. */
	get isEmpty(): boolean {
		return this.files.length === 0 && this.kept.length === 0;
	}

	/**
	 * Whose airspace the server's is, with what their licences ask to be
	 * shown wherever it is.
	 */
	get credits(): Credit[] {
		return this.kept.flatMap((layer) => (layer.credit ? [layer.credit] : []));
	}

	/** The airspaces that touch a part of the map. */
	async shapes(area: Bounds): Promise<AirspaceShape[]> {
		if (this.isEmpty) return [];
		try {
			const shapes = await this.#core.airspaceIn(area, this.ceilingM);
			this.problem = undefined;
			return shapes;
		} catch (error) {
			this.problem = `Your airspace could not be loaded, so none is drawn. ${reasonFor(error)}`;
			return [];
		}
	}

	/** Filters every map's airspace by a height, or by none; the browser remembers it. */
	filterBelow(ceilingM: number | undefined): void {
		if (ceilingM === this.ceilingM) return;
		this.ceilingM = ceilingM;
		keepCeiling(globalThis.localStorage, ceilingM);
		// The maps draw again.
		this.revision += 1;
	}

	/** The heights the maps' airspace can be filtered by, in the core's words. */
	heightFilter(): Promise<AirspaceHeightFilter> {
		return this.#core.airspaceHeightFilter();
	}

	/** What airspace lies over a place, in the core's words. */
	over(place: Coordinate): Promise<AirspaceOverPlace> {
		return this.#core.airspaceOver(place);
	}

	/** Takes in the files the pilot picked or dropped and says what happened, in words. */
	async importFiles(files: readonly File[], text: Pick<Writing, 'count'>): Promise<Notice> {
		const lines: string[] = [];
		for (const file of files) {
			try {
				const result = await this.#core.importAirspace(file.name, await file.arrayBuffer());
				lines.push(importSummary(result, file.name, text));
			} catch (error) {
				lines.push(`${file.name}: ${reasonFor(error)}`);
			}
		}
		await this.reload();
		return { title: 'Airspace', lines };
	}

	/** Removes a file. Its airspace is no longer drawn. */
	async delete(file: AirspaceFile): Promise<void> {
		try {
			await this.#core.deleteAirspaceFile(file.storedName);
		} finally {
			await this.reload();
		}
	}

	/**
	 * Asks the server for the airspace around a place and has it kept, when
	 * the core says it is time. Otherwise nothing is sent. Called whenever
	 * the app learns where the pilot is looking. A server that failed is
	 * left alone for five minutes.
	 */
	async refresh(around: Coordinate): Promise<void> {
		if (!this.usesServer || this.#fetching) return;
		if (this.#failedAt !== undefined && this.#now() - this.#failedAt < RETRY_AFTER_MS) return;
		this.#fetching = true;
		try {
			const kept = await this.#core.refreshAirspace(around);
			this.#failedAt = undefined;
			this.serverProblem = undefined;
			// Nothing came back when the server was not asked.
			if (kept) await this.reload();
		} catch (error) {
			this.#failedAt = this.#now();
			this.serverProblem = reasonFor(error);
		} finally {
			this.#fetching = false;
		}
	}

	/** Asks the server for airspace again from now on. */
	useServer(): void {
		this.usesServer = true;
		this.#choice.write(false);
		this.#failedAt = undefined;
	}

	/** Removes what is kept from the server and stops asking for it. */
	async stopUsingServer(): Promise<void> {
		this.usesServer = false;
		this.#choice.write(true);
		this.serverProblem = undefined;
		try {
			await this.#core.forgetKeptAirspace();
		} finally {
			await this.reload();
		}
	}

	/** Reads what airspace there is now: after a change here, or in another tab. */
	async reload(): Promise<void> {
		try {
			[this.files, this.kept] = await Promise.all([
				this.#core.airspaceFiles(),
				this.#core.keptAirspace()
			]);
			this.problem = undefined;
		} catch (error) {
			this.problem = `Your airspace could not be loaded, so none is drawn. ${reasonFor(error)}`;
		}
		this.revision += 1;
	}
}
