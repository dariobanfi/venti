<!--
	The map tab: terrain, live wind from the stations of this site's Venti
	server, and the pilot's own flights. Sites and forecasts arrive later.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { getApp } from '#lib/app/app.svelte.ts';
	import type { Bounds, WindStation } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import MapButton from '#lib/map/MapButton.svelte';
	import OverviewMap from '#lib/map/OverviewMap.svelte';
	import { problemText } from '#lib/map/position.ts';
	import StationList from '#lib/wind/StationList.svelte';
	import StationPanel from '#lib/wind/StationPanel.svelte';
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

	/** The part of the map in view. The map may be there before the core is. */
	let region = $state.raw<Bounds>();
	$effect(() => {
		if (region) wind?.show(region);
	});
	// Asks again every minute while the map is on screen.
	$effect(() => wind?.keepCurrent());

	/** The station the pilot chose, as it was then. */
	let chosen = $state.raw<WindStation>();
	/** The same station with its newest reading, for as long as the map still has it. */
	const station = $derived(chosen && (wind?.station(chosen.id) ?? chosen));
	let listing = $state(false);

	const said = $derived(notice(current, text.list));
	const credits = $derived(text.list(current.credits.map((credit) => credit.provider)));
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
		coveredBottom={navigationHeight}
		onproblem={(problem) =>
			(app.notice = { title: 'Position unavailable', lines: [problemText(problem)] })}
		onregion={(shown) => (region = shown)}
		onstation={(id) => (chosen = wind?.station(id))}
	/>
	<div class="navigation-height" bind:clientHeight={navigationHeight}></div>

	<div class="over">
		{#if wind}
			<StationPanel {station} {wind} wide={wide.current} onclose={() => (chosen = undefined)} />
		{/if}
		{#if said}
			<p class="notice" role="status">
				<Icon name="warning" />
				{said}
			</p>
		{/if}
	</div>

	<!-- Whose wind data is on the map. Their licences ask for the credit next
	     to the data; the full wording is on each station's page. -->
	{#if credits}
		<p class="credits">Wind: {credits}</p>
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
		chosen = picked;
	}}
	onclose={() => (listing = false)}
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
