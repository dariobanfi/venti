<!--
	The map tab: terrain, live wind from the stations of this site's Venti
	server, the flying sites it knows, the pilots who show where they fly,
	the pilot's airspace, the cables and the pilot's own flights.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { page } from '$app/state';
	import { creditLine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { CablesModel } from '#lib/cables/model.svelte.ts';
	import { accountHref, mapAroundIn, placeHref } from '#lib/app/routes.ts';
	import type {
		Bounds,
		CandidateOffer,
		MapLayersWords,
		Coordinate,
		Credit,
		SiteCandidateView,
		SiteOutlook,
		SiteView,
		SiteWritten,
		WindStation
	} from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import PromptDialog from '#lib/design/PromptDialog.svelte';
	import {
		type Reported,
		reportedSite,
		reportedSiteRevision,
		reportedTrack
	} from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import AirspaceOver from '#lib/airspace/AirspaceOver.svelte';
	import ForecastView from '#lib/forecast/ForecastView.svelte';
	import type { OpenOn } from '#lib/forecast/table.ts';
	import {
		creditLine as forecastCreditLine,
		hasSquaresIn,
		isDrawn,
		sameAsked
	} from '#lib/forecast/thermal-map.ts';
	import ThermalMapCard from '#lib/forecast/ThermalMapCard.svelte';
	import ThermalSquare from '#lib/forecast/ThermalSquare.svelte';
	import ThermalHourSquare from '#lib/forecast/ThermalHourSquare.svelte';
	import GliderView from '#lib/live/GliderView.svelte';
	import LiveList from '#lib/live/LiveList.svelte';
	import { countIn as glidersIn } from '#lib/live/mark.ts';
	import OtherView from '#lib/live/OtherView.svelte';
	import { otherById } from '#lib/live/others.ts';
	import { NO_LIVE } from '#lib/live/watch.ts';
	import { ClimbsModel, FlownModel } from '#lib/air/model.svelte.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import MapLayersSheet, { type LayerRow, type ToolRow } from '#lib/map/MapLayersSheet.svelte';
	import MapPanel from '#lib/map/MapPanel.svelte';
	import OverviewMap from '#lib/map/OverviewMap.svelte';
	import {
		aerialOn,
		airLayerOn,
		forecastOn,
		keepAerial,
		keepAirLayer,
		keepForecast,
		keepLayersOff,
		keepRadar,
		keepSlopes,
		type MapLayer,
		layersOff,
		radarOn,
		slopesOn
	} from '#lib/map/layers.ts';
	import { problemText } from '#lib/map/position.ts';
	import RadarCard from '#lib/radar/RadarCard.svelte';
	import { SlopesModel } from '#lib/slopes/model.svelte.ts';
	import SlopesKey from '#lib/slopes/SlopesKey.svelte';
	import { RoutePlanner } from '#lib/route/planner.svelte.ts';
	import RoutePlanCard from '#lib/route/RoutePlanCard.svelte';
	import RouteWeather from '#lib/route/RouteWeather.svelte';
	import { RouteWeatherModel } from '#lib/route/weather.svelte.ts';
	import GlideReachCard from '#lib/reach/GlideReachCard.svelte';
	import { GlideReachModel } from '#lib/reach/reach.svelte.ts';
	import { said as whyNot, type SiteDraft } from '#lib/sites/edit.svelte.ts';
	import { measuredNow } from '#lib/sites/filter.ts';
	import { countIn, flownFrom } from '#lib/sites/mark.ts';
	import SiteEditForm from '#lib/sites/SiteEditForm.svelte';
	import SiteHistoryView from '#lib/sites/SiteHistoryView.svelte';
	import SiteList from '#lib/sites/SiteList.svelte';
	import SitesFilterCard from '#lib/sites/SitesFilterCard.svelte';
	import SitesRemovedView from '#lib/sites/SitesRemovedView.svelte';
	import SitesFilterLine from '#lib/sites/SitesFilterLine.svelte';
	import SitePage from '#lib/sites/SiteView.svelte';
	import CandidateView from '#lib/sites/CandidateView.svelte';
	import { candidateKey } from '#lib/sites/mark.ts';
	import { offerFor } from '#lib/sites/takeoffs.svelte.ts';
	import { NO_SITES } from '#lib/sites/watch.ts';
	import { creditLine as sitesCreditLine } from '#lib/sites/words.ts';
	import StationList from '#lib/wind/StationList.svelte';
	import StationView from '#lib/wind/StationView.svelte';
	import { notice, NO_WIND } from '#lib/wind/watch.ts';

	const app = getApp();
	const text = format();
	const wide = new MediaQuery('min-width: 56rem', false);

	let following = $state(false);
	let map = $state<OverviewMap>();
	/** Where the map opens, where its address names a place: a site's page sent the pilot here. */
	const around = browser ? untrack(() => mapAroundIn(page.url.searchParams)) : undefined;
	/** How far the navigation reaches in from the bottom edge, measured in pixels. */
	let navigationHeight = $state(0);
	/** How high the map's foot is (the credits of what Venti draws), in pixels. */
	let footHeight = $state(0);

	/** The layers the pilot switched off with the chips, as this browser remembers them. */
	// A layer the address names (`?show=pilots`, from the home page's "In
	// the air now") is on whatever this browser remembers.
	const shownByAddress = browser ? untrack(() => page.url.searchParams.get('show')) : null;
	let off = $state<MapLayer[]>(
		layersOff(globalThis.localStorage).filter((layer) => layer !== shownByAddress)
	);
	function toggle(layer: MapLayer) {
		off = off.includes(layer) ? off.filter((one) => one !== layer) : [...off, layer];
		keepLayersOff(globalThis.localStorage, off);
	}
	/** Whether the ground is photographs from above: the pilot's choice for every map. */
	let aerial = $state(aerialOn(globalThis.localStorage));
	/** Whose photographs can show where the map has come to rest, as the map says. */
	let photosShowing = $state('');
	/**
	 * Whose the photographs are, while they show on a map whose own credits
	 * are folded behind their button. A wide map writes them out itself.
	 */
	const photosCredit = $derived(aerial && !wide.current ? photosShowing : '');
	/** Whether the map shows the kind of day, square by square: off until the pilot asks. */
	let forecast = $state(forecastOn(globalThis.localStorage));
	const thermals = $derived(app.thermals);
	const showForecast = $derived(forecast && Boolean(app.account?.hasServer) && Boolean(thermals));
	// Asked for when the layer is switched on, and again when the map is half an hour old.
	$effect(() => {
		if (showForecast) return thermals?.keepCurrent();
	});
	// The hours of the layer, the day and the height chosen, and what the
	// map draws of the hour shown.
	$effect(() => {
		if (showForecast) void thermals?.loadHours();
	});
	$effect(() => {
		if (showForecast) void thermals?.loadShapes();
	});
	/** One hour of the layer chosen, drawn in place of the day. */
	const thermalHour = $derived.by(() => {
		const view = showForecast ? thermals?.view : undefined;
		const shapes = thermals?.shapes;
		if (!view || thermals?.hour === undefined || !shapes) return undefined;
		if (!sameAsked(shapes.asked, thermals.hours?.asked)) return undefined;
		const choice = thermals.map?.layers.find((one) => one.layer === view.layer);
		return { squares: shapes.squares, arrows: shapes.arrows, legend: choice?.legend ?? [] };
	});
	/**
	 * The day of the thermal map on the map, while the layer is on: not while
	 * the hours of another layer are asked for, so that its squares do not
	 * stand under another key, and not while an hour is drawn.
	 */
	const thermalDay = $derived.by(() => {
		if (!showForecast || thermalHour) return undefined;
		if (thermals?.asked && thermals.hoursAsking && !thermals.view) return undefined;
		if (thermals?.view && thermals.hour !== undefined) return undefined;
		return thermals?.day;
	});
	/** A square of an hour the pilot chose: the place clicked and its square's number. */
	let chosenHourSquare = $state.raw<{ place: Coordinate; cell: number }>();
	// The day as a whole has squares of its own; while another day's or
	// layer's hours are asked for, the square stays open.
	$effect(() => {
		if (!showForecast || (thermals?.view && thermals.hour === undefined)) {
			chosenHourSquare = undefined;
		}
	});
	/**
	 * The day whose square can be open: the day on the map, and while the
	 * hours of another day are asked for with the day as a whole shown, that
	 * day, so that a square stays open when the pilot goes through its days.
	 */
	const squareDay = $derived.by(() => {
		if (thermalDay) return thermalDay;
		if (!showForecast || thermals?.asked?.layer !== 'thermals') return undefined;
		const awaited = thermals.hoursAsking && !thermals.view;
		return awaited && typeof thermals.chosenHour !== 'number' ? thermals.day : undefined;
	});
	/** The square the pilot chose, by its corner: the same square on another day. */
	let chosenSquare = $state.raw<{ south: number; west: number }>();
	const square = $derived(
		squareDay?.cells.find(
			(cell) =>
				isDrawn(cell) && cell.south === chosenSquare?.south && cell.west === chosenSquare?.west
		)
	);
	/** Whose forecast the squares are. */
	const forecastCredit = $derived(showForecast ? forecastCreditLine(thermals?.map, text) : '');

	/** Whether the map shows the rain radar: off until the pilot asks. */
	let radarChosen = $state(radarOn(globalThis.localStorage));
	const radar = $derived(app.radar);
	const showRadar = $derived(radarChosen && Boolean(app.account?.hasServer) && Boolean(radar));
	// Asked for when the layer is switched on, again every five minutes
	// while the map is on screen, and when the tab comes back to the front.
	$effect(() => {
		if (showRadar) return radar?.keepCurrent();
	});
	// The pictures of the moment the strip marks.
	$effect(() => {
		if (showRadar) void radar?.showMoment();
	});
	/**
	 * Whose rain is on the map: the credit of each of the radar's layers
	 * drawn, as its provider words it, at the data.
	 */
	const radarCredits = $derived(
		showRadar ? (radar?.layersDrawn ?? []).flatMap((layer) => layer.credit?.text || []) : []
	);

	/**
	 * Whether the one sheet of the map's layers and tools is open: a sheet
	 * at a phone's width, a card at the map's side on a wide screen.
	 */
	let layersOpen = $state(false);
	/**
	 * Whether the credits are unfolded: closed from the start behind the
	 * map's "i", which unfolds the map's own and Venti's together.
	 */
	let creditsOpen = $state(false);

	const wind = $derived(app.wind);
	const current = $derived(wind?.state ?? NO_WIND);
	const sites = $derived(app.sites);
	const places = $derived(sites?.state ?? NO_SITES);
	const live = $derived(app.live);
	const inTheAir = $derived(live?.state ?? NO_LIVE);

	/** The part of the map in view, and its zoom. The map may be there before the core is. */
	let view = $state.raw<{ region: Bounds; zoom: number }>();
	$effect(() => {
		if (view) wind?.show(view.region);
	});
	// The flying sites of what the pilot is looking at, once that is a
	// valley and not a country.
	$effect(() => {
		if (view) sites?.show(view.region, view.zoom);
	});
	/** The sites the pilot's filter leaves, and the take-offs that stand apart from them. */
	const sitesFitting = $derived(sites?.shown.fitting ?? places.sites);
	const sitesApart = $derived(sites?.shown.unsaid ?? NO_SITES.sites);
	/** What the filter is and what it leaves, in the core's words. */
	const sitesLeft = $derived(sites?.left);
	/**
	 * Whether a filter is on and the map is close enough to draw sites: then
	 * the map says what the filter is and what it leaves. Further out, and
	 * with the "Sites" layer off, there is nothing it leaves anything of.
	 */
	const sitesFiltered = $derived(
		sitesLeft && sitesLeft.isOn && places.close && !off.includes('sites') ? sitesLeft : undefined
	);
	/** Whether the filter's choices are open: a pane at the map's side, or a sheet. */
	let filtering = $state(false);
	// "Measured now" is the wind of the station nearest to the map's middle.
	$effect(() => {
		if (!sites) return;
		const middle = view && {
			lat: (view.region.south + view.region.north) / 2,
			lon: (view.region.west + view.region.east) / 2
		};
		const measuredDeg = measuredNow(current.stations, middle);
		if (measuredDeg !== sites.windNow.measuredDeg) sites.windNow = { measuredDeg };
	});
	// The core is asked what the filter leaves whenever the sites or the filter change.
	$effect(() => {
		if (!sites) return;
		void [sites.state.sites, sites.filter];
		void sites.refilter();
	});
	// The take-offs without a site of what the pilot is looking at, while their layer is on.
	const takeoffs = $derived(app.takeoffs);
	$effect(() => {
		if (view && takeoffs?.layerOn) takeoffs.show(view.region);
	});
	/** The candidate whose sheet is open, by its place (`candidateKey`). */
	let chosenCandidate = $state.raw<string>();
	/**
	 * The title of a candidate's sheet: its height, and the core's title
	 * after it only where the core's line does not begin with it already
	 * ("Pilots take off here: 4 pilots, …").
	 */
	function candidateHeading(one: SiteCandidateView, title: string): string {
		return one.words.startsWith(title) ? one.altitudeWords : `${one.altitudeWords} · ${title}`;
	}
	const candidate = $derived(
		chosenCandidate === undefined
			? undefined
			: takeoffs?.candidates.find((one) => candidateKey(one) === chosenCandidate)
	);
	/** What the core says over the map about the layer: zoom in, or more than one answer lists. */
	const candidatesSaid = $derived(
		takeoffs?.layerOn && !off.includes('sites') ? takeoffs.over || takeoffs.more : ''
	);

	// Who is in the air over what the pilot is looking at.
	$effect(() => {
		if (view) live?.show(view.region);
	});
	// Asked again every quarter of a minute while the map is on screen.
	$effect(() => live?.keepCurrent());
	// The airspace around what the pilot is looking at is fetched from the
	// server when the core says it is time, and kept.
	$effect(() => {
		if (!view) return;
		const { region } = view;
		void app.airspace?.refresh({
			lat: (region.south + region.north) / 2,
			lon: (region.west + region.east) / 2
		});
	});

	// --- Slopes ---

	/** Whether the map shows how steep the ground is: off until the pilot asks. */
	let slopesChosen = $state(slopesOn(globalThis.localStorage));
	/** The slopes of the part of the map in view, asked for once the map rests close enough. */
	const slopes = new SlopesModel(
		(asked) => app.slopes(asked),
		() => void app.forgetSlopes()
	);
	$effect(() => {
		if (view && slopesChosen) slopes.show(view.region, view.zoom);
	});
	/** Whether the slopes are on but the map too far out to draw them. */
	const slopesTooFar = $derived(slopesChosen && view !== undefined && !slopes.close);

	// --- Cables ---

	/** The cables around the map's middle, asked for once the map rests close enough. */
	const cables = new CablesModel((place) => app.cables(place));
	$effect(() => {
		if (view && !off.includes('cables') && app.account?.hasServer) {
			cables.show(view.region, view.zoom);
		}
	});
	/** The cables the map draws: none while their layer is off or the map is zoomed out. */
	const cablesShown = $derived(off.includes('cables') ? [] : cables.lines);
	/** The cable the pilot clicked, by its identifier: its sheet says what it is. */
	let chosenCable = $state<string>();
	const cable = $derived(chosenCable === undefined ? undefined : cables.line(chosenCable));
	/** Whose a cable is: the credit of its source, by the start of its identifier. */
	const cableCredit = $derived(
		cable && cables.answer?.credits.find((credit) => cable.id.startsWith(`${credit.source}:`))
	);
	/** Whose the cables on the map are, in their providers' wording, while some are drawn. */
	const cablesCredits = $derived(
		cablesShown.length > 0 ? (cables.answer?.credits ?? []).map((credit) => credit.text) : []
	);

	function openCable(id: string) {
		closeOthers();
		chosenCable = id;
	}

	// --- What many gliders say together: climbs now, and where thermals have been ---

	/** Whether the map shows the climbs of the last minutes: off until the pilot asks. */
	let climbsChosen = $state(airLayerOn(globalThis.localStorage, 'climbs'));
	/** Whether the map shows where thermals have been: off until the pilot asks. */
	let flownChosen = $state(airLayerOn(globalThis.localStorage, 'flown'));
	const climbs = new ClimbsModel((box) => app.liveClimbs(box));
	const flown = new FlownModel((box) => app.flownThermals(box));
	/**
	 * What the layers' sheet says, in the core's words: its title, its two
	 * headings, the two layers' names and lines, and what is said where one
	 * of them has nothing to show.
	 */
	let layerWords = $state.raw<MapLayersWords>();
	$effect(() => {
		void app.mapLayersWords().then(
			(words) => (layerWords = words),
			() => {}
		);
	});
	const showClimbs = $derived(climbsChosen && Boolean(app.account?.hasServer));
	const showFlown = $derived(flownChosen && Boolean(app.account?.hasServer));
	// The climbs of the part in view, and again every half minute while on.
	$effect(() => {
		if (view && showClimbs) climbs.show(view.region);
	});
	$effect(() => {
		if (showClimbs) return climbs.keepCurrent();
	});
	// Where thermals have been in the part in view, in every month of the year.
	$effect(() => {
		if (view && showFlown) flown.show(view.region);
	});
	/** Whose the climbs are, in the core's credits, while their layer is on. */
	const climbsCredits = $derived(showClimbs ? (climbs.answer?.credits ?? []) : []);
	/** What the core says where the climbs are on and there are none. */
	const climbsSaid = $derived(
		showClimbs
			? climbs.problem || (climbs.answer?.climbs.length === 0 ? climbs.answer.emptyWords : '')
			: ''
	);
	/**
	 * What the core says where the map shows too much to ask for the
	 * thermals, or where no cell lies in the part shown.
	 */
	const flownSaid = $derived(
		showFlown
			? flown.problem ||
					flown.answer?.tooWide ||
					(flown.answer?.thermals?.cells.length === 0 ? (layerWords?.flownEmpty ?? '') : '')
			: ''
	);

	/** A climb or a cell clicked: the core's line about it, in the card of its place. */
	function openAir(at: { position: Coordinate; line: string } | undefined) {
		if (at) openPlace(at.position, false, at.line);
	}

	/** How many airspaces the map draws, or nothing while it is zoomed too far out for any. */
	let airspaceDrawn = $state<number>();
	/** Whose airspace is on the map. Their licences ask for this wherever it is shown. */
	const airspaceCredit = $derived(
		airspaceDrawn === undefined ? '' : creditLine(app.airspace?.credits ?? [], text)
	);
	// Asks again every minute while the map is on screen.
	$effect(() => wind?.keepCurrent());

	/** The station the pilot chose, as it was then. */
	let chosen = $state.raw<WindStation>();
	/** The same station with its newest reading, for as long as the map still has it. */
	const station = $derived(chosen && (wind?.station(chosen.id) ?? chosen));
	let listing = $state(false);

	/**
	 * The flying site the pilot chose, with whose data it is and of when:
	 * all three as they were then, because the map lets go of its sites when
	 * it is zoomed out.
	 */
	let chosenSite = $state.raw<{
		site: SiteView;
		credit: Credit | undefined;
		asOfUnixS: number | undefined;
	}>();
	let listingSites = $state(false);

	/** What the forecast says about flying from the chosen site, for a place to take off from. */
	let siteOutlook = $state.raw<SiteOutlook>();
	$effect(() => {
		const site = chosenSite?.site;
		siteOutlook = undefined;
		if (!site || !site.uses.includes('takeoff')) return;
		let current = true;
		app.siteOutlook(site).then(
			(found) => {
				if (current) siteOutlook = found;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/**
	 * The place whose forecast is looked at, with what it is called: a
	 * site's or a station's page leads there, and a square of the thermal
	 * map, which also names the day the map was showing.
	 */
	let forecastFor = $state.raw<{ place: Coordinate; name: string; openOn?: OpenOn }>();

	/**
	 * The glider the pilot chose, by the identifier of its track. Only that
	 * is kept: its page shows the glider as the newest answer has it.
	 */
	let chosenGlider = $state<string>();
	let listingGliders = $state(false);
	/** The live track being reported to whoever runs the server, by its identifier. */
	let reportedGlider = $state<string>();
	const glider = $derived(chosenGlider === undefined ? undefined : live?.glider(chosenGlider));
	/**
	 * The aircraft of another network the pilot chose, by its identifier:
	 * its sheet shows it as the newest answer has it, and closes when the
	 * network no longer hears it here.
	 */
	let chosenOther = $state<string>();
	const other = $derived(
		chosenOther === undefined ? undefined : otherById(inTheAir.others, chosenOther)
	);
	/** The others the map draws: none while the "Pilots" layer is off. */
	const othersShown = $derived(off.includes('pilots') ? NO_LIVE.others : inTheAir.others);
	/** Whose the others are, in the core's credits, while the "Pilots" layer is on. */
	const othersCredits = $derived(
		off.includes('pilots') ? NO_LIVE.othersCredits : inTheAir.othersCredits
	);
	const NO_TRACK: readonly never[] = [];
	/** The flight of the chosen glider so far, for the map to draw as a line. */
	const followedTrack = $derived(live?.followed.view?.track ?? NO_TRACK);
	/** A glider's page as the stylesheet sizes it, measured in pixels for the map. */
	let gliderPage = $state({ width: 0, height: 0 });
	/** How far the map's buttons reach in from the right edge, in pixels. */
	let buttonsWidth = $state(0);
	/**
	 * What lies over the map while a glider's page is open: the card and
	 * the buttons at the sides of a wide screen, the sheet over the lower
	 * half of a narrow one, which covers the buttons too.
	 */
	const coveredByPage = $derived(
		wide.current
			? { left: gliderPage.width, right: buttonsWidth, bottom: 0 }
			: { left: 0, right: 0, bottom: gliderPage.height }
	);

	/**
	 * A place clicked where nothing else of the map's lies under the
	 * pointer: what airspace lies over it, where some does, and what can be
	 * done from there (what a glide reaches, adding a site).
	 */
	let chosenPlace = $state.raw<{
		place: Coordinate;
		name: string;
		airspace: boolean;
		/** The core's line about what was clicked there: a climb, or a cell of flown thermals. */
		said?: string;
	}>();
	/** Counts the clicks on places, so that a slow answer for an older one is dropped. */
	let placeAsked = 0;

	/**
	 * Opens a place's card: with what airspace lies over it where the map
	 * draws airspace (`withAirspace`) and some does.
	 */
	function openPlace(place: Coordinate, withAirspace = true, said?: string) {
		chosenCandidate = undefined;
		layersOpen = false;
		const mine = ++placeAsked;
		const open = (airspace: boolean) => {
			if (mine !== placeAsked) return;
			chosenSquare = undefined;
			chosen = undefined;
			chosenSite = undefined;
			chosenGlider = undefined;
			chosenOther = undefined;
			chosenPlace = { place, name: text.place(place.lat, place.lon), airspace, said };
		};
		const model = app.airspace;
		if (!withAirspace || !model || model.isEmpty) {
			open(false);
			return;
		}
		model.over(place).then(
			(over) => open(over.airspaces.length > 0),
			() => open(false)
		);
	}

	/** One thing is open beside the map at a time: a station, a site, a glider, a square or airspace. */
	function openStation(picked: WindStation | undefined) {
		chosenCandidate = undefined;
		layersOpen = false;
		if (!picked) return;
		chosenPlace = undefined;
		chosenSquare = undefined;
		chosenSite = undefined;
		chosenGlider = undefined;
		chosenOther = undefined;
		chosen = picked;
	}

	function openGlider(id: string) {
		chosenCandidate = undefined;
		layersOpen = false;
		chosenPlace = undefined;
		chosenSquare = undefined;
		chosen = undefined;
		chosenSite = undefined;
		chosenOther = undefined;
		chosenGlider = id;
	}

	function openOther(id: string) {
		closeOthers();
		chosenOther = id;
	}

	function openSite(picked: SiteView | undefined) {
		chosenCandidate = undefined;
		layersOpen = false;
		if (!picked) return;
		chosenPlace = undefined;
		chosenSquare = undefined;
		chosen = undefined;
		chosenGlider = undefined;
		chosenOther = undefined;
		siteWritten = undefined;
		chosenSite = {
			site: picked,
			credit: places.credits.find((credit) => credit.source === picked.source),
			asOfUnixS: places.asOfUnixS
		};
	}

	/** The square of an hour under a place; airspace there where the map has no square. */
	function openHourSquare(place: Coordinate) {
		void thermals?.cellAt(place).then((cell) => {
			if (cell === undefined) {
				openPlace(place);
				return;
			}
			chosenPlace = undefined;
			chosen = undefined;
			chosenSite = undefined;
			chosenGlider = undefined;
			chosenOther = undefined;
			chosenSquare = undefined;
			chosenHourSquare = { place, cell };
		});
	}

	function openSquare(cell: number) {
		const picked = thermalDay?.cells[cell];
		if (!picked) return;
		chosenPlace = undefined;
		chosen = undefined;
		chosenSite = undefined;
		chosenGlider = undefined;
		chosenOther = undefined;
		chosenSquare = { south: picked.south, west: picked.west };
	}

	/**
	 * The forecast for the middle of a square of the thermal map, open on
	 * the day the map shows, of the model the map read that day from.
	 */
	function squareForecast(
		cell: { south: number; west: number; north: number; east: number },
		day: OpenOn
	) {
		const place = { lat: (cell.south + cell.north) / 2, lon: (cell.west + cell.east) / 2 };
		forecastFor = {
			place,
			name: text.place(place.lat, place.lon),
			openOn: { date: day.date, model: day.model }
		};
	}

	// --- Sites that pilots make: adding one, correcting, removing, their history ---

	const edits = $derived(app.siteEdits);
	/** The form in the core's words, and the words of a site's page. */
	const siteForm = $derived(edits?.form);
	const siteWords = $derived(edits?.words);
	/**
	 * The entry for adding a site, where the core has words for one
	 * (`Core.siteAdding`, for the account this browser is signed in to):
	 * into which tier, and what the entry and its form say. Where the core
	 * says it leads to signing in, the entry is its sentence about that and
	 * opens the account. The page decides nothing of it.
	 */
	const adding = $derived(edits?.adding?.entryWords ? edits.adding : undefined);
	// What the core offers follows the account: asked again when the pilot
	// signs in or out, and when the server has said what the account is.
	$effect(() => {
		void app.account?.account;
		void edits?.accountChanged();
	});
	/** The site being added or corrected, while its form is open or its place is being moved. */
	let editing = $state.raw<SiteDraft>();
	/**
	 * Whether the pilot is moving the map under the mark: to say where a
	 * new site is, or to move the one in the form, which waits meanwhile.
	 */
	let placing = $state(false);
	/** Why a place could not be made into a draft, or a site not opened in its form. */
	let placingProblem = $state.raw<string>();
	/** What the server answered to the pilot's last change of the chosen site, in the core's line. */
	let siteWritten = $state.raw<{ id: string; words: string }>();
	/**
	 * The site whose history is open, by its identifier, with the core's
	 * line about its removal. What may be done in it the core says of the
	 * history itself.
	 */
	let historyOf = $state.raw<{ id: string; said?: string }>();
	/** Whether "Removed lately" is open: the removed sites of Venti's tiers. */
	let removedLately = $state(false);
	/** The site the pilot is about to remove, while the line about it is asked for. */
	let removing = $state.raw<SiteView>();
	let removingNow = $state(false);
	let removeRefusal = $state.raw<string>();
	/** A site or one of its revisions being reported to whoever runs the server. */
	let reportedOfSites = $state.raw<Reported>();
	/** The "Why" line of the core's form: its title and its help, for the dialogs that ask it. */
	const why = $derived(siteForm?.fields.find((field) => field.field === 'comment'));
	/**
	 * What the form says, as the core says it: for a new site
	 * `Core.siteAdding`, of the tier this pilot adds to; for a correction
	 * `siteCorrecting`, of the tier the site is of.
	 */
	const formSays = $derived.by(() => {
		if (!editing) return undefined;
		const says = editing.corrects ? editing.correcting : adding?.mayAdd ? adding : undefined;
		return (
			says && { title: says.title, note: says.note, giving: says.giving, send: says.sendWords }
		);
	});

	function closeOthers() {
		layersOpen = false;
		chosenCable = undefined;
		chosenCandidate = undefined;
		chosenPlace = undefined;
		chosenSquare = undefined;
		chosenHourSquare = undefined;
		chosen = undefined;
		chosenSite = undefined;
		chosenGlider = undefined;
		chosenOther = undefined;
		filtering = false;
	}

	/** "Add a site": the pilot moves the map under the mark to say where. */
	function startAdding() {
		if (!adding?.mayAdd) return;
		closeOthers();
		planner.close();
		editing = undefined;
		placingProblem = undefined;
		placing = true;
	}

	/** "Add a site here": the form for a new site at a place clicked on the map. */
	async function addSiteAt(place: Coordinate) {
		if (!edits || !adding?.mayAdd) return;
		closeOthers();
		try {
			editing = await edits.draftAt(place);
		} catch (error) {
			app.notice = { title: adding.title, lines: [whyNot(error)] };
		}
	}

	/**
	 * "Add this place": the form of a new site on the core's draft, at the
	 * pilot's own take-off.
	 */
	function addOffered(offer: CandidateOffer) {
		if (!edits || !adding?.mayAdd) return;
		closeOthers();
		placing = false;
		editing = edits.draftFrom(offer.draft);
	}

	/** "Not this one": set aside in this browser, and the sheet closes. */
	async function dismissOffered(offer: CandidateOffer) {
		chosenCandidate = undefined;
		await takeoffs?.setAside(offer).catch(() => {});
	}

	// An offer taken from Today: its form opens here, where the map is.
	$effect(() => {
		const waiting = edits?.waiting;
		if (!waiting || !edits || !adding?.mayAdd) return;
		edits.waiting = undefined;
		closeOthers();
		editing = edits.draftFrom(waiting);
	});

	/** "Use this place": the middle of the map is where the site is. */
	async function usePlace() {
		const place = map?.middle();
		if (!edits || !place) return;
		placingProblem = undefined;
		try {
			if (editing) await editing.move(place);
			else editing = await edits.draftAt(place);
			placing = false;
		} catch (error) {
			placingProblem = whyNot(error);
		}
	}

	/** "Move": the form gives way to the map, with the site's place under the mark. */
	function movePlace() {
		if (!editing) return;
		map?.showPlace(editing.draft.position);
		placingProblem = undefined;
		placing = true;
	}

	/** "Correct this site": the form on the site as it stands. */
	async function correctSite(site: SiteView) {
		if (!edits) return;
		try {
			const draft = await edits.draftOf($state.snapshot(site) as SiteView);
			closeOthers();
			editing = draft;
		} catch (error) {
			app.notice = { title: siteForm?.titleCorrect ?? '', lines: [whyNot(error)] };
		}
	}

	/** Shows a site of Venti's layer as the server has it now, with whose data it is. */
	function showWritten(site: SiteView, words?: string) {
		sites?.put(site);
		siteWritten = words ? { id: site.id, words } : undefined;
		chosenPlace = undefined;
		chosenSquare = undefined;
		chosen = undefined;
		chosenGlider = undefined;
		chosenOther = undefined;
		chosenSite = {
			site,
			credit: places.credits.find((credit) => credit.source === site.source),
			asOfUnixS: undefined
		};
		// The credit and the notice of the layer come with the site's own page.
		void edits?.page(site.id).then(
			(page) => {
				if (chosenSite?.site.id !== site.id) return;
				chosenSite = { site: page.site, credit: page.credit, asOfUnixS: undefined };
			},
			() => {}
		);
	}

	/**
	 * "Show how it stands now": the site as it is after someone else
	 * changed it, beside the form, which keeps what the pilot entered.
	 */
	function showBesideForm(site: SiteView) {
		sites?.put(site);
		siteWritten = undefined;
		chosenSite = {
			site,
			credit: places.credits.find((credit) => credit.source === site.source),
			asOfUnixS: undefined
		};
	}

	/** A confirmation was said or taken back: the site as the server has it now, with the core's line. */
	function siteConfirmed(written: SiteWritten) {
		sites?.put(written.site);
		siteWritten = { id: written.site.id, words: written.words };
		if (chosenSite?.site.id === written.site.id) chosenSite = { ...chosenSite, site: written.site };
	}

	/** The form sent its site: the core's line, and the site's page. */
	function siteSent(written: SiteWritten) {
		editing = undefined;
		showWritten(written.site, written.words);
	}

	/** Opens a site by its identifier alone: the one that stands in the way of a new one. */
	async function openSiteById(id: string) {
		if (!edits) return;
		try {
			const page = await edits.page(id);
			editing = undefined;
			showWritten(page.site);
		} catch (error) {
			app.notice = { title: 'Flying site', lines: [whyNot(error)] };
		}
	}

	async function removeSite(comment: string) {
		if (!edits || !removing || removingNow) return;
		const { id } = removing;
		removingNow = true;
		removeRefusal = undefined;
		try {
			const words = await edits.remove(id, comment);
			removing = undefined;
			sites?.drop(id);
			if (chosenSite?.site.id === id) chosenSite = undefined;
			// Its history stays, and is where it can be brought back.
			historyOf = { id, said: words };
		} catch (error) {
			removeRefusal = whyNot(error);
		} finally {
			removingNow = false;
		}
	}

	/** A revision was put back from the history: the site stands on the map as it is now. */
	function putBack(written: SiteWritten) {
		sites?.put(written.site);
		if (historyOf) historyOf = { id: historyOf.id };
		if (chosenSite?.site.id === written.site.id) showWritten(written.site, written.words);
	}

	/**
	 * "Removed lately" is offered to whoever is signed in: the core says
	 * line by line who may bring which site back.
	 */
	const openRemoved = $derived(
		edits && app.account?.signedIn ? () => (removedLately = true) : undefined
	);

	/** A removed site was brought back from "Removed lately": its page, with the core's line. */
	function broughtBack(written: SiteWritten) {
		removedLately = false;
		filtering = false;
		listingSites = false;
		showWritten(written.site, written.words);
	}

	// --- The route planner ---

	/** The route the pilot draws on the map, kept in the browser until it is cleared. */
	const planner = new RoutePlanner(globalThis.localStorage, (places) => app.planRoute(places));
	/** The weather along it: the start, the speed and the glide, and the core's answer. */
	const routeWeather = new RouteWeatherModel(globalThis.localStorage, (asked) =>
		app.routeWeather(asked)
	);
	/** The days the route can be flown on with a forecast: the thermal map's. */
	const routeDays = $derived(app.account?.hasServer ? (thermals?.map?.days ?? []) : []);
	// Asked again whenever the route, the day or the setting changes.
	$effect(() => {
		if (planner.open) void routeWeather.show(planner.places, routeDays);
	});
	// The thermal map's days, asked for once while the planner is open.
	$effect(() => {
		if (planner.open && app.account?.hasServer && thermals && !thermals.map && !thermals.asking) {
			if (!untrack(() => thermals.problem)) void thermals.load();
		}
	});
	/** What the map draws of it and takes clicks for, while the planner is open. */
	const route = $derived(
		planner.open && !placing
			? {
					places: planner.places,
					plan: planner.plan,
					departures: routeWeather.along?.points.map((point) => point.departureM)
				}
			: undefined
	);

	/** "Plan a route": the planner opens on the route kept, or closes. */
	function togglePlanner() {
		if (!planner.open) {
			closeOthers();
			reach.clear();
			placing = false;
		}
		planner.toggle();
	}

	/** A click while planning: the next place, with the site's name where it was a site's sign. */
	function planAt(at: Coordinate, siteId?: string) {
		const name = siteId ? sites?.site(siteId)?.name : undefined;
		planner.add(name ? { ...at, name } : at);
	}

	/** "Plan route" on a site's page: a new route that begins at the site. */
	function planFrom(site: SiteView) {
		planner.startAt({ lat: site.position.lat, lon: site.position.lon, name: site.name });
		closeOthers();
		map?.showAround(site.position);
	}

	// --- What a glide reaches ---

	/** How much of a narrow screen the card of a glide's reach may cover: `--forecast-card-height`. */
	const REACH_CARD_SHARE = 0.4;

	/** What a glide reaches from a place, while its area is on the map. */
	const reach = new GlideReachModel(
		globalThis.localStorage,
		(setting) => app.glideReach(setting),
		() => void app.forgetGlideReach()
	);

	/** "What a glide reaches from here": the area from the place, and its card. */
	function reachFrom(place: Coordinate) {
		closeOthers();
		planner.close();
		void reach.open(place);
		// On a narrow screen its card stands over the map's foot, at most two
		// fifths of the screen: the place goes above it, with the area around.
		if (!wide.current) map?.showAbove(place, globalThis.innerHeight * REACH_CARD_SHARE);
	}

	const said = $derived(notice(current, text.list));
	const credits = $derived(text.list(current.credits.map((credit) => credit.provider)));
	/** Whose sites are on the map. Their licence asks for this wherever they are shown. */
	const sitesCredit = $derived(
		places.sites.length === 0 ? '' : sitesCreditLine(places.credits, text)
	);

	// --- The chips, and the one sheet of layers and tools ---

	function setForecast(on: boolean) {
		forecast = on;
		keepForecast(globalThis.localStorage, on);
		if (!on) chosenSquare = undefined;
	}
	function setRadar(on: boolean) {
		radarChosen = on;
		keepRadar(globalThis.localStorage, on);
	}
	function setSlopes(on: boolean) {
		slopesChosen = on;
		keepSlopes(globalThis.localStorage, on);
		if (!on) slopes.off();
	}
	function setAerial(on: boolean) {
		aerial = on;
		keepAerial(globalThis.localStorage, on);
	}
	function setClimbs(on: boolean) {
		climbsChosen = on;
		keepAirLayer(globalThis.localStorage, 'climbs', on);
		if (!on) climbs.off();
	}
	function setFlown(on: boolean) {
		flownChosen = on;
		keepAirLayer(globalThis.localStorage, 'flown', on);
		if (!on) flown.off();
	}
	/** A layer the chips' list switches (`off`), as a row or a chip. */
	const listed = (layer: MapLayer) => ({
		on: !off.includes(layer),
		onchange: () => toggle(layer)
	});

	/** The layers' and the rows' names, the core's; empty until it answered. */
	const named = $derived<Partial<MapLayersWords>>(layerWords ?? {});

	/** The four layers a pilot switches most: the chips along the map's top. */
	const chips = $derived<LayerRow[]>([
		{
			key: 'forecast',
			name: named.forecast ?? '',
			icon: 'forecast',
			on: forecast,
			onchange: setForecast
		},
		{ key: 'radar', name: named.radar ?? '', icon: 'rain', on: radarChosen, onchange: setRadar },
		{ key: 'wind', name: named.wind ?? '', icon: 'wind', ...listed('wind') },
		{ key: 'sites', name: named.sites ?? '', icon: 'pin', ...listed('sites') }
	]);
	/**
	 * Every layer as a row with a switch: the chips' four in the same
	 * state, then the others. A site without a server has the ground alone.
	 */
	const layerRows = $derived<LayerRow[]>([
		...(app.account?.hasServer
			? [
					...chips,
					{ key: 'airspace', name: named.airspace ?? '', icon: 'airspace', ...listed('airspace') },
					{ key: 'pilots', name: named.pilots ?? '', icon: 'glider', ...listed('pilots') },
					{
						key: 'slopes',
						name: named.slopes ?? '',
						icon: 'mountain',
						on: slopesChosen,
						onchange: setSlopes
					},
					{ key: 'cables', name: named.cables ?? '', icon: 'warning', ...listed('cables') }
				]
			: []),
		{
			key: 'aerial',
			name: named.aerialView ?? '',
			icon: 'layers',
			on: aerial,
			onchange: setAerial
		},
		...(app.account?.hasServer && layerWords
			? [
					{
						key: 'climbs',
						name: layerWords.climbsNow,
						icon: 'thermal',
						on: climbsChosen,
						onchange: setClimbs,
						notes: [layerWords.climbsNowNote, climbs.answer?.countWords || climbsSaid]
					},
					{
						key: 'flown',
						name: layerWords.flownThermals,
						icon: 'thermal',
						on: flownChosen,
						onchange: setFlown,
						notes: [layerWords.flownThermalsNote, flownSaid, flown.answer?.thermals?.note ?? ''],
						terms: flown.answer?.thermals && {
							title: flown.answer.thermals.termsTitle ?? '',
							text: flown.answer.thermals.terms
						}
					}
				]
			: [])
	] as LayerRow[]);
	/** A tool's row closes the sheet and opens what its round button used to. */
	const opens = (open: () => void) => () => {
		layersOpen = false;
		open();
	};
	const toolRows = $derived<ToolRow[]>([
		...(current.available
			? ([
					{
						key: 'stations',
						name: named.windStations ?? '',
						icon: 'wind',
						onclick: opens(() => (listing = true))
					},
					{
						key: 'sites',
						name: named.flyingSites ?? '',
						icon: 'pin',
						onclick: opens(() => (listingSites = true))
					},
					{
						key: 'air',
						name: named.pilotsInTheAir ?? '',
						icon: 'glider',
						onclick: opens(() => (listingGliders = true))
					}
				] satisfies ToolRow[])
			: []),
		{ key: 'route', name: named.planRoute ?? '', icon: 'route', onclick: opens(togglePlanner) },
		// A site of the pilot's own knowledge, in the core's words for the
		// tier this account adds to; for someone who has to sign in first,
		// its sentence about that, which leads to the account.
		...(current.available && adding
			? ([
					adding.leadsToSignIn
						? { key: 'add', name: adding.entryWords, icon: 'takeoff', href: accountHref() }
						: { key: 'add', name: adding.entryWords, icon: 'takeoff', onclick: startAdding }
				] satisfies ToolRow[])
			: [])
	]);
</script>

<svelte:head>
	<title>Map · Venti</title>
</svelte:head>

<h1 class="visually-hidden">Map</h1>
<!-- The foot's height is set by the script once measured, never written
     into the built page, whose policy allows no style attribute. -->
<div
	class="screen"
	style:--foot-height={footHeight > 0 ? `${footHeight}px` : undefined}
	data-wind={current.available ? 'answered' : undefined}
>
	<OverviewMap
		bind:this={map}
		bind:following
		outlines={app.logbook?.outlines ?? []}
		stations={current.stations}
		sites={sitesFitting}
		{sitesApart}
		candidates={takeoffs?.layerOn ? takeoffs.candidates : []}
		oncandidate={(key) => {
			closeOthers();
			chosenCandidate = key;
		}}
		gliders={inTheAir.gliders}
		others={othersShown}
		followed={followedTrack}
		{coveredByPage}
		cables={cablesShown}
		oncable={openCable}
		climbs={showClimbs ? climbs.climbs : []}
		onclimb={(key) => openAir(climbs.climb(key))}
		flown={showFlown ? flown.cells : []}
		onflown={(key) => openAir(flown.cell(key))}
		oncredits={(open) => (creditsOpen = open)}
		airspace={app.airspace}
		coveredBottom={navigationHeight}
		onproblem={(problem) =>
			(app.notice = { title: 'Position unavailable', lines: [problemText(problem)] })}
		onregion={(region, zoom) => (view = { region, zoom })}
		onphotos={(line) => (photosShowing = line)}
		onstation={(id) => openStation(wind?.station(id))}
		onsite={(id) => openSite(sites?.site(id))}
		onglider={openGlider}
		onother={openOther}
		onairspace={(count) => (airspaceDrawn = count)}
		hidden={off}
		{aerial}
		{thermalDay}
		{thermalHour}
		radar={showRadar ? radar?.onMap : undefined}
		slopes={slopesChosen ? slopes.shown : undefined}
		onsquare={openSquare}
		onhourat={openHourSquare}
		onplaceat={openPlace}
		reach={reach.place ? { from: reach.place, reach: reach.reach } : undefined}
		{placing}
		{route}
		{around}
		onrouteat={planAt}
		onroutemove={(index, to) => planner.move(index, to)}
	/>
	{#snippet alongTheRoute()}
		<RouteWeather weather={routeWeather} days={routeDays} />
	{/snippet}
	<!-- The design pack's chips: the four layers switched most. Every other
	     layer, and these four in the same state, is a row of the layers' sheet. -->
	{#if app.account?.hasServer}
		<div class="chips" role="group" aria-label="Layers on the map">
			{#each chips as chip (chip.key)}
				<button
					class="chip"
					type="button"
					aria-pressed={chip.on}
					onclick={() => chip.onchange(!chip.on)}
				>
					<Icon name={chip.icon} />
					{chip.name}
				</button>
			{/each}
		</div>
	{/if}
	<!-- A map says nothing to a screen reader: this much it can be told. -->
	{#if airspaceDrawn !== undefined && !app.airspace?.isEmpty}
		<p class="visually-hidden">
			Airspace on this part of the map: {text.count(airspaceDrawn)}.
		</p>
	{/if}
	{#if cablesShown.length > 0 && cables.answer}
		<p class="visually-hidden">
			Cables on this part of the map: {text.count(cablesShown.length)}. {cables.answer.limits}
		</p>
	{/if}
	{#if view && places.known}
		<p class="visually-hidden">
			Flying sites on this part of the map: {text.count(
				countIn([...sitesFitting, ...sitesApart], view.region)
			)}.
		</p>
	{/if}
	{#if view && inTheAir.known}
		<p class="visually-hidden">
			In the air on this part of the map: {text.count(glidersIn(inTheAir.gliders, view.region))}.
		</p>
		{#if othersShown.length > 0 && inTheAir.countWords}
			<p class="visually-hidden">{inTheAir.countWords}.</p>
		{/if}
	{/if}
	<div class="navigation-height" bind:clientHeight={navigationHeight}></div>
	<div
		class="glider-page-size"
		bind:clientWidth={gliderPage.width}
		bind:clientHeight={gliderPage.height}
	></div>
	<div class="buttons-size" bind:clientWidth={buttonsWidth}></div>

	{#snippet thermalCard(chosenHere: boolean)}
		{#if thermals}
			<ThermalMapCard
				map={thermals.map}
				day={thermals.day}
				asking={thermals.asking}
				problem={thermals.problem}
				inView={!view || !thermals.day || hasSquaresIn(thermals.day, view.region)}
				onday={(date) => thermals.chooseDay(date)}
				onretry={() => void thermals.load()}
				layer={thermals.layer}
				heightM={thermals.heightM}
				view={thermals.view}
				hour={thermals.hour}
				hoursAsking={thermals.hoursAsking}
				hoursProblem={thermals.hoursProblem}
				onlayer={(layer) => thermals.chooseLayer(layer)}
				onheight={(heightM) => thermals.chooseHeight(heightM)}
				onhour={(hour) => (thermals.chosenHour = hour)}
				onretryhours={() => void thermals.loadHours()}
				compact={!wide.current}
				keyOpen={thermals.keyOpen}
				onkey={(open) => (thermals.keyOpen = open)}
			>
				{#if chosenHere && chosenHourSquare}
					<div class="chosen-square">
						<header>
							<h3>{thermals.day?.name}</h3>
							<button
								type="button"
								aria-label={app.said?.common.close}
								title={app.said?.common.close}
								onclick={() => (chosenHourSquare = undefined)}
							>
								<Icon name="close" />
							</button>
						</header>
						<ThermalHourSquare
							{thermals}
							place={chosenHourSquare.place}
							cell={chosenHourSquare.cell}
							onforecast={(cell) => thermals.day && squareForecast(cell, thermals.day)}
						/>
					</div>
				{:else if chosenHere && square && thermals.map && thermals.day}
					<div class="chosen-square">
						<header>
							<h3>{thermals.day.name}</h3>
							<button
								type="button"
								aria-label={app.said?.common.close}
								title={app.said?.common.close}
								onclick={() => (chosenSquare = undefined)}
							>
								<Icon name="close" />
							</button>
						</header>
						<ThermalSquare
							{thermals}
							map={thermals.map}
							day={thermals.day}
							cell={square}
							onforecast={() => square && thermals?.day && squareForecast(square, thermals.day)}
							named={false}
							withRun={false}
						/>
					</div>
				{/if}
			</ThermalMapCard>
		{/if}
	{/snippet}

	<!-- What the sites' filter is and what it leaves, while one is on: a row
	     that opens its choices. -->
	{#snippet filterOn(left: NonNullable<typeof sitesLeft>)}
		<button class="forecast-pane filter-on" type="button" onclick={() => (filtering = true)}>
			<Icon name="filter" />
			<SitesFilterLine {left} />
		</button>
	{/snippet}

	<div class="over">
		<!-- On a wide screen the cards of the radar and of the forecast map are
		     panes at the map's side, the radar's first, and a square chosen on
		     the map is shown in the forecast's. -->
		{#if (planner.open || reach.place || showRadar || showForecast || (sites && filtering) || sitesFiltered) && wide.current}
			<div class="panes">
				{#if reach.place}
					<aside class="forecast-pane" aria-label="What a glide reaches">
						<GlideReachCard {reach} />
					</aside>
				{/if}
				{#if planner.open}
					<aside class="forecast-pane" aria-label="Plan a route">
						<RoutePlanCard {planner} onclose={() => planner.close()} weather={alongTheRoute} />
					</aside>
				{/if}
				<!-- The sites' filter: its choices while they are open, and else,
				     while a filter is on, what it is and what it leaves. -->
				{#if sites && filtering}
					<aside class="forecast-pane" aria-label="Filter sites">
						<header class="pane-head">
							<h2>Filter sites</h2>
							<button
								type="button"
								aria-label={app.said?.common.close}
								title={app.said?.common.close}
								onclick={() => (filtering = false)}
							>
								<Icon name="close" />
							</button>
						</header>
						<SitesFilterCard {sites} onremoved={openRemoved} />
					</aside>
				{:else if sitesFiltered}
					{@render filterOn(sitesFiltered)}
				{/if}
				{#if showRadar && radar}
					<aside class="forecast-pane" aria-label="Rain radar">
						<RadarCard {radar} />
					</aside>
				{/if}
				{#if showForecast}
					<aside class="forecast-pane" aria-label="Forecast map">
						{@render thermalCard(true)}
					</aside>
				{/if}
			</div>
		{/if}
		{#if wind}
			<MapPanel
				shown={station?.id}
				title={station?.name ?? ''}
				wide={wide.current}
				onclose={() => (chosen = undefined)}
			>
				{#if station}
					<StationView
						{station}
						{wind}
						onforecast={() => (forecastFor = { place: station.position, name: station.name })}
					/>
				{/if}
			</MapPanel>
		{/if}
		<MapPanel
			shown={chosenSite?.site.id}
			title={chosenSite?.site.name ?? ''}
			wide={wide.current}
			onclose={() => (chosenSite = undefined)}
		>
			{#if chosenSite}
				{@const { site } = chosenSite}
				<SitePage
					{...chosenSite}
					onforecast={() => (forecastFor = { place: site.position, name: site.name })}
					onplan={() => planFrom(site)}
					flightsHref={flownFrom(site) ? placeHref(site.position, site.id) : undefined}
					outlook={siteOutlook}
					written={siteWritten?.id === site.id ? siteWritten.words : undefined}
					own={edits
						? {
								edits,
								oncorrect: () => void correctSite(site),
								onremove: () => (removing = site),
								onhistory: () => (historyOf = { id: site.id }),
								onreport: () => (reportedOfSites = reportedSite(site.id)),
								onchanged: siteConfirmed
							}
						: undefined}
				/>
			{/if}
		</MapPanel>
		{#if siteForm && siteWords}
			<!-- The form for a site that is added or corrected: a pane beside
			     the map on a wide screen, a sheet on a narrow one. It gives way
			     to the map while the pilot moves the place. Its title, notice,
			     sentence of giving and button are the tier's, as the core says. -->
			<MapPanel
				shown={editing && formSays && !placing ? 'site-form' : undefined}
				title={formSays?.title ?? ''}
				wide={wide.current}
				onclose={() => (editing = undefined)}
			>
				{#if editing && formSays}
					<SiteEditForm
						form={siteForm}
						words={siteWords}
						says={formSays}
						{editing}
						compact={!wide.current}
						onmove={movePlace}
						onsent={siteSent}
						onopen={(id) => void openSiteById(id)}
						onshow={showBesideForm}
					/>
				{/if}
			</MapPanel>
		{/if}
		{#if !wide.current && thermals && chosenHourSquare}
			<!-- A square of an hour of the forecast map, on a narrow screen. -->
			<MapPanel
				shown={chosenHourSquare ? String(chosenHourSquare.cell) : undefined}
				title={thermals.day?.name ?? 'Forecast'}
				wide={false}
				half
				onclose={() => (chosenHourSquare = undefined)}
			>
				{#if chosenHourSquare}
					<ThermalHourSquare
						{thermals}
						place={chosenHourSquare.place}
						cell={chosenHourSquare.cell}
						onforecast={(cell) => thermals.day && squareForecast(cell, thermals.day)}
					/>
				{/if}
			</MapPanel>
		{/if}
		{#if !wide.current && thermals?.map && squareDay}
			<!-- A square of the thermal map, on a narrow screen: a sheet of half
			     the screen, the square staying in view above it. -->
			<MapPanel
				shown={square ? `${square.south},${square.west}` : undefined}
				title={squareDay.name}
				wide={false}
				half
				onclose={() => (chosenSquare = undefined)}
			>
				{#if square}
					<ThermalSquare
						{thermals}
						map={thermals.map}
						day={squareDay}
						cell={square}
						onforecast={() => square && squareDay && squareForecast(square, squareDay)}
						named={false}
					/>
				{/if}
			</MapPanel>
		{/if}
		<!-- A place clicked on the map, where nothing of Venti's lay: what
		     airspace lies over it, and what can be done from there. -->
		<MapPanel
			shown={chosenPlace ? `${chosenPlace.place.lat},${chosenPlace.place.lon}` : undefined}
			title={chosenPlace?.name ?? 'Place'}
			wide={wide.current}
			onclose={() => (chosenPlace = undefined)}
		>
			{#if chosenPlace}
				{@const { place } = chosenPlace}
				{#if chosenPlace.said}
					<!-- A climb of now or a cell of flown thermals: the core's line about it. -->
					<p class="air-line numeric">{chosenPlace.said}</p>
				{/if}
				{#if chosenPlace.airspace && app.airspace}
					<AirspaceOver {place} airspace={app.airspace} />
				{/if}
				<div class="group-card here">
					<button class="group-row" type="button" onclick={() => reachFrom(place)}>
						What a glide reaches from here
					</button>
					{#if adding}
						<!-- The core's entry for the tier this pilot adds to; for
						     someone who has to sign in first, its sentence about that,
						     which leads to the account. -->
						{#if adding.leadsToSignIn}
							<a class="group-row entry" href={accountHref()}>{adding.entryWords}</a>
						{:else}
							<button class="group-row" type="button" onclick={() => void addSiteAt(place)}>
								{adding.entryWords}
							</button>
						{/if}
					{/if}
				</div>
			{/if}
		</MapPanel>
		{#if live}
			<!-- Another pilot's glider. Its flight is on the map while this is
			     open, so a narrow screen keeps its upper half for the map. -->
			<MapPanel
				shown={chosenGlider}
				title={glider ? `@${glider.handle}` : 'Glider'}
				wide={wide.current}
				half
				onclose={() => (chosenGlider = undefined)}
			>
				{#if chosenGlider}
					<GliderView id={chosenGlider} {live} onreport={() => (reportedGlider = chosenGlider)} />
				{/if}
			</MapPanel>
		{/if}
		<!-- A cable clicked on the map: what it is, whose the data is, and
		     what the layer cannot promise, in the core's words. -->
		<MapPanel
			shown={cable ? chosenCable : undefined}
			title={cable?.name ?? cable?.words ?? ''}
			wide={wide.current}
			half
			onclose={() => (chosenCable = undefined)}
		>
			{#if cable}
				<div class="cable" data-cable={cable.id}>
					{#if cable.name}<p class="cable-words">{cable.words}</p>{/if}
					{#if cables.answer}<p class="cable-limits">{cables.answer.limits}</p>{/if}
					{#if cableCredit}
						<p class="cable-credit">
							<a href={cableCredit.url} rel="noopener noreferrer" target="_blank"
								>{cableCredit.text}</a
							>
							{#if cableCredit.notice}<span>{cableCredit.notice}</span>{/if}
						</p>
					{/if}
				</div>
			{/if}
		</MapPanel>
		<!-- An aircraft another network hears: a small sheet, with no flight to follow. -->
		<MapPanel
			shown={other ? chosenOther : undefined}
			title={other?.aircraft.kindWords ?? ''}
			wide={wide.current}
			half
			onclose={() => (chosenOther = undefined)}
		>
			{#if other}
				<OtherView aircraft={other.aircraft} credit={other.network.credit} />
			{/if}
		</MapPanel>
		{#if takeoffs?.words}
			<!-- A place where pilots take off and no site stands: a small sheet. -->
			<MapPanel
				shown={candidate ? chosenCandidate : undefined}
				title={candidate ? candidateHeading(candidate, takeoffs.words.candidateTitle) : ''}
				wide={wide.current}
				half
				onclose={() => (chosenCandidate = undefined)}
			>
				{#if candidate}
					<CandidateView
						{candidate}
						offer={adding?.mayAdd ? offerFor(takeoffs.offers, candidate) : undefined}
						terms={takeoffs.answer?.terms.text}
						onadd={addOffered}
						ondismiss={(offer) => void dismissOffered(offer)}
					/>
				{/if}
			</MapPanel>
		{/if}
		<!-- Every layer and every tool of the map, in one place. -->
		<MapPanel
			shown={layersOpen ? 'layers' : undefined}
			title={layerWords?.title ?? ''}
			wide={wide.current}
			onclose={() => (layersOpen = false)}
		>
			<MapLayersSheet
				layers={layerRows}
				tools={toolRows}
				layersTitle={layerWords?.onTheMap ?? ''}
				toolsTitle={layerWords?.listsTitle ?? ''}
			/>
		</MapPanel>
		{#if said}
			<p class="notice" role="status">
				<Icon name="warning" />
				{said}
			</p>
		{/if}
		{#if candidatesSaid}
			<p class="notice" role="status">{candidatesSaid}</p>
		{/if}
		{#if slopesTooFar}
			<p class="notice" role="status">Zoom in to a hillside to see how steep it is.</p>
		{:else if slopesChosen && slopes.problem}
			<p class="notice" role="status">{slopes.problem}</p>
		{/if}
		{#if cables.problem && cables.close && !off.includes('cables')}
			<p class="notice" role="status">{cables.problem}</p>
		{/if}
		{#if inTheAir.othersSaid && !off.includes('pilots')}
			<p class="notice" role="status">{inTheAir.othersSaid}</p>
		{/if}
		{#if climbsSaid}
			<p class="notice" role="status">{climbsSaid}</p>
		{/if}
		{#if flownSaid}
			<p class="notice" role="status">{flownSaid}</p>
		{/if}
	</div>

	<!-- Whose wind, airspace and flying sites are on the map. Their licences
	     ask for the credit next to the data; the full wording is on each
	     station's page, on the airspace screen and on each site's page. -->
	<div class="foot" bind:clientHeight={footHeight}>
		{#if placing && siteForm && siteWords}
			<!-- The map is moved under the mark; this says so and takes the place. -->
			<div
				class="forecast-card placing"
				role="group"
				aria-label={editing?.corrects ? (formSays?.title ?? '') : (adding?.entryWords ?? '')}
			>
				<p>{siteForm.placeWords}</p>
				{#if placingProblem}
					<p class="refusal" role="alert">{placingProblem}</p>
				{/if}
				<div class="placing-acts">
					<button class="button-prominent" type="button" onclick={() => void usePlace()}>
						{siteWords.usePlace}
					</button>
					<button class="button-plain" type="button" onclick={() => (placing = false)}>
						{siteWords.cancel}
					</button>
				</div>
			</div>
		{/if}
		<!-- On a narrow screen the route planner's card stands over the map's
		     foot, as the phones' panel does, and leaves the map to the finger. -->
		{#if planner.open && !placing && !wide.current}
			<div class="forecast-card">
				<RoutePlanCard {planner} onclose={() => planner.close()} weather={alongTheRoute} />
			</div>
		{/if}
		{#if reach.place && !placing && !wide.current}
			<div class="forecast-card">
				<GlideReachCard {reach} />
			</div>
		{/if}
		<!-- On a narrow screen the radar's card and the forecast map's stand
		     over the credits. -->
		{#if sitesFiltered && !wide.current}
			{@render filterOn(sitesFiltered)}
		{/if}
		{#if slopesChosen && slopes.shown && !placing}
			<div class="forecast-card">
				<SlopesKey />
			</div>
		{/if}
		{#if showRadar && radar && !wide.current}
			<div class="forecast-card">
				<RadarCard {radar} compact />
			</div>
		{/if}
		{#if showForecast && !wide.current}
			<div class="forecast-card">
				{@render thermalCard(false)}
			</div>
		{/if}
		<!-- Closed from the start: the map's "i" unfolds them with its own,
		     every one complete. -->
		{#if creditsOpen && (credits || airspaceCredit || sitesCredit || photosCredit || forecastCredit || radarCredits.length > 0 || othersCredits.length > 0 || cablesCredits.length > 0 || climbsCredits.length > 0)}
			<p class="credits">
				{#if photosCredit}<span>{photosCredit}</span>{/if}
				{#if credits}<span>Wind: {credits}</span>{/if}
				{#if airspaceCredit}<span>{airspaceCredit}</span>{/if}
				{#each cablesCredits as credit (credit)}<span>Cables: {credit}</span>{/each}
				{#if sitesCredit}<span>{sitesCredit}</span>{/if}
				{#if forecastCredit}<span>{forecastCredit}</span>{/if}
				{#each radarCredits as credit (credit)}<span>{credit}</span>{/each}
				{#each othersCredits as credit (credit.source)}<span>{credit.text}</span>{/each}
				{#each climbsCredits as credit (credit.source)}
					{#if !othersCredits.some((other) => other.text === credit.text)}
						<span>{credit.text}</span>
					{/if}
				{/each}
			</p>
		{/if}
	</div>

	<div class="controls">
		<div class="zoom">
			<MapButton label="Zoom in" icon="plus" onclick={() => map?.zoomBy(1)} />
			<MapButton label="Zoom out" icon="minus" onclick={() => map?.zoomBy(-1)} />
		</div>
		<!-- Every layer and every tool: one sheet. Shown from the start, so
		     that the ground can be changed where no server answers. -->
		<MapButton
			label={layerWords?.title || 'Layers'}
			icon="layers"
			pressed={layersOpen}
			onclick={() => {
				const open = !layersOpen;
				if (open) closeOthers();
				layersOpen = open;
			}}
		/>
		<MapButton
			label="Show my position"
			icon="location"
			pressed={following}
			onclick={() => (following = !following)}
		/>
	</div>
</div>

<StationList
	open={listing}
	wind={current}
	onchoose={(picked) => {
		listing = false;
		openStation(picked);
	}}
	onclose={() => (listing = false)}
/>

<!-- What the weather models say about the place of a site or a station. -->
<Sheet open={Boolean(forecastFor)} title="Forecast" onclose={() => (forecastFor = undefined)}>
	{#if forecastFor}
		<ForecastView {...forecastFor} ask={(place) => app.forecast(place)} />
	{/if}
</Sheet>

<!-- The sites' filter on a narrow screen: a sheet of half the screen, the
     map staying in view above it while the pilot chooses. -->
{#if sites}
	<Sheet
		open={filtering && !wide.current}
		title="Filter sites"
		half
		onclose={() => (filtering = false)}
	>
		{#if filtering && !wide.current}
			<SitesFilterCard {sites} compact onremoved={openRemoved} />
		{/if}
	</Sheet>
{/if}

<SiteList
	open={listingSites}
	sites={places}
	model={sites}
	onchoose={(picked) => {
		listingSites = false;
		openSite(picked);
	}}
	onremoved={openRemoved}
	onclose={() => (listingSites = false)}
/>

{#if live}
	<LiveList
		open={listingGliders}
		{live}
		onchoose={(picked) => {
			listingGliders = false;
			openGlider(picked.id);
		}}
		onclose={() => (listingGliders = false)}
	/>
{/if}

<!-- A live track that should not be there is reported from its glider's page. -->
<ReportSheet
	open={reportedGlider !== undefined}
	{...reportedTrack(reportedGlider ?? '')}
	onclose={() => (reportedGlider = undefined)}
/>

<!-- A site's history: its revisions, and the way back to any of them. -->
<Sheet
	open={historyOf !== undefined}
	title={siteWords?.history ?? ''}
	onclose={() => (historyOf = undefined)}
>
	{#if historyOf && edits}
		{#if historyOf.said}
			<p class="group-card removed" role="status">{historyOf.said}</p>
		{/if}
		<SiteHistoryView
			{edits}
			id={historyOf.id}
			onwritten={putBack}
			onreport={(revision) =>
				historyOf && (reportedOfSites = reportedSiteRevision(historyOf.id, revision))}
		/>
	{/if}
</Sheet>

<!-- "Removed lately": the removed sites of Venti's tiers, and the way back for each. -->
<Sheet
	open={removedLately}
	title={siteWords?.removedTitle ?? ''}
	onclose={() => (removedLately = false)}
>
	{#if removedLately && edits && sites}
		<SitesRemovedView
			{edits}
			withPilots={sites.filter.withPilots ?? false}
			pilotsTitle={sites.choices?.withPilotsWords ?? ''}
			onwritten={broughtBack}
		/>
	{/if}
</Sheet>

<!-- Removing a site asks for the line that is kept with it first. -->
<PromptDialog
	open={removing !== undefined}
	title={siteWords?.removeQuestion ?? ''}
	message={[siteWords?.removeAbout, why?.help].filter(Boolean).join(' ')}
	label={why?.title ?? ''}
	confirm={siteWords?.remove ?? ''}
	optional
	working={removingNow}
	refusal={removeRefusal}
	onconfirm={(comment) => void removeSite(comment)}
	oncancel={() => {
		removing = undefined;
		removeRefusal = undefined;
	}}
/>

<!-- A site that should not be there, or one revision of it, is reported from its page. -->
<ReportSheet
	open={reportedOfSites !== undefined}
	{...reportedOfSites ?? reportedSite('')}
	onclose={() => (reportedOfSites = undefined)}
/>

<style>
	.screen {
		position: relative;
		height: calc(100dvh - var(--nav-top));
		overflow: hidden;
		/* The map's own credits keep clear of the navigation too. */
		--map-covered-bottom: var(--nav-bottom);
	}

	/* Measures the navigation's reach, which the stylesheet knows and the map
	   needs in pixels. */
	/* Along the top of the map, as the pack's map screen has them. */
	.chips {
		position: absolute;
		top: calc(var(--space-3) + var(--safe-top));
		left: calc(var(--space-3) + var(--safe-left));
		right: calc(var(--space-3) + var(--safe-right));
		display: flex;
		gap: var(--space-2);
		overflow-x: auto;
		pointer-events: none;
	}

	/* The pack's `.v-chip`: 36 px drawn, with its tap area padded to 48. */
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		height: 2.25rem;
		margin: 0.375rem 0;
		padding: 0 0.875rem 0 0.625rem;
		border-radius: var(--radius-pill);
		background: var(--surface);
		color: var(--text);
		box-shadow:
			inset 0 0 0 1px var(--separator),
			var(--shadow-float);
		font-size: var(--subheadline);
		font-weight: 500;
		white-space: nowrap;
		pointer-events: auto;
	}

	.chip :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	/* At a phone's width the four share the row and none is cut off; with
	   large text they keep their words and the row scrolls. */
	@media (max-width: 55.99rem) {
		.chips {
			gap: var(--space-1);
		}

		.chip {
			flex: 1 0 auto;
			justify-content: center;
			gap: var(--space-1);
			padding: 0 var(--space-2) 0 var(--space-1);
		}
	}

	.chip[aria-pressed='true'] {
		background: var(--accent);
		color: var(--on-accent);
		box-shadow: var(--shadow-float);
	}

	.navigation-height {
		position: absolute;
		visibility: hidden;
		height: var(--nav-bottom);
	}

	/* A glider's page: the card at the map's side with the gap before it on a
	   wide screen, half the screen's height on a narrow one. */
	.glider-page-size {
		position: absolute;
		visibility: hidden;
		width: calc(25rem + var(--safe-left));
		height: 50dvh;
	}

	/* The round buttons with the gap between them and the edge. */
	.buttons-size {
		position: absolute;
		visibility: hidden;
		width: calc(4rem + var(--safe-right));
	}

	/* What lies over the top of the map: a chosen station at the side of a
	   wide screen, and what the pilot has to be told about the wind. */
	.over {
		position: absolute;
		/* Under the row of chips, which stays in reach. */
		top: calc(var(--space-3) + 3rem + var(--safe-top));
		right: calc(1rem + var(--safe-right));
		/* The credits at the bottom edge stay in sight. */
		bottom: calc(4rem + var(--nav-bottom));
		left: calc(1rem + var(--safe-left));
		display: flex;
		align-items: flex-start;
		gap: 1rem;
		pointer-events: none;
	}

	.notice {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		max-width: 30rem;
		margin: 0 auto;
		padding: 0.5rem 0.875rem 0.5rem 0.625rem;
		border-radius: 0.875rem;
		background: var(--surface);
		box-shadow: var(--shadow-float);
		font-size: var(--footnote);
		pointer-events: auto;
	}

	.notice :global(svg) {
		color: var(--text-secondary);
	}

	/* The map's lower edge, over its own credits and left of the round
	   buttons: the forecast map's card, and the credits of what Venti draws. */
	.foot {
		position: absolute;
		left: calc(0.75rem + var(--safe-left));
		right: calc(5rem + var(--safe-right));
		bottom: calc(var(--nav-bottom) + 2.875rem);
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		pointer-events: none;
	}

	.forecast-card {
		align-self: stretch;
		max-width: 26rem;
		/* At most two fifths of the screen. The card is short until the pilot
		   unfolds its key and notes; what is then too much scrolls inside. */
		--forecast-card-height: 40dvh;
		max-height: var(--forecast-card-height);
		overflow-y: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		pointer-events: auto;
	}

	/* A cable's sheet: its words, the layer's limits, its credit. */
	.cable {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		font-size: var(--subheadline);
	}

	.cable p {
		margin: 0;
	}

	.cable-limits,
	.cable-credit {
		color: var(--text-secondary);
		font-size: var(--footnote);
	}

	.cable-credit {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.cable-credit a {
		color: var(--link);
	}

	/* The core's line about a climb or a cell, over what can be done there. */
	.air-line {
		margin: 0;
		font-size: var(--subheadline);
	}

	/* "Add a site here", under what lies over the place. */
	.here {
		margin-top: var(--space-3);
	}

	/* The entry as a link, where it leads to the account. */
	.entry {
		color: var(--link);
	}

	/* The card that takes a place for a site, over the credits. */
	.placing {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.placing .refusal {
		color: var(--danger);
	}

	.placing-acts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.removed {
		margin-bottom: var(--space-3);
		padding: 0.75rem 1rem;
	}

	/* The panes at the map's side on a wide screen, one under the other;
	   each scrolls inside itself where the two are higher than the map. */
	.panes {
		flex: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		width: 18rem;
		max-height: 100%;
		min-height: 0;
	}

	/* A pane at the map's side on a wide screen, as dense as the rest of it. */
	.forecast-pane {
		flex: 0 1 auto;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		pointer-events: auto;
	}

	/* The head of a pane that the pilot opened and closes. */
	.pane-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: var(--space-2);
	}

	.pane-head h2 {
		margin: 0;
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.pane-head button {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1);
		color: var(--text-secondary);
	}

	/* The filter that is on, as a row to press: a pane of its own on a wide
	   screen, a short card over the credits on a narrow one. */
	.filter-on {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		max-width: 26rem;
		text-align: left;
	}

	.filter-on :global(svg) {
		flex: none;
		width: 1.125rem;
		height: 1.125rem;
		color: var(--link);
	}

	.chosen-square {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-1);
		padding-top: var(--space-3);
		border-top: 1px solid var(--separator);
	}

	.chosen-square header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.chosen-square h3 {
		margin: 0;
		font-size: var(--subheadline);
	}

	.chosen-square header button {
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin: calc(var(--space-2) * -1);
		color: var(--text-secondary);
	}

	/* In the manner of the map's own credits, directly above them. */
	.credits {
		max-width: 100%;
		margin: 0;
		pointer-events: auto;
		padding: 0.125rem 0.5rem;
		border-radius: 0.75rem;
		background: var(--on-map-paper);
		-webkit-backdrop-filter: blur(0.75rem);
		backdrop-filter: blur(0.75rem);
		/* The map is light in both appearances, and so is what is written on it. */
		color: var(--on-map-ink);
		font-size: var(--caption);
		line-height: 1.3;
		/* Every provider is named, on a second line if one is too short: the
		   credit is owed to each of them. */
		text-wrap: pretty;
	}

	/* Wind, airspace and sites each have a line of their own. */
	.credits span {
		display: block;
	}

	.controls {
		position: absolute;
		right: calc(1rem + var(--safe-right));
		bottom: calc(1rem + var(--nav-bottom));
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		pointer-events: none;
	}

	.zoom {
		display: none;
		flex-direction: column;
		gap: 0.5rem;
	}

	/* Fingers pinch to zoom. A mouse gets buttons, which the keyboard reaches too. */
	@media (min-width: 56rem) {
		/* Compact on a wide screen: chips and buttons about 32 px. */
		.chip {
			height: 2rem;
			margin: 0;
			padding: 0 0.75rem 0 0.5rem;
		}

		.chip :global(svg) {
			width: 1rem;
			height: 1rem;
		}

		.chips {
			padding: var(--space-1) 0;
		}

		.over {
			top: calc(var(--space-3) + 2.75rem + var(--safe-top));
		}

		.controls {
			gap: var(--space-2);
		}

		.zoom {
			display: flex;
		}

		/* The map's credits are written out in one line here, not behind a button. */
		.foot {
			bottom: 2.375rem;
		}

		/* What lies at the map's side, the panes and a station's or a site's
		   card, ends above the foot, whose credits run to several lines: the
		   foot's top lies 2.375 rem and its height above the screen's edge. */
		.over {
			bottom: max(4rem, calc(2.375rem + var(--foot-height, 0px) + var(--space-3)));
		}
	}
</style>
