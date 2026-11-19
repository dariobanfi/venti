<!--
	The map of the map tab: terrain, every flight in the logbook as a line, and
	the pilot's own position when asked for.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { Coordinate } from '#lib/core/index.ts';
	import { ACCENT_ON_MAP } from '#lib/design/palette.ts';
	import { type Fix, type PositionProblem, watchPosition } from './position';
	import type { Map } from './setup';
	import { boundsAround, NOTHING, outlineLines, place } from './shapes';

	interface Props {
		/** The shapes of the logbook's flights. */
		outlines: Coordinate[][];
		/** Whether the map keeps the pilot's position in its middle. */
		following: boolean;
		/** How far the navigation reaches in from the bottom edge, in pixels. */
		coveredBottom: number;
		/** Called when the position cannot be shown. */
		onproblem: (problem: PositionProblem) => void;
	}

	let { outlines, following = $bindable(), coveredBottom, onproblem }: Props = $props();

	/** The northern Alps between Lake Constance and Salzburg. */
	const START = { center: [11.6, 47.45] as [number, number], zoom: 7.4 };
	/** How close the map goes to a position it is asked to show. */
	const POSITION_ZOOM = 12.5;

	let container: HTMLDivElement;
	let setup: typeof import('./setup') | undefined;
	/** The map, once its style is there to draw on. */
	let map = $state.raw<Map>();
	let lastFix: Fix | undefined;
	let stopWatching: (() => void) | undefined;

	onMount(() => {
		let gone = false;
		let created: Map | undefined;
		void import('./setup').then((module) => {
			if (gone) return;
			setup = module;
			created = module.createMap(container, 'Map', START);
			// Dragging the map away means the pilot wants to look elsewhere.
			created.on('dragstart', () => (following = false));
			const styled = created;
			module.whenStyled(styled, () => {
				addLayers(module, styled);
				map = styled;
			});
		});
		return () => {
			gone = true;
			stopWatching?.();
			created?.remove();
		};
	});

	function addLayers(module: typeof import('./setup'), map: Map) {
		map.addSource('logbook-tracks', { type: 'geojson', data: NOTHING });
		// A white edge keeps the lines readable over any ground colour.
		map.addLayer(module.line('logbook-tracks-edge', 'logbook-tracks', '#ffffff', 5));
		map.addLayer(module.line('logbook-tracks-line', 'logbook-tracks', ACCENT_ON_MAP, 2.5));

		map.addSource('position', { type: 'geojson', data: NOTHING });
		map.addLayer({
			id: 'position-halo',
			type: 'circle',
			source: 'position',
			paint: { 'circle-radius': 16, 'circle-color': ACCENT_ON_MAP, 'circle-opacity': 0.2 }
		});
		map.addLayer(module.dot('position-dot', 'position', 7, ACCENT_ON_MAP, '#ffffff'));
	}

	// Draws the flights when they change, and moves the camera to show them
	// all. A map that is following the pilot stays where it is.
	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'logbook-tracks')?.setData(outlineLines(outlines));
		const box = boundsAround(outlines);
		const covered = { top: 0, right: 0, bottom: coveredBottom, left: 0 };
		if (box && !lastFix) setup.showBounds(map, box, covered);
		else map.setPadding(covered);
	});

	// Starts and stops following. The position stays on the map once found;
	// following only decides whether the map moves with it.
	$effect(() => {
		if (!map || !following) return;
		if (lastFix) centre(map, lastFix);
		stopWatching ??= watchPosition(
			navigator.geolocation,
			(fix) => {
				lastFix = fix;
				if (!map) return;
				setup?.shapes(map, 'position')?.setData(place(fix));
				if (following) centre(map, fix);
			},
			(problem) => {
				stopWatching = undefined;
				following = false;
				onproblem(problem);
			}
		);
	});

	function centre(map: Map, fix: Fix) {
		map.easeTo({ center: [fix.lon, fix.lat], zoom: Math.max(map.getZoom(), POSITION_ZOOM) });
	}

	/** One step closer or further away, for the buttons of a wide screen. */
	export function zoomBy(steps: number) {
		map?.easeTo({ zoom: map.getZoom() + steps, duration: 250 });
	}
</script>

<div class="map" bind:this={container}></div>

<style>
	.map {
		position: absolute;
		inset: 0;
	}
</style>
