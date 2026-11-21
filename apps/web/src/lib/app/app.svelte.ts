/**
 * What the app holds for as long as the page is open: the logbook, the
 * wind, the flying sites, who is in the air, the pilot's account, groups
 * and airspace, once the core has started, and whatever the pilot has to
 * be told.
 *
 * It is created once, in the root layout, and reached from every screen
 * through {@link getApp}.
 */

import { createContext } from 'svelte';
import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { AccountModel } from '#lib/account/model.svelte.ts';
import { AirspaceModel } from '#lib/airspace/model.svelte.ts';
import { ALERTS_EVERY_MS, AlertsModel } from '#lib/alerts/model.svelte.ts';
import { CommunityModel } from '#lib/community/model.svelte.ts';
import {
	askToKeepStorage,
	type Coordinate,
	CoreClient,
	type FlightView,
	type ForecastView,
	type CablesMap,
	type SlopesAnswer,
	type SlopesAsked,
	type GlideReach,
	type GlideReachSetting,
	type GroundUnderFlight,
	type RoutePlan,
	type RouteWeatherAnswer,
	type RouteWeatherAsked,
	type SiteOutlook,
	type SiteView
} from '#lib/core/index.ts';
import { format } from '#lib/design/format.ts';
import { ThermalMapModel } from '#lib/forecast/thermal-map.svelte.ts';
import { RadarModel } from '#lib/radar/radar.svelte.ts';
import { GroupsModel } from '#lib/groups/model.svelte.ts';
import { KeptSitesModel } from '#lib/sites/kept.svelte.ts';
import { TodayModel } from '#lib/today/model.svelte.ts';
import {
	flightToShow,
	importNotice,
	type ImportReport,
	type Notice
} from '#lib/logbook/import-report.ts';
import { LiveModel } from '#lib/live/model.svelte.ts';
import { LogbookModel } from '#lib/logbook/model.svelte.ts';
import { SiteEditsModel } from '#lib/sites/edit.svelte.ts';
import { SitesModel } from '#lib/sites/model.svelte.ts';
import { TakeoffsModel } from '#lib/sites/takeoffs.svelte.ts';
import { WindModel } from '#lib/wind/model.svelte.ts';
import { alertsHref, flightHref } from './routes';

export class App {
	/** The logbook. Nothing until the core has started. */
	logbook = $state.raw<LogbookModel>();

	/** Live wind for the map. Nothing until the core has started. */
	wind = $state.raw<WindModel>();

	/** Flying sites for the map. Nothing until the core has started. */
	sites = $state.raw<SitesModel>();

	/** Who is in the air, for the map. Nothing until the core has started. */
	live = $state.raw<LiveModel>();

	/** Other pilots' public flights. Nothing until the core has started. */
	everyone = $state.raw<CommunityModel>();

	/** The take-offs around a place, and what the forecast says of each. */
	today = $state.raw<TodayModel>();

	/** Adding and correcting a site of Venti's own layer. Nothing until the core has started. */
	siteEdits = $state.raw<SiteEditsModel>();
	/**
	 * What public flights say about where pilots take off: a site's flights,
	 * the map's take-offs without a site and the offer to add one.
	 */
	takeoffs = $state.raw<TakeoffsModel>();
	/** The sites the pilot marks as their own, kept in this browser and, signed in, on the account. */
	kept = $state.raw<KeptSitesModel>();

	/** The kind of day, square by square, for the map's "Forecast" layer. */
	thermals = $state.raw<ThermalMapModel>();

	/** The rain radar, for the map's "Radar" layer. */
	radar = $state.raw<RadarModel>();

	/** The pilot's account on the Venti server. Nothing until the core has started. */
	account = $state.raw<AccountModel>();

	/** The pilot's alarms and what they fired. Nothing until the core has started. */
	alerts = $state.raw<AlertsModel>();

	/**
	 * The pilot's groups and their plans for a flying day. Nothing until
	 * the core has started.
	 */
	groups = $state.raw<GroupsModel>();

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
				this.siteEdits = new SiteEditsModel(core);
				this.takeoffs = new TakeoffsModel(core);
				this.live = new LiveModel(core);
				this.everyone = new CommunityModel(core);
				this.today = new TodayModel(core);
				const kept = new KeptSitesModel(core);
				this.kept = kept;
				this.thermals = new ThermalMapModel(core);
				this.radar = new RadarModel(core);
				const logbook = await LogbookModel.open(core);
				const alerts = new AlertsModel(core, () => void goto(`${alertsHref()}#fired`));
				this.alerts = alerts;
				void alerts.reread();
				// A sync may have fetched or removed flights, and take-offs the
				// pilot keeps as their own; what the alarms fired is fetched
				// with it.
				const account = await AccountModel.open(core, async (keptSites) => {
					await logbook.refresh();
					await kept.synced(keptSites).catch(() => {});
					void alerts.sync();
				});
				const airspace = await AirspaceModel.open(core);
				// Another tab imported, deleted, synced or signed out: show
				// what is stored now.
				core.onChangedElsewhere = () => {
					void logbook.refresh();
					void account.refresh();
					void alerts.reread();
					void airspace.reload();
					kept.changedElsewhere();
				};
				this.logbook = logbook;
				this.account = account;
				this.groups = new GroupsModel(core, account);
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
	 * What the forecast says about flying from a site, by Venti's clock
	 * now. Nothing from a site without Venti behind it. Not kept.
	 */
	async siteOutlook(site: SiteView): Promise<SiteOutlook | undefined> {
		await this.#started;
		return this.#core?.siteOutlook(site);
	}

	/**
	 * What the core says of a route drawn through these places, in order:
	 * each leg, the whole, and what it would score flown as drawn. Nothing
	 * if the core could not start.
	 */
	async planRoute(places: Coordinate[]): Promise<RoutePlan | undefined> {
		// The map tab asks for its kept route as it opens, which can be
		// before the layout has started the core; a page being written out
		// at build time asks nothing.
		if (!browser) return undefined;
		await this.start();
		return this.#core?.planRoute(places);
	}

	/**
	 * A route as the pilot means to fly it, with the weather along it on
	 * a day of the thermal map, and the heights to leave each place at.
	 * Nothing if the core could not start.
	 */
	async routeWeather(asked: RouteWeatherAsked): Promise<RouteWeatherAnswer | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.routeWeather(asked);
	}

	/**
	 * What a glide reaches from a place on the map, over the ground the
	 * worker fetches around it. Nothing if the core could not start.
	 */
	async glideReach(setting: GlideReachSetting): Promise<GlideReach | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.glideReach(setting);
	}

	/** Lets go of the ground kept for the glide's reach. */
	async forgetGlideReach(): Promise<void> {
		await this.#core?.forgetGlideReach();
	}

	/** How steep the ground is over a box of the map, cell by cell; nothing for a question asked since. */
	async slopes(asked: SlopesAsked): Promise<SlopesAnswer | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.slopes(asked);
	}

	/** Lets go of the ground kept for the map of slopes. */
	async forgetSlopes(): Promise<void> {
		await this.#core?.forgetSlopes();
	}

	/**
	 * The cables around a place, as the core makes them a map layer.
	 * Nothing from a site without a server, or if the core could not start.
	 */
	async cables(place: Coordinate): Promise<CablesMap | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.cables(place);
	}

	/** Numbers the questions about the ground under a flight. */
	#groundAsked = 0;

	/**
	 * The ground under a flight, for its altitude profile: read by the core
	 * from the map's elevation tiles, which are fetched for it. Empty where
	 * there is none to be had. Not kept: it is asked for when a flight is
	 * looked at. `letGo` says that the answer is no longer wanted, so that
	 * tiles not fetched yet are left where they are.
	 */
	groundUnder(flight: Pick<FlightView, 'bounds' | 'track'>): {
		ground: Promise<GroundUnderFlight[]>;
		letGo: () => void;
	} {
		const asking = (this.#groundAsked += 1);
		return {
			ground: (async () => {
				await this.#started;
				const found = await this.#core?.flightGround({
					asking,
					bounds: flight.bounds,
					track: flight.track
				});
				return found ?? [];
			})(),
			letGo: () => void this.#core?.forgetFlightGround(asking).catch(() => {})
		};
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
		// What the alarms fired, while a page is open; the account's sync
		// fetches it too, but that pauses a minute between runs.
		const alerts = setInterval(() => {
			if (this.account?.signedIn) void this.alerts?.sync();
		}, ALERTS_EVERY_MS);
		return () => {
			document.removeEventListener('visibilitychange', sync);
			clearInterval(alerts);
		};
	}
}

export const [getApp, setApp] = createContext<App>();
