/**
 * What the app holds for as long as the page is open: the pilot's flights
 * on the server, the wind, the flying sites, who is in the air, the pilot's account, groups
 * and airspace, once the core has started, and whatever the pilot has to
 * be told.
 *
 * It is created once, in the root layout, and reached from every screen
 * through {@link getApp}.
 */

import { createContext } from 'svelte';
import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { asset, resolve } from '$app/paths';
import { AccountModel } from '#lib/account/model.svelte.ts';
import { pushOff, pushState, thisBrowser } from '#lib/alerts/push.ts';
import { SocialModel } from '#lib/social/model.svelte.ts';
import { UNREAD_EVERY_MS, UnreadModel } from '#lib/social/unread.svelte.ts';
import { AirspaceModel } from '#lib/airspace/model.svelte.ts';
import { ALERTS_EVERY_MS, AlertsModel } from '#lib/alerts/model.svelte.ts';
import { CommunityModel } from '#lib/community/model.svelte.ts';
import { OpenPlansModel } from '#lib/community/open-plans.svelte.ts';
import { ConnectModel } from '#lib/connect/model.svelte.ts';
import {
	type Bounds,
	type CablesMap,
	canRunWebAssembly,
	type ClimbsAnswer,
	type Coordinate,
	CoreClient,
	type FlightView,
	type FlownAnswer,
	type ForecastView,
	type GlideReach,
	type GlideReachSetting,
	type GroundUnderFlight,
	type MapLayersWords,
	messageOf,
	NO_WEBASSEMBLY,
	type NothingFoundWords,
	type RoutePlan,
	type RouteWeatherAnswer,
	type RouteWeatherAsked,
	type ScreenWords,
	type SiteAirAsked,
	type SiteOutlook,
	type SiteView,
	type SlopesAnswer,
	type SlopesAsked
} from '#lib/core/index.ts';
import { format } from '#lib/design/format.ts';
import { ThermalMapModel } from '#lib/forecast/thermal-map.svelte.ts';
import { RegionForecastModel } from '#lib/forecast/regions.svelte.ts';
import { RadarModel } from '#lib/radar/radar.svelte.ts';
import { GroupsModel } from '#lib/groups/model.svelte.ts';
import { CommunitiesModel } from '#lib/groups/communities.svelte.ts';
import { KeptSitesModel } from '#lib/sites/kept.svelte.ts';
import { TodayModel } from '#lib/today/model.svelte.ts';
import { HomeModel } from '#lib/home/model.svelte.ts';
import { flightToShow, importNotice, type Notice } from '#lib/logbook/import-report.ts';
import { LiveModel } from '#lib/live/model.svelte.ts';
import { MyFlightsModel } from '#lib/logbook/model.svelte.ts';
import { SiteEditsModel } from '#lib/sites/edit.svelte.ts';
import { SitesModel } from '#lib/sites/model.svelte.ts';
import { TakeoffsModel } from '#lib/sites/takeoffs.svelte.ts';
import { WindModel } from '#lib/wind/model.svelte.ts';
import { alertsHref, myFlightHref } from './routes';

export class App {
	/** The pilot's own flights, on the server. Nothing until the core has started. */
	myFlights = $state.raw<MyFlightsModel>();

	/** Live wind for the map. Nothing until the core has started. */
	wind = $state.raw<WindModel>();

	/** Flying sites for the map. Nothing until the core has started. */
	sites = $state.raw<SitesModel>();

	/** Who is in the air, for the map. Nothing until the core has started. */
	live = $state.raw<LiveModel>();

	/** Other pilots' public flights. Nothing until the core has started. */
	everyone = $state.raw<CommunityModel>();

	/**
	 * The core's words for the screens that no answer carries: "Try again",
	 * "Cancel", the planner's menu, the sheets' headings. Nothing until the
	 * core has started.
	 */
	said = $state.raw<ScreenWords>();

	/** Pilots among each other: the core's words and the feed of the pilots followed. */
	social = $state.raw<SocialModel>();

	/** The take-offs around a place, and what the forecast says of each. */
	today = $state.raw<TodayModel>();

	/** What the home page shows of what is going on. Nothing until the core has started. */
	home = $state.raw<HomeModel>();

	/** Adding and correcting a site of Venti's own layer. Nothing until the core has started. */
	siteEdits = $state.raw<SiteEditsModel>();
	/**
	 * What public flights say about where pilots take off: a site's flights,
	 * the map's take-offs without a site and the offer to add one.
	 */
	takeoffs = $state.raw<TakeoffsModel>();
	/** The sites the pilot marks as their own, kept in this browser and, signed in, on the account. */
	kept = $state.raw<KeptSitesModel>();

	/** The kind of day, square by square, and the forecast's hours, for what the map shows. */
	thermals = $state.raw<ThermalMapModel>();
	/** The thermal forecast by region, for the map's thermals. */
	regions = $state.raw<RegionForecastModel>();

	/** The rain radar, for the map's rain. */
	radar = $state.raw<RadarModel>();

	/** The pilot's account on the Venti server. Nothing until the core has started. */
	account = $state.raw<AccountModel>();

	/** The pilot's alarms and what they fired. Nothing until the core has started. */
	alerts = $state.raw<AlertsModel>();

	/** What is new for the pilot, for the bubble on their picture. Nothing until the core has started. */
	unread = $state.raw<UnreadModel>();

	/**
	 * The pilot's groups and their plans for a flying day. Nothing until
	 * the core has started.
	 */
	groups = $state.raw<GroupsModel>();

	/**
	 * The pilot's groups as communities with a chat, their invites, and the
	 * public groups found. Nothing until the core has started.
	 */
	communities = $state.raw<CommunitiesModel>();

	/**
	 * The open plans near a place, for everyone flying from there. Nothing
	 * until the core has started.
	 */
	openPlans = $state.raw<OpenPlansModel>();

	/** The pilot's airspace, for the maps. Nothing until the core has started. */
	airspace = $state.raw<AirspaceModel>();

	/** Why the core could not start, if it could not. */
	failure = $state.raw<string>();

	/**
	 * Whether this browser has WebAssembly switched off, so that the core
	 * cannot run in it at all. The frame then says so in place of every
	 * screen, and `failure` stays empty: the browser's own words for it are
	 * not for the pilot. Asked as the app is made, before any screen is
	 * drawn, so that none is drawn for nothing and none starts the core.
	 */
	withoutWebAssembly = $state.raw(browser && !canRunWebAssembly());

	/** A message waiting to be shown as a dialog. */
	notice = $state.raw<Notice>();

	/** Settles once the core has started, or has failed to. */
	#started: Promise<void> | undefined;

	/** The core, once it has started. */
	#core: CoreClient | undefined;

	/** Fetches and starts the Rust core. Runs in the browser only. */
	start(): Promise<void> {
		// No worker is started where it could only fail. Nothing is set here:
		// a screen being made can be what asks.
		if (this.withoutWebAssembly) this.#started ??= Promise.resolve();
		this.#started ??= CoreClient.open()
			.then(async (core) => {
				this.#core = core;
				this.wind = new WindModel(core);
				this.sites = new SitesModel(core);
				this.siteEdits = new SiteEditsModel(core);
				this.takeoffs = new TakeoffsModel(core);
				this.live = new LiveModel(core);
				this.everyone = new CommunityModel(core);
				this.social = new SocialModel(core);
				void core.screenWords().then(
					(said) => (this.said = said),
					() => {}
				);
				this.today = new TodayModel(core);
				this.home = new HomeModel(core);
				const kept = new KeptSitesModel(core);
				this.kept = kept;
				this.thermals = new ThermalMapModel(core);
				this.regions = new RegionForecastModel(core);
				this.radar = new RadarModel(core);
				const myFlights = new MyFlightsModel(core);
				void myFlights.reload();
				const alerts = new AlertsModel(core, () => void goto(`${alertsHref()}#fired`));
				this.alerts = alerts;
				void alerts.reread();
				const unread = new UnreadModel(core);
				this.unread = unread;
				// A sync may have fetched or removed take-offs the pilot keeps
				// as their own; what the alarms fired is fetched with it. It
				// runs after signing in, when the pilot's flights are read anew.
				const account = await AccountModel.open(core, async (keptSites) => {
					void myFlights.reload();
					await kept.synced(keptSites).catch(() => {});
					void alerts.sync();
					void unread.read();
				});
				const airspace = await AirspaceModel.open(core);
				// Another tab signed in or out, or synced: show what is stored now.
				core.onChangedElsewhere = () => {
					void myFlights.reload();
					void account.refresh();
					void alerts.reread();
					void unread.read();
					void airspace.reload();
					kept.changedElsewhere();
				};
				// Signing out ends this browser's subscription to Web Push, if
				// it has one: the alarms are the account's.
				account.beforeSignOut = async () => {
					const browser = thisBrowser(asset('/push-worker.js'), resolve('/'));
					if (browser && (await pushState(browser)) === 'on') await pushOff(browser, core);
				};
				// Signed out, the pilot's flights are asked for no more.
				account.afterLeaving = () => {
					void myFlights.reload();
					unread.clear();
				};
				this.myFlights = myFlights;
				this.account = account;
				const groups = new GroupsModel(core, account);
				this.groups = groups;
				this.communities = new CommunitiesModel(core, account, () => void groups.load());
				this.openPlans = new OpenPlansModel(core, account);
				this.airspace = airspace;
			})
			.catch((error) => {
				// The page has it and the worker does not.
				if (error instanceof Error && error.name === NO_WEBASSEMBLY) {
					this.withoutWebAssembly = true;
					return;
				}
				this.failure = messageOf(error);
			});
		return this.#started;
	}

	/**
	 * The page `/connect/` for the request its address names: what a program
	 * asks to read, and the pilot's answer. Nothing if the core could not
	 * start. The identifier is handed to the page's model and kept nowhere
	 * else.
	 */
	async agreement(id: string): Promise<ConnectModel | undefined> {
		if (!browser) return undefined;
		await this.start();
		if (!this.#core) return undefined;
		// A session the server no longer takes: the account's screens say so.
		return new ConnectModel(this.#core, id, () => void this.account?.ask());
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

	/** Where pilots climb right now in a part of the map; nothing without a server. */
	async liveClimbs(box: Bounds): Promise<ClimbsAnswer | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.liveClimbs(box);
	}

	/** Where thermals have been in a part of the map, in every month of the year. */
	async flownThermals(box: Bounds): Promise<FlownAnswer | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.flownThermals(box);
	}

	/** What the layers' sheet of the map says, in the core's words; nothing if the core could not start. */
	async mapLayersWords(): Promise<MapLayersWords | undefined> {
		if (!browser) return undefined;
		await this.start();
		return this.#core?.mapLayersWords();
	}

	/** What a list says when nothing in it matches `searched`; nothing if the core could not start. */
	async nothingFound(searched: string): Promise<NothingFoundWords | undefined> {
		await this.#started;
		return this.#core?.nothingFound(searched);
	}

	/** The core's line of who flies at a site now; empty when nobody does. */
	async siteAir(site: SiteAirAsked): Promise<string> {
		if (!browser) return '';
		await this.start();
		return (await this.#core?.siteAir(site)) ?? '';
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
	 * Uploads the flight files the pilot picked or dropped to the account.
	 * Files dropped while the app is still starting wait for it.
	 */
	async importFiles(files: readonly File[]): Promise<void> {
		await this.#started;
		if (!this.myFlights || files.length === 0) return;
		if (!this.account?.signedIn) {
			this.notice = {
				title: 'Sign in to import flights',
				lines: ['Your flights are kept on your Venti account. Sign in, then import them.']
			};
			return;
		}
		const report = await this.myFlights.upload(files);
		this.notice = importNotice(report);
		const id = flightToShow(report);
		if (id) await goto(myFlightHref(id));
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

	/** Deletes one of the pilot's flights from the account, at once and for good. */
	async deleteFlight(id: string): Promise<void> {
		await this.myFlights?.delete(id);
	}

	/**
	 * Keeps the pilot's own sites and the count of what is new in step with
	 * the account while the page is open: now, and whenever the page comes
	 * back to the front. Returns what to call
	 * when the page goes away.
	 */
	keepInStep(): () => void {
		const sync = () => {
			if (document.visibilityState !== 'visible') return;
			void this.account?.sync();
			if (this.account?.signedIn) void this.unread?.read();
		};
		void this.#started?.then(sync);
		document.addEventListener('visibilitychange', sync);
		// What the alarms fired, while a page is open; the account's sync
		// fetches it too, but that pauses a minute between runs.
		const alerts = setInterval(() => {
			if (this.account?.signedIn) void this.alerts?.sync();
		}, ALERTS_EVERY_MS);
		// What is new for the bubble on the pilot's picture, while a page is in front.
		const unread = setInterval(() => {
			if (this.account?.signedIn && document.visibilityState === 'visible') {
				void this.unread?.read();
			}
		}, UNREAD_EVERY_MS);
		return () => {
			document.removeEventListener('visibilitychange', sync);
			clearInterval(alerts);
			clearInterval(unread);
		};
	}
}

export const [getApp, setApp] = createContext<App>();
