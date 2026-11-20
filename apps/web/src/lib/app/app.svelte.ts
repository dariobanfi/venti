/**
 * What the app holds for as long as the page is open: the logbook, the
 * wind, the flying sites, who is in the air, the pilot's account and
 * airspace, once the core has started, and whatever the pilot has to be
 * told.
 *
 * It is created once, in the root layout, and reached from every screen
 * through {@link getApp}.
 */

import { createContext } from 'svelte';
import { goto } from '$app/navigation';
import { AccountModel } from '#lib/account/model.svelte.ts';
import { AirspaceModel } from '#lib/airspace/model.svelte.ts';
import {
	askToKeepStorage,
	type Coordinate,
	CoreClient,
	type ForecastView
} from '#lib/core/index.ts';
import { format } from '#lib/design/format.ts';
import {
	flightToShow,
	importNotice,
	type ImportReport,
	type Notice
} from '#lib/logbook/import-report.ts';
import { LiveModel } from '#lib/live/model.svelte.ts';
import { LogbookModel } from '#lib/logbook/model.svelte.ts';
import { SitesModel } from '#lib/sites/model.svelte.ts';
import { WindModel } from '#lib/wind/model.svelte.ts';
import { flightHref } from './routes';

export class App {
	/** The logbook. Nothing until the core has started. */
	logbook = $state.raw<LogbookModel>();

	/** Live wind for the map. Nothing until the core has started. */
	wind = $state.raw<WindModel>();

	/** Flying sites for the map. Nothing until the core has started. */
	sites = $state.raw<SitesModel>();

	/** Who is in the air, for the map. Nothing until the core has started. */
	live = $state.raw<LiveModel>();

	/** The pilot's account on the Venti server. Nothing until the core has started. */
	account = $state.raw<AccountModel>();

	/** The pilot's airspace, for the maps. Nothing until the core has started. */
	airspace = $state.raw<AirspaceModel>();

	/** Why the core could not start, if it could not. */
	failure = $state.raw<string>();

	/** A message waiting to be shown as a dialog. */
	notice = $state.raw<Notice>();

	/** Settles once the core has started, or has failed to. */
	#started: Promise<void> | undefined;

	/** The core, once it has started. */
	#core: CoreClient | undefined;

	/** Fetches and starts the Rust core and opens the logbook. Runs in the browser only. */
	start(): Promise<void> {
		this.#started ??= CoreClient.open()
			.then(async (core) => {
				this.#core = core;
				this.wind = new WindModel(core);
				this.sites = new SitesModel(core);
				this.live = new LiveModel(core);
				const logbook = await LogbookModel.open(core);
				// A sync may have fetched or removed flights.
				const account = await AccountModel.open(core, () => logbook.refresh());
				const airspace = await AirspaceModel.open(core);
				// Another tab imported, deleted, synced or signed out: show
				// what is stored now.
				core.onChangedElsewhere = () => {
					void logbook.refresh();
					void account.refresh();
					void airspace.reload();
				};
				this.logbook = logbook;
				this.account = account;
				this.airspace = airspace;
			})
			.catch((error) => {
				this.failure = error instanceof Error ? error.message : String(error);
			});
		return this.#started;
	}

	/**
	 * What the server's weather models say about a place. Nothing comes
	 * back from a site without a server. Not kept: a forecast is asked for
	 * when it is looked at.
	 */
	async forecast(place: Coordinate): Promise<ForecastView | undefined> {
		await this.#started;
		return this.#core?.forecast(place);
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

	/**
	 * Brings in the airspace files the pilot picked or dropped, and says
	 * what each gave and what it left out.
	 */
	async importAirspace(files: readonly File[]): Promise<void> {
		await this.#started;
		if (!this.airspace || files.length === 0) return;
		this.notice = await this.airspace.importFiles(files, format());
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
		// With an account, new flights go to the server without being asked.
		if (report.added.length > 0) void this.account?.sync({ force: true });
		this.notice = importNotice(report);
		const id = flightToShow(report);
		if (id) await goto(flightHref(id));
	}

	/**
	 * Deletes a flight from the logbook. With an account, the server's copy
	 * goes with the sync that follows.
	 */
	async deleteFlight(id: string): Promise<void> {
		await this.logbook?.delete(id);
		void this.account?.sync({ force: true });
	}

	/**
	 * Keeps the logbook in step with the server while the page is open: now,
	 * and whenever the page comes back to the front. Returns what to call
	 * when the page goes away.
	 */
	keepInStep(): () => void {
		const sync = () => {
			if (document.visibilityState === 'visible') void this.account?.sync();
		};
		void this.#started?.then(sync);
		document.addEventListener('visibilitychange', sync);
		return () => document.removeEventListener('visibilitychange', sync);
	}
}

export const [getApp, setApp] = createContext<App>();
