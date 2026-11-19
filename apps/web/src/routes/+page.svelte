<!--
	The map tab. Today it shows terrain and the pilot's own flights; live wind,
	sites and forecasts arrive with the server.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import OverviewMap from '#lib/map/OverviewMap.svelte';
	import { problemText } from '#lib/map/position.ts';

	const app = getApp();

	let following = $state(false);
	let map = $state<OverviewMap>();
	/** How far the navigation reaches in from the bottom edge, measured in pixels. */
	let navigationHeight = $state(0);
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
		coveredBottom={navigationHeight}
		onproblem={(problem) =>
			(app.notice = { title: 'Position unavailable', lines: [problemText(problem)] })}
	/>
	<div class="navigation-height" bind:clientHeight={navigationHeight}></div>

	<div class="controls">
		<div class="zoom">
			<MapButton label="Zoom in" icon="plus" onclick={() => map?.zoomBy(1)} />
			<MapButton label="Zoom out" icon="minus" onclick={() => map?.zoomBy(-1)} />
		</div>
		<MapButton
			label="Show my position"
			icon="location"
			pressed={following}
			onclick={() => (following = !following)}
		/>
	</div>
</div>

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
	}
</style>
