/**
 * What the app holds for as long as the page is open: the logbook, once the
 * core has started, and whatever the pilot has to be told.
 *
 * It is created once, in the root layout, and reached from every screen
 * through {@link getApp}.
 */

import { createContext } from 'svelte';
import { goto } from '$app/navigation';
import { askToKeepStorage } from '#lib/core/index.ts';
import {
	flightToShow,
	importNotice,
	type ImportReport,
	type Notice
} from '#lib/logbook/import-report.ts';
import { LogbookModel } from '#lib/logbook/model.svelte.ts';
import { flightHref } from './routes';

export class App {
	/** The logbook. Nothing until the core has started. */
	logbook = $state.raw<LogbookModel>();

	/** Why the core could not start, if it could not. */
	failure = $state.raw<string>();

	/** A message waiting to be shown as a dialog. */
	notice = $state.raw<Notice>();

	/** Settles once the core has started, or has failed to. */
	#started: Promise<void> | undefined;

	/** Fetches and starts the Rust core and opens the logbook. Runs in the browser only. */
	start(): Promise<void> {
		this.#started ??= LogbookModel.open().then(
			(logbook) => {
				this.logbook = logbook;
			},
			(error) => {
				this.failure = error instanceof Error ? error.message : String(error);
			}
		);
		return this.#started;
	}

	/**
	 * Imports the files the pilot picked or dropped. Files dropped while the
	 * app is still starting wait for it.
	 */
	async importFiles(files: readonly File[]): Promise<void> {
		await this.#started;
		if (!this.logbook || files.length === 0) return;
		await this.#finish(await this.logbook.importFiles(files));
	}

	async addSample(): Promise<void> {
		await this.#started;
		if (!this.logbook) return;
		await this.#finish(await this.logbook.addSample());
	}

	/**
	 * After an import: open the flight if there was exactly one new one, and
	 * say so if something needs saying.
	 */
	async #finish(report: ImportReport): Promise<void> {
		// From now on there is something worth keeping.
		if (report.added.length > 0 && this.logbook?.durable) askToKeepStorage();
		this.notice = importNotice(report);
		const id = flightToShow(report);
		if (id) await goto(flightHref(id));
	}
}

export const [getApp, setApp] = createContext<App>();
