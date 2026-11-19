/**
 * The pilot's logbook as the screens see it.
 *
 * The model contains no flying logic. The core stores the flights, works
 * out every number and groups them; the model asks for the result and keeps
 * the latest copy for the screens to draw. It is the browser's counterpart
 * of the iPhone app's `LogbookModel.swift`.
 */

import {
	type CoreClient,
	type FlightAnalysis,
	type FlightFile,
	type FlightView,
	isCoreError,
	type LogbookStatistics,
	type LogbookView
} from '#lib/core/index.ts';
import sampleUrl from '../../../../ios/Venti/Resources/sample.igc?url';
import { emptyReport, type ImportReport } from './import-report';

export class LogbookModel {
	/** The logbook as the core last described it. */
	view: LogbookView;

	/** How far a running import has got, for the screen to say. */
	progress = $state.raw<{ done: number; of: number }>();

	#core: CoreClient;

	private constructor(core: CoreClient, view: LogbookView) {
		this.#core = core;
		this.view = $state.raw(view);
	}

	/** Opens the logbook the browser holds for this site. */
	static async open(core: CoreClient): Promise<LogbookModel> {
		return new LogbookModel(core, await core.logbook());
	}

	/**
	 * Whether the flights are still there after the page is closed. They are
	 * not when the browser gives the page no storage.
	 */
	get durable(): boolean {
		return this.#core.durable;
	}

	/** The version of the core, for bug reports. */
	get coreVersion(): string {
		return this.#core.version;
	}

	/** Every flight's outline, for the map of all flights. */
	get outlines() {
		return this.view.years.flatMap((year) => year.flights.map((flight) => flight.outline));
	}

	/**
	 * The logbook as a whole: the last twelve months, the gliders flown and
	 * the pilot's bests, worked out by the core from the logbook as it is now.
	 */
	statistics(): Promise<LogbookStatistics> {
		return this.#core.statistics();
	}

	/** Imports flight files, one after another, and reports on each. */
	async importFiles(files: readonly File[]): Promise<ImportReport> {
		const report = emptyReport();
		this.progress = { done: 0, of: files.length };
		try {
			for (const [index, file] of files.entries()) {
				try {
					const imported = await this.#core.importFlight(await file.arrayBuffer());
					(imported.alreadyInLogbook ? report.alreadyThere : report.added).push(imported.id);
				} catch (error) {
					report.failures.push({ file: file.name, reason: reasonFor(error) });
					if (!isCoreError(error)) {
						// The browser would not store it, and will not store the
						// next one either. Say so for the rest without trying.
						for (const left of files.slice(index + 1)) {
							report.failures.push({ file: left.name, reason: reasonFor(error) });
						}
						break;
					}
				}
				this.progress = { done: index + 1, of: files.length };
			}
		} finally {
			await this.refresh();
			this.progress = undefined;
		}
		return report;
	}

	/**
	 * Imports the made-up flight that comes with the app, so a new pilot has
	 * something to look at.
	 */
	async addSample(): Promise<ImportReport> {
		try {
			const response = await fetch(sampleUrl);
			if (!response.ok) throw new Error(`The server answered ${response.status}.`);
			return await this.importFiles([new File([await response.blob()], 'sample.igc')]);
		} catch (error) {
			return {
				...emptyReport(),
				failures: [
					{
						file: 'sample.igc',
						reason: `The sample flight could not be fetched. ${message(error)}`
					}
				]
			};
		}
	}

	/** One flight, prepared for display. */
	flight(id: string): Promise<FlightView> {
		return this.#core.flight(id);
	}

	/** A flight's file as it was imported, with a name to save it under. */
	flightFile(id: string): Promise<FlightFile> {
		return this.#core.flightFile(id);
	}

	/** What happened in one flight: its thermals and glides, and how it scores. */
	analysis(id: string): Promise<FlightAnalysis> {
		return this.#core.flightAnalysis(id);
	}

	/** Removes a flight for good. */
	async delete(id: string): Promise<void> {
		try {
			await this.#core.deleteFlight(id);
		} finally {
			await this.refresh();
		}
	}

	/**
	 * Asks the core for the logbook as it is now: after a change made here,
	 * by a sync with the server, or in another tab.
	 */
	async refresh(): Promise<void> {
		try {
			this.view = await this.#core.logbook();
		} catch {
			// The screens keep the logbook they have.
		}
	}
}

function message(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

/** Why something could not be imported or deleted, in words for the pilot. */
export function reasonFor(error: unknown): string {
	if (isCoreError(error)) return message(error);
	if (error instanceof Error && error.name === 'QuotaExceededError') {
		return 'This browser has no room left to store flights for this site.';
	}
	return `The browser could not store the logbook. ${message(error)}`;
}
