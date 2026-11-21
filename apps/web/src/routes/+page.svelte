<!--
	The map tab: terrain, live wind from the stations of this site's Venti
	server, the flying sites it knows, the pilots who show where they fly,
	the pilot's airspace and the pilot's own flights.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { creditLine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { placeHref } from '#lib/app/routes.ts';
	import type {
		Bounds,
		Coordinate,
		Credit,
		SiteOutlook,
		SiteView,
		WindStation
	} from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { reportedTrack } from '#lib/community/report.ts';
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
	import { NO_LIVE } from '#lib/live/watch.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import MapPanel from '#lib/map/MapPanel.svelte';
	import OverviewMap from '#lib/map/OverviewMap.svelte';
	import {
		aerialOn,
		forecastOn,
		keepAerial,
		keepForecast,
		keepLayersOff,
		type MapLayer,
		MAP_LAYERS,
		layersOff
	} from '#lib/map/layers.ts';
	import { problemText } from '#lib/map/position.ts';
	import { countIn, flownFrom } from '#lib/sites/mark.ts';
	import SiteList from '#lib/sites/SiteList.svelte';
	import SitePage from '#lib/sites/SiteView.svelte';
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
	/** How far the navigation reaches in from the bottom edge, measured in pixels. */
	let navigationHeight = $state(0);

	/** The layers the pilot switched off with the chips, as this browser remembers them. */
	let off = $state<MapLayer[]>(layersOff(globalThis.localStorage));
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
	/** The square the pilot chose, by its corner: the same square on another day. */
	let chosenSquare = $state.raw<{ south: number; west: number }>();
	const square = $derived(
		thermalDay?.cells.find(
			(cell) =>
				isDrawn(cell) && cell.south === chosenSquare?.south && cell.west === chosenSquare?.west
		)
	);
	/** Whose forecast the squares are. */
	const forecastCredit = $derived(showForecast ? forecastCreditLine(thermals?.map, text) : '');

	const CHIP_ICONS = {
		wind: 'wind',
		airspace: 'airspace',
		sites: 'pin',
		pilots: 'glider'
	} as const;

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
	 * The place whose airspace is open, with what lies over it, after a
	 * click where nothing else of the map's lies under the pointer.
	 */
	let chosenAirspace = $state.raw<{ place: Coordinate; name: string }>();
	/** Counts the clicks on airspace, so that a slow answer for an older one is dropped. */
	let airspaceAsked = 0;

	/** Opens what airspace lies over a place, if any does: a click elsewhere on the map does nothing. */
	function openAirspace(place: Coordinate) {
		const model = app.airspace;
		if (!model || model.isEmpty) return;
		const mine = ++airspaceAsked;
		model.over(place).then(
			(over) => {
				if (mine !== airspaceAsked || over.airspaces.length === 0) return;
				chosenSquare = undefined;
				chosen = undefined;
				chosenSite = undefined;
				chosenGlider = undefined;
				chosenAirspace = { place, name: text.place(place.lat, place.lon) };
			},
			() => {}
		);
	}

	/** One thing is open beside the map at a time: a station, a site, a glider, a square or airspace. */
	function openStation(picked: WindStation | undefined) {
		if (!picked) return;
		chosenAirspace = undefined;
		chosenSquare = undefined;
		chosenSite = undefined;
		chosenGlider = undefined;
		chosen = picked;
	}

	function openGlider(id: string) {
		chosenAirspace = undefined;
		chosenSquare = undefined;
		chosen = undefined;
		chosenSite = undefined;
		chosenGlider = id;
	}

	function openSite(picked: SiteView | undefined) {
		if (!picked) return;
		chosenAirspace = undefined;
		chosenSquare = undefined;
		chosen = undefined;
		chosenGlider = undefined;
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
				openAirspace(place);
				return;
			}
			chosenAirspace = undefined;
			chosen = undefined;
			chosenSite = undefined;
			chosenGlider = undefined;
			chosenSquare = undefined;
			chosenHourSquare = { place, cell };
		});
	}

	function openSquare(cell: number) {
		const picked = thermalDay?.cells[cell];
		if (!picked) return;
		chosenAirspace = undefined;
		chosen = undefined;
		chosenSite = undefined;
		chosenGlider = undefined;
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

	const said = $derived(notice(current, text.list));
	const credits = $derived(text.list(current.credits.map((credit) => credit.provider)));
	/** Whose sites are on the map. Their licence asks for this wherever they are shown. */
	const sitesCredit = $derived(
		places.sites.length === 0 ? '' : sitesCreditLine(places.credits, text)
	);
</script>

<svelte:head>
	<title>Map · Venti</title>
</svelte:head>

<h1 class="visually-hidden">Map</h1>
<div class="screen">
	<OverviewMap
		bind:this={map}
		bind:following
		outlines={app.logbook?.outlines ?? []}
		stations={current.stations}
		sites={places.sites}
		gliders={inTheAir.gliders}
		followed={followedTrack}
		{coveredByPage}
		airspace={app.airspace}
		coveredBottom={navigationHeight}
		onproblem={(problem) =>
			(app.notice = { title: 'Position unavailable', lines: [problemText(problem)] })}
		onregion={(region, zoom) => (view = { region, zoom })}
		onphotos={(line) => (photosShowing = line)}
		onstation={(id) => openStation(wind?.station(id))}
		onsite={(id) => openSite(sites?.site(id))}
		onglider={openGlider}
		onairspace={(count) => (airspaceDrawn = count)}
		hidden={off}
		{aerial}
		{thermalDay}
		{thermalHour}
		onsquare={openSquare}
		onhourat={openHourSquare}
		onairspaceat={openAirspace}
	/>
	<!-- The design pack's chips: what the map shows, switched on and off. -->
	{#if app.account?.hasServer}
		<div class="chips" role="group" aria-label="Layers on the map">
			<!-- The kind of day, square by square: the server's thermal map. -->
			<button
				class="chip"
				type="button"
				aria-pressed={forecast}
				onclick={() => {
					forecast = !forecast;
					keepForecast(globalThis.localStorage, forecast);
					if (!forecast) chosenSquare = undefined;
				}}
			>
				<Icon name="forecast" />
				Forecast
			</button>
			{#each MAP_LAYERS as { layer, name } (layer)}
				<button
					class="chip"
					type="button"
					aria-pressed={!off.includes(layer)}
					onclick={() => toggle(layer)}
				>
					<Icon name={CHIP_ICONS[layer]} />
					{name}
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
	{#if view && places.known}
		<p class="visually-hidden">
			Flying sites on this part of the map: {text.count(countIn(places.sites, view.region))}.
		</p>
	{/if}
	{#if view && inTheAir.known}
		<p class="visually-hidden">
			In the air on this part of the map: {text.count(glidersIn(inTheAir.gliders, view.region))}.
		</p>
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
			>
				{#if chosenHere && chosenHourSquare}
					<div class="chosen-square">
						<header>
							<h3>{thermals.day?.name}</h3>
							<button
								type="button"
								aria-label="Close"
								title="Close"
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
								aria-label="Close"
								title="Close"
								onclick={() => (chosenSquare = undefined)}
							>
								<Icon name="close" />
							</button>
						</header>
						<ThermalSquare
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

	<div class="over">
		<!-- On a wide screen the forecast map's card is a pane at the map's
		     side, and a square chosen on the map is shown in it. -->
		{#if showForecast && wide.current}
			<aside class="forecast-pane" aria-label="Forecast map">
				{@render thermalCard(true)}
			</aside>
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
					flightsHref={flownFrom(site) ? placeHref(site.position, site.id) : undefined}
					outlook={siteOutlook}
				/>
			{/if}
		</MapPanel>
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
		{#if !wide.current && thermals?.map && thermalDay}
			<!-- A square of the thermal map, on a narrow screen: a sheet of half
			     the screen, the square staying in view above it. -->
			<MapPanel
				shown={square ? `${square.south},${square.west}` : undefined}
				title={thermalDay.name}
				wide={false}
				half
				onclose={() => (chosenSquare = undefined)}
			>
				{#if square}
					<ThermalSquare
						map={thermals.map}
						day={thermalDay}
						cell={square}
						onforecast={() => square && thermalDay && squareForecast(square, thermalDay)}
						named={false}
					/>
				{/if}
			</MapPanel>
		{/if}
		{#if app.airspace}
			<!-- What airspace lies over a place clicked on the map. -->
			<MapPanel
				shown={chosenAirspace
					? `${chosenAirspace.place.lat},${chosenAirspace.place.lon}`
					: undefined}
				title={chosenAirspace?.name ?? 'Airspace'}
				wide={wide.current}
				onclose={() => (chosenAirspace = undefined)}
			>
				{#if chosenAirspace}
					<AirspaceOver place={chosenAirspace.place} airspace={app.airspace} />
				{/if}
			</MapPanel>
		{/if}
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
		{#if said}
			<p class="notice" role="status">
				<Icon name="warning" />
				{said}
			</p>
		{/if}
	</div>

	<!-- Whose wind, airspace and flying sites are on the map. Their licences
	     ask for the credit next to the data; the full wording is on each
	     station's page, on the airspace screen and on each site's page. -->
	<div class="foot">
		<!-- On a narrow screen the forecast map's card stands over the credits. -->
		{#if showForecast && !wide.current}
			<div class="forecast-card">
				{@render thermalCard(false)}
			</div>
		{/if}
		{#if credits || airspaceCredit || sitesCredit || photosCredit || forecastCredit}
			<p class="credits">
				{#if photosCredit}<span>{photosCredit}</span>{/if}
				{#if credits}<span>Wind: {credits}</span>{/if}
				{#if airspaceCredit}<span>{airspaceCredit}</span>{/if}
				{#if sitesCredit}<span>{sitesCredit}</span>{/if}
				{#if forecastCredit}<span>{forecastCredit}</span>{/if}
			</p>
		{/if}
	</div>

	<div class="controls">
		<div class="zoom">
			<MapButton label="Zoom in" icon="plus" onclick={() => map?.zoomBy(1)} />
			<MapButton label="Zoom out" icon="minus" onclick={() => map?.zoomBy(-1)} />
		</div>
		{#if current.available}
			<!-- Everything the arrows say, as a list: for reading the numbers
			     side by side, and for the keyboard and screen readers. -->
			<MapButton label="Wind stations" icon="wind" onclick={() => (listing = true)} />
			<!-- The signs on the map, as a list. The button is there before
			     the map is close enough for signs: the list then says how to
			     get them. -->
			<MapButton label="Flying sites" icon="pin" onclick={() => (listingSites = true)} />
			<!-- Everyone the map shows in the air, as a list. -->
			<MapButton label="In the air" icon="glider" onclick={() => (listingGliders = true)} />
		{/if}
		<!-- The ground under everything: the drawn map, or photographs from above. -->
		<MapButton
			label="Aerial view"
			icon="layers"
			pressed={aerial}
			onclick={() => {
				aerial = !aerial;
				keepAerial(globalThis.localStorage, aerial);
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

<SiteList
	open={listingSites}
	sites={places}
	onchoose={(picked) => {
		listingSites = false;
		openSite(picked);
	}}
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

<style>
	.screen {
		position: relative;
		height: 100dvh;
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
		/* At most two fifths of the screen; the rest scrolls inside. */
		max-height: 40dvh;
		overflow-y: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		pointer-events: auto;
	}

	/* A pane at the map's side on a wide screen, as dense as the rest of it. */
	.forecast-pane {
		flex: none;
		width: 18rem;
		max-height: 100%;
		overflow-y: auto;
		padding: var(--space-3);
		border-radius: var(--radius-card);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		pointer-events: auto;
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
	}
</style>
