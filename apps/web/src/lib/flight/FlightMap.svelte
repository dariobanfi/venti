<!--
	The map of one flight: its track coloured by climb, where it started and
	ended, a marker at the moment the pilot is inspecting, and under it all
	the airspace the app knows there, as it is today.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { FlightAnalysis, GroundUnderFlight } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import { climbBand } from '#lib/design/palette.ts';
	import {
		type AirLabel,
		curtain,
		deepestGlide,
		frameAt,
		groundAtSeconds,
		headingAt,
		ribbon,
		rgba,
		thermalColumns,
		toLocal
	} from './air';
	import { AirLayer } from './AirLayer';
	import { bankAt, caughtUp, type Chase, chaseStep, wing } from './glider';
	import { type AirspaceForMaps, type AirspaceOnMap, drawAirspace } from '#lib/airspace/on-map.ts';
	import type { FlightScore, FlightView, TrackPoint } from '#lib/core/index.ts';
	import {
		ACCENT_ON_MAP,
		CLIMB_BANDS,
		GLIDER_ON_MAP,
		INK_ON_MAP,
		OTHER_ON_MAP,
		PAPER_ON_MAP,
		ROUTE_ON_MAP
	} from '#lib/design/palette.ts';
	import { aerialMakers, photosCredit } from '#lib/map/aerial-credit.ts';
	import { addLine } from '#lib/map/casing.ts';
	import type { Map } from '#lib/map/setup.ts';
	import { NOTHING, place, routeLines, trackByBand } from '#lib/map/shapes.ts';

	interface Props {
		flight: FlightView;
		/** The moment to mark, if any. */
		cursor: TrackPoint | undefined;
		/** How the flight scores, once that has been worked out: its route is drawn. */
		score: FlightScore | undefined;
		/** How far the screen's bars and the panel reach in from the top and bottom, in pixels. */
		coveredTop: number;
		coveredBottom: number;
		/** Whether the map is tilted to show the terrain in 3D. */
		terrain3d: boolean;
		/** Whether the ground is photographs from above instead of the drawn map. */
		aerial: boolean;
		/** Where the airspace to draw comes from, once the app has it. */
		airspace: AirspaceForMaps | undefined;
		/** Called with how many airspaces are drawn, or nothing while none can be. */
		onairspace: (count: number | undefined) => void;
		/** A second flight laid beside this one: its track, and where its glider was at the marked moment. */
		other?: { track: readonly TrackPoint[]; at: TrackPoint | undefined };
		/** What happened in the flight, for its thermals and glides in the air. */
		analysis?: FlightAnalysis;
		/** The ground under the flight, for the curtain and the marked moment's line. */
		ground?: readonly GroundUnderFlight[];
		/**
		 * Whether the camera chases the glider at the marked moment, in 3D:
		 * from behind and above it, smoothly.
		 */
		follow?: boolean;
		/**
		 * Called when the pilot moves the map by hand (drags, turns or tilts
		 * it): the camera then lets go of the glider.
		 */
		onletgo?: () => void;
		/**
		 * Called whenever the map has come to rest with the short line of
		 * whose photographs of the aerial view can show there (`aerial-credit.ts`).
		 */
		onphotos?: (line: string) => void;
	}

	let {
		flight,
		cursor,
		score,
		coveredTop,
		coveredBottom,
		terrain3d,
		aerial,
		airspace,
		onairspace,
		other,
		analysis,
		ground = [],
		follow = false,
		onletgo,
		onphotos
	}: Props = $props();

	let container: HTMLDivElement;
	let setup: typeof import('#lib/map/setup.ts') | undefined;
	/** The map, once its style is there to draw on. */
	let map = $state.raw<Map>();
	let tilted = false;
	/** A thermal's column: the climb's coral, see-through. */
	const THERMAL = rgba(CLIMB_BANDS[5].color, 0.45);
	let airspaceOnMap: AirspaceOnMap | undefined;

	onMount(() => {
		let gone = false;
		let created: Map | undefined;
		void import('#lib/map/setup.ts').then((module) => {
			if (gone) return;
			setup = module;
			const first = flight.track[0];
			created = module.createMap(container, 'Map of the flight', {
				center: first ? [first.lon, first.lat] : [11.6, 47.45],
				zoom: 10
			});
			const styled = created;
			module.whenStyled(styled, () => {
				// Airspace lies under everything of the flight.
				airspaceOnMap = drawAirspace(
					module,
					styled,
					() => airspace,
					(count) => onairspace(count)
				);
				addRoute(module, styled);
				// The other flight lies under this one, in one colour.
				styled.addSource('other-track', { type: 'geojson', data: NOTHING });
				addLine(styled, module.line('other-track-edge', 'other-track', INK_ON_MAP, 4.5));
				styled.addLayer(module.line('other-track-line', 'other-track', OTHER_ON_MAP, 2.5));
				addTrack(module, styled);
				addEnds(module, styled);
				styled.addSource('other-cursor', { type: 'geojson', data: NOTHING });
				styled.addLayer(
					module.dot('other-cursor-dot', 'other-cursor', 7, OTHER_ON_MAP, PAPER_ON_MAP)
				);
				styled.addSource('flight-cursor', { type: 'geojson', data: NOTHING });
				styled.addLayer(
					module.dot('flight-cursor-dot', 'flight-cursor', 8, ACCENT_ON_MAP, PAPER_ON_MAP)
				);
				map = styled;
				// The pilot takes the camera: a move with no event of the
				// pilot's behind it is the camera following, and keeps it.
				for (const moved of ['dragstart', 'rotatestart', 'pitchstart'] as const) {
					styled.on(moved, (event) => {
						if (event.originalEvent) onletgo?.();
					});
				}
				// While a finger or the mouse is down, the chasing camera holds
				// still, so that a drag can begin and take the camera.
				for (const down of ['mousedown', 'touchstart'] as const) {
					styled.on(down, () => (held = true));
				}
				for (const up of ['mouseup', 'touchend', 'touchcancel'] as const) {
					styled.on(up, () => {
						held = false;
						if (chased) chaseOn();
					});
				}
				// Whose photographs can show, read from the style the map loaded.
				const makers = aerialMakers(styled.getStyle());
				const tellPhotos = () => {
					const visible = styled.getBounds();
					const region = {
						south: visible.getSouth(),
						west: visible.getWest(),
						north: visible.getNorth(),
						east: visible.getEast()
					};
					onphotos?.(photosCredit(makers, region, styled.getZoom()));
				};
				tellPhotos();
				styled.on('moveend', tellPhotos);
			});
		});
		return () => {
			gone = true;
			created?.remove();
		};
	});

	/** The scored route goes under the track: a thin dashed line. */
	function addRoute(module: typeof import('#lib/map/setup.ts'), map: Map) {
		map.addSource('flight-route', { type: 'geojson', data: NOTHING });
		const route = module.line('flight-route-line', 'flight-route', ROUTE_ON_MAP, 1.5);
		addLine(map, { ...route, paint: { ...route.paint, 'line-dasharray': [4, 3] } });
	}

	function addTrack(module: typeof import('#lib/map/setup.ts'), map: Map) {
		map.addSource('flight-track', { type: 'geojson', data: trackByBand(flight.track) });
		// A dark casing keeps the track readable over any ground colour.
		// The line is the pack's chart line (2–2.5 px) and the casing about
		// 1.5 px each side of it (design/DESIGN.md, Maps).
		map.addLayer(module.line('flight-track-edge', 'flight-track', INK_ON_MAP, 5.5));
		// Sink first and climbs last, so that where the track crosses itself
		// the thermals stay on top.
		CLIMB_BANDS.forEach((band, index) => {
			map.addLayer({
				...module.line(`flight-track-${index}`, 'flight-track', band.color, 2.5),
				filter: ['==', ['get', 'band'], index]
			});
		});
	}

	/** A hollow dot where the recording starts and a solid one where it ends. */
	function addEnds(module: typeof import('#lib/map/setup.ts'), map: Map) {
		const [first, last] = [flight.track[0], flight.track.at(-1)];
		if (!first || !last) return;
		map.addSource('flight-start', { type: 'geojson', data: place(first) });
		map.addLayer(module.dot('flight-start-dot', 'flight-start', 5, PAPER_ON_MAP, INK_ON_MAP));
		map.addSource('flight-end', { type: 'geojson', data: place(last) });
		map.addLayer(module.dot('flight-end-dot', 'flight-end', 5, INK_ON_MAP, PAPER_ON_MAP));
	}

	// Fits the whole track into the part of the map that shows, again
	// whenever that part changes size.
	$effect(() => {
		if (!map || !setup) return;
		setup.showBounds(map, flight.bounds, {
			top: coveredTop,
			right: 0,
			bottom: coveredBottom,
			left: 0
		});
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'flight-route')?.setData(score ? routeLines(score) : NOTHING);
	});

	// Draws the airspace again when the app gets it, and when it changes.
	$effect(() => {
		if (!map || airspace?.revision === undefined) return;
		airspaceOnMap?.look();
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'flight-cursor')?.setData(cursor ? place(cursor) : NOTHING);
	});

	/** Kept apart from the marked moment, so that moving the marker does not move the map. */
	const otherTrack = $derived(other?.track);
	const otherMarked = $derived(other?.at);

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'other-cursor')?.setData(otherMarked ? place(otherMarked) : NOTHING);
	});

	$effect(() => {
		if (!map || !setup) return;
		setup.shapes(map, 'other-track')?.setData(otherTrack ? trackLine(otherTrack) : NOTHING);
		// Room for both tracks.
		if (otherTrack && otherTrack.length > 1) {
			const lats = otherTrack.map((point) => point.lat);
			const lons = otherTrack.map((point) => point.lon);
			setup.showBounds(
				map,
				{
					south: Math.min(flight.bounds.south, ...lats),
					west: Math.min(flight.bounds.west, ...lons),
					north: Math.max(flight.bounds.north, ...lats),
					east: Math.max(flight.bounds.east, ...lons)
				},
				{ top: untrack(() => coveredTop), right: 0, bottom: untrack(() => coveredBottom), left: 0 }
			);
		}
	});

	/** A track as one line, without its colours. */
	function trackLine(track: readonly TrackPoint[]) {
		return {
			type: 'FeatureCollection' as const,
			features: [
				{
					type: 'Feature' as const,
					properties: {},
					geometry: {
						type: 'LineString' as const,
						coordinates: track.map((point) => [point.lon, point.lat])
					}
				}
			]
		};
	}

	$effect(() => {
		if (!map || !setup || terrain3d === tilted) return;
		tilted = terrain3d;
		setup.setTerrain3d(map, terrain3d);
		// In 3D the track is drawn in the air: the one on the ground goes.
		for (const layer of map.getStyle().layers ?? []) {
			if (layer.id.startsWith('flight-track')) {
				map.setLayoutProperty(layer.id, 'visibility', terrain3d ? 'none' : 'visible');
			}
		}
	});

	/** The flight in the air, while the map is in 3D. */
	let air = $state.raw<AirLayer>();
	/** The words beside the flight in the air, with where they stand on the screen. */
	let labels = $state.raw<AirLabel[]>([]);
	let labelElements: (HTMLElement | undefined)[] = $state([]);
	let markerElement = $state<HTMLElement>();
	const text = format();

	/** The local frame of the flight: its first position. */
	const frame = $derived(frameAt(flight.track[0]?.lat ?? 0, flight.track[0]?.lon ?? 0));

	$effect(() => {
		if (!map || !terrain3d) return;
		const styled = map;
		const points = flight.track.map((point) =>
			toLocal(frame, point.lat, point.lon, point.altitudeM)
		);
		const colours = flight.track.map((point) => rgba(CLIMB_BANDS[climbBand(point.climbMs)].color));
		const groundAt = (index: number) => groundAtSeconds(ground, flight.track[index].seconds);
		const columns = thermalColumns(
			frame,
			flight.track,
			analysis?.thermals ?? [],
			(thermal) => `${text.climb(thermal.climbMs)} m/s · ${text.signedMetres(thermal.gainM)} m`
		);
		const sink = deepestGlide(
			frame,
			flight.track,
			analysis?.glides ?? [],
			(glide) => `${text.signedMetres(-glide.heightLostM)} m on one glide`
		);
		const layer = new AirLayer(
			{
				frame,
				track: ribbon(points, colours),
				curtain: curtain(points, groundAt, rgba(ACCENT_ON_MAP, 0.16)),
				columns: ribbon(
					columns.flatMap((column) => [column.bottom, column.top]).length > 0
						? columns.flatMap((column) => [column.bottom, column.top])
						: [],
					columns.flatMap(() => [THERMAL, THERMAL])
				),
				casing: rgba(INK_ON_MAP),
				wing: wing(rgba(GLIDER_ON_MAP), rgba(INK_ON_MAP))
			},
			(place) => {
				labels.forEach((label, index) => {
					const element = labelElements[index];
					const at = place(label.at);
					if (!element) return;
					element.hidden = !at;
					if (at) element.style.transform = `translate(${at.x}px, ${at.y}px)`;
				});
				if (markerElement) {
					const at = marked ? place(marked) : undefined;
					markerElement.hidden = !at;
					if (at) markerElement.style.transform = `translate(${at.x}px, ${at.y}px)`;
				}
			}
		);
		labels = [...columns.map((column) => column.label), ...(sink ? [sink] : [])];
		styled.addLayer(layer);
		air = layer;
		(globalThis as { ventiAir?: AirLayer }).ventiAir = layer;
		return () => {
			if (styled.getLayer(layer.id)) styled.removeLayer(layer.id);
			air = undefined;
			labels = [];
		};
	});

	/** The marked moment in the air, in the local frame. */
	const marked = $derived(
		cursor ? toLocal(frame, cursor.lat, cursor.lon, cursor.altitudeM) : undefined
	);

	// The marked moment: its line down to the ground, and the glider there,
	// pointed along the track and banked into its turn.
	$effect(() => {
		if (!air) return;
		const groundM = cursor ? groundAtSeconds(ground, cursor.seconds) : undefined;
		air.mark(
			marked && groundM !== undefined ? [marked, [marked[0], marked[1], groundM]] : undefined
		);
		const index = cursor ? Math.max(flight.track.indexOf(cursor), 0) : 0;
		air.glider(
			marked
				? {
						at: marked,
						headingDeg: headingAt(flight.track, index),
						bankDeg: bankAt(flight.track, index)
					}
				: undefined
		);
	});

	/*
	 * The chasing camera: behind the glider and above it, looking at it,
	 * catching up smoothly frame by frame and resting once it has. The map
	 * looks at a point at the glider's height (the map's centre is then not
	 * on the ground), so that the glider stays where the eye is.
	 */
	/** How far the camera leans back when it chases, and how close it comes when the flight plays. */
	const CHASE_PITCH = 68;
	const CHASE_ZOOM = 14.5;
	/** How many seconds of the flight the camera's way is taken over. */
	const COURSE_S = 60;
	/** Where the camera looks now, while it chases. */
	let chased: Chase | undefined;
	let chaseFrame = 0;
	let chasedAt = 0;
	/** A zoom the camera is on its way to, while it chases. */
	let zoomTarget: number | undefined;
	/** Whether a finger or the mouse is down on the map. */
	let held = false;

	function glider(): Chase | undefined {
		if (!cursor) return undefined;
		const index = Math.max(flight.track.indexOf(cursor), 0);
		return {
			lat: cursor.lat,
			lon: cursor.lon,
			altitudeM: cursor.altitudeM,
			// Along the course of the last minute, so that circling in a
			// thermal turns the glider and not the whole view.
			bearing: headingAt(flight.track, index, COURSE_S)
		};
	}

	function chaseOn() {
		if (chaseFrame) return;
		chasedAt = performance.now();
		chaseFrame = requestAnimationFrame(chaseFrom);
	}

	function chaseOff() {
		cancelAnimationFrame(chaseFrame);
		chaseFrame = 0;
		// The room the chase kept for the covered parts goes with it.
		if (chased) map?.setPadding({ top: 0, right: 0, bottom: 0, left: 0 });
		chased = undefined;
		zoomTarget = undefined;
	}

	function chaseFrom(now: number) {
		chaseFrame = 0;
		const target = glider();
		if (!map || !target || held) return;
		const seconds = Math.min((now - chasedAt) / 1000, 0.1);
		chasedAt = now;
		if (!chased) {
			map.setCenterClampedToGround(false);
			const centre = map.getCenter();
			chased = {
				lat: centre.lat,
				lon: centre.lng,
				altitudeM: map.getCenterElevation(),
				bearing: map.getBearing()
			};
		}
		chased = chaseStep(chased, target, seconds);
		const ease = 1 - Math.exp(-seconds / 0.4);
		const pitch = map.getPitch() + (CHASE_PITCH - map.getPitch()) * ease;
		let zoom: number | undefined;
		if (zoomTarget !== undefined) {
			zoom = map.getZoom() + (zoomTarget - map.getZoom()) * ease;
			if (Math.abs(zoomTarget - zoom) < 0.01) zoomTarget = undefined;
		}
		map.jumpTo({
			center: [chased.lon, chased.lat],
			// The glider in the middle of the part of the map that shows.
			padding: { top: coveredTop, right: 0, bottom: coveredBottom, left: 0 },
			elevation: chased.altitudeM,
			bearing: chased.bearing,
			pitch,
			zoom
		});
		// A map says nothing of its camera; the browser tests read it here.
		(globalThis as { ventiChase?: unknown }).ventiChase = {
			bearing: map.getBearing(),
			course: target.bearing,
			pitch: map.getPitch(),
			zoom: map.getZoom()
		};
		const resting =
			caughtUp(chased, target) && Math.abs(CHASE_PITCH - pitch) < 0.05 && zoomTarget === undefined;
		if (!resting) chaseFrame = requestAnimationFrame(chaseFrom);
	}

	// The camera chases while it follows in 3D, and again whenever the marked moment moves.
	$effect(() => {
		if (!map || !terrain3d || !follow) {
			untrack(chaseOff);
			return;
		}
		if (cursor) untrack(chaseOn);
	});

	// Flat again, the map's centre is on the ground again.
	$effect(() => {
		if (map && !terrain3d) map.setCenterClampedToGround(true);
	});

	$effect(() => () => chaseOff());

	/** The flight plays: the camera comes close behind the glider and follows it. */
	export function chase() {
		if (!map) return;
		zoomTarget = Math.max(map.getZoom(), CHASE_ZOOM);
		chaseOn();
	}

	$effect(() => {
		if (!map || !setup) return;
		setup.setAerial(map, aerial);
	});

	/** Shows the whole flight in 3D from a good angle: from the south, leaning back. */
	export function wholeFlight() {
		if (!map) return;
		chaseOff();
		map.setCenterClampedToGround(true);
		const { south, west, north, east } = flight.bounds;
		const camera = map.cameraForBounds(
			[
				[west, south],
				[east, north]
			],
			{
				padding: { top: coveredTop + 24, right: 24, bottom: coveredBottom + 24, left: 24 },
				bearing: -20
			}
		);
		if (!camera) return;
		// Leaning back, the far side of the flight moves up: a step further away keeps all of it.
		map.easeTo({ ...camera, zoom: (camera.zoom ?? map.getZoom()) - 0.7, pitch: 58, duration: 400 });
	}

	/** One step closer or further away, for the buttons of a wide screen. */
	export function zoomBy(steps: number) {
		if (!map) return;
		// While the camera chases, it takes the zoom along; else the map eases there.
		if (chased) {
			zoomTarget = (zoomTarget ?? map.getZoom()) + steps;
			chaseOn();
			return;
		}
		map.easeTo({ zoom: map.getZoom() + steps, duration: 250 });
	}
</script>

<div class="map" bind:this={container}></div>
<!-- The words beside the flight in the air, moved by the layer after each
     frame to where their places lie on the screen. -->
{#if air}
	<div class="air" aria-hidden="true">
		{#each labels as label, index (index)}
			<span class="label {label.kind}" bind:this={labelElements[index]} hidden>{label.text}</span>
		{/each}
		<span class="marker" bind:this={markerElement} hidden></span>
	</div>
{/if}

<style>
	.air {
		position: absolute;
		inset: 0;
		overflow: hidden;
		pointer-events: none;
	}

	.label {
		position: absolute;
		top: 0;
		left: 0;
		margin: -1.75rem 0 0 0.25rem;
		padding: 0.0625rem 0.375rem;
		border-radius: var(--radius-s);
		background: var(--on-map-paper);
		color: var(--on-map-ink);
		font-size: var(--caption);
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	/* A thermal's words carry the climb's coral as a mark, a glide's loss the sink's teal. */
	.thermal {
		box-shadow: inset 0.1875rem 0 0 var(--vario-6);
	}

	.sink {
		box-shadow: inset 0.1875rem 0 0 var(--vario-1);
	}

	/* The pilot under the wing that the layer draws: a small dot. */
	.marker {
		position: absolute;
		top: 0;
		left: 0;
		width: var(--space-2);
		height: var(--space-2);
		margin: calc(-1 * var(--space-1)) 0 0 calc(-1 * var(--space-1));
		border-radius: 50%;
		background: var(--on-map-ink);
		box-shadow: 0 0 0 0.1875rem var(--on-map-paper-solid);
	}

	.map {
		position: absolute;
		inset: 0;
	}
</style>
