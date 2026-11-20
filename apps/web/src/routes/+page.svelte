<!--
	The map tab: terrain, live wind from the stations of this site's Venti
	server, the flying sites it knows, the pilots who show where they fly,
	the pilot's airspace and the pilot's own flights.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { creditLine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Bounds, Coordinate, Credit, SiteView, WindStation } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { reportedTrack } from '#lib/community/report.ts';
	import ReportSheet from '#lib/community/ReportSheet.svelte';
	import ForecastView from '#lib/forecast/ForecastView.svelte';
	import GliderView from '#lib/live/GliderView.svelte';
	import LiveList from '#lib/live/LiveList.svelte';
	import { countIn as glidersIn } from '#lib/live/mark.ts';
	import { NO_LIVE } from '#lib/live/watch.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import MapPanel from '#lib/map/MapPanel.svelte';
	import OverviewMap from '#lib/map/OverviewMap.svelte';
	import { problemText } from '#lib/map/position.ts';
	import { countIn } from '#lib/sites/mark.ts';
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

	/**
	 * The place whose forecast is looked at, with what it is called: a
	 * site's or a station's page leads there.
	 */
	let forecastFor = $state.raw<{ place: Coordinate; name: string }>();

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

	/** One thing is open beside the map at a time: a station, a site or a glider. */
	function openStation(picked: WindStation | undefined) {
		if (!picked) return;
		chosenSite = undefined;
		chosenGlider = undefined;
		chosen = picked;
	}

	function openGlider(id: string) {
		chosen = undefined;
		chosenSite = undefined;
		chosenGlider = id;
	}

	function openSite(picked: SiteView | undefined) {
		if (!picked) return;
		chosen = undefined;
		chosenGlider = undefined;
		chosenSite = {
			site: picked,
			credit: places.credits.find((credit) => credit.source === picked.source),
			asOfUnixS: places.asOfUnixS
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
		onstation={(id) => openStation(wind?.station(id))}
		onsite={(id) => openSite(sites?.site(id))}
		onglider={openGlider}
		onairspace={(count) => (airspaceDrawn = count)}
	/>
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

	<div class="over">
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
				/>
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
	{#if credits || airspaceCredit || sitesCredit}
		<p class="credits">
			{#if credits}<span>Wind: {credits}</span>{/if}
			{#if airspaceCredit}<span>{airspaceCredit}</span>{/if}
			{#if sitesCredit}<span>{sitesCredit}</span>{/if}
		</p>
	{/if}

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
		top: calc(0.75rem + var(--safe-top));
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

	/* In the manner of the map's own credits, directly above them. */
	.credits {
		position: absolute;
		left: calc(0.75rem + var(--safe-left));
		bottom: calc(var(--nav-bottom) + 2.875rem);
		max-width: calc(100% - 6rem);
		padding: 0.125rem 0.5rem;
		border-radius: 0.75rem;
		background: rgb(255 255 255 / 0.78);
		-webkit-backdrop-filter: blur(0.75rem);
		backdrop-filter: blur(0.75rem);
		/* The map is light in both appearances, and so is what is written on it. */
		color: #3a3a3c;
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
		.zoom {
			display: flex;
		}

		/* The map's credits are written out in one line here, not behind a button. */
		.credits {
			bottom: 2.375rem;
		}
	}
</style>
