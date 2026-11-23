<!--
	A small map for choosing where an open plan is: the pilot moves the map
	under a mark in its middle, as the map tab does for a new site, and the
	place under the mark is the plan's. `onmove` is told the middle each
	time the map comes to rest; `show` brings a place there, as when a
	take-off near is chosen.

	The map engine is loaded only when the form opens.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { Coordinate } from '#lib/core/index.ts';
	import type { Map } from '#lib/map/setup.ts';

	interface Props {
		/** Where the map opens. */
		start: Coordinate;
		/** What a screen reader calls the map. */
		label: string;
		/** Told the place under the mark whenever the map comes to rest. */
		onmove: (place: Coordinate) => void;
	}

	let { start, label, onmove }: Props = $props();

	let container: HTMLDivElement;
	let map: Map | undefined;

	onMount(() => {
		let gone = false;
		void import('#lib/map/setup.ts').then((module) => {
			if (gone) return;
			const at = start;
			map = module.createMap(container, label, { center: [at.lon, at.lat], zoom: 12 });
			const shown = map;
			shown.on('moveend', () => {
				const middle = shown.getCenter();
				onmove({ lat: middle.lat, lon: middle.lng });
			});
		});
		return () => {
			gone = true;
			map?.remove();
		};
	});

	/** Brings a place under the mark. */
	export function show(place: Coordinate): void {
		map?.jumpTo({ center: [place.lon, place.lat], zoom: Math.max(map.getZoom(), 13) });
	}
</script>

<div class="frame">
	<div class="map" bind:this={container}></div>
	<div class="mark" aria-hidden="true"></div>
</div>

<style>
	.frame {
		position: relative;
		height: 13rem;
		overflow: hidden;
		border-radius: var(--radius-card);
	}

	.map {
		position: absolute;
		inset: 0;
	}

	/* The mark of the place: a ring around a point, in the ink of what is
	   written on the map, as on the map tab. */
	.mark {
		position: absolute;
		top: 50%;
		left: 50%;
		width: 2.75rem;
		height: 2.75rem;
		margin: -1.375rem 0 0 -1.375rem;
		border: 0.1875rem solid var(--on-map-ink);
		border-radius: 50%;
		box-shadow:
			0 0 0 0.0625rem var(--on-map-paper),
			inset 0 0 0 0.0625rem var(--on-map-paper);
		pointer-events: none;
	}

	.mark::after {
		content: '';
		position: absolute;
		top: 50%;
		left: 50%;
		width: 0.375rem;
		height: 0.375rem;
		margin: -0.1875rem 0 0 -0.1875rem;
		border-radius: 50%;
		background: var(--on-map-ink);
	}
</style>
