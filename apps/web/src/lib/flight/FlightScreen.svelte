<!--
	One flight: its track on the map, coloured by climb, with a panel of its
	numbers and altitude profile.

	On a narrow screen the map fills the screen and the panel floats over its
	lower part, as on the iPhone. On a wide one the panel stands beside the map
	with everything else known about the flight under it, and the map has the
	rest. Either way the map is told how much of it is covered, and fits the
	track into the part that shows.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { creditLine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { placeHref } from '#lib/app/routes.ts';
	import type {
		FlightAirspace,
		FlightAnalysis,
		FlightView,
		GroundUnderFlight
	} from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Menu, { type MenuItem } from '#lib/design/Menu.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { AERIAL_CREDIT, aerialOn, keepAerial } from '#lib/map/layers.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import { type Beside, otherAt } from './beside';
	import { nearestIndex } from './chart';
	import ComparisonSheet from './ComparisonSheet.svelte';
	import FlightDetails from './FlightDetails.svelte';
	import FlightMap from './FlightMap.svelte';
	import FlightPanel from './FlightPanel.svelte';

	interface Props {
		flight: FlightView;
		/** What happened in the flight, once it has been worked out. */
		analysis: FlightAnalysis | undefined;
		/** Whether the screen is wide enough for the panel to stand beside the map. */
		wide: boolean;
		/** Where the flight was opened from, to go back to. */
		back: { href: string; label: string };
		/** What can be done with the flight besides looking at it. */
		actions: MenuItem[];
		/**
		 * Whether the flight is in the pilot's logbook. One that is, is
		 * checked against the airspace the app has; another pilot's is not.
		 */
		inLogbook: boolean;
		/** Whose flight it is, for another pilot's: the name, and the way to that pilot's flights. */
		whose?: { name: string; href: string };
		/** A second flight laid beside this one. */
		beside?: Beside;
		/** Takes the second flight away again. */
		onendbeside?: () => void;
		/** Hands the flight to a league: for a flight of the pilot's own. */
		onleague?: () => void;
	}

	let {
		flight,
		analysis,
		wide,
		back,
		actions,
		inLogbook,
		whose,
		beside,
		onendbeside,
		onleague
	}: Props = $props();

	const app = getApp();
	const text = format();

	// The airspace around the flight is fetched from the server when the
	// core says it is time, and kept.
	$effect(() => {
		void app.airspace?.refresh({
			lat: (flight.bounds.south + flight.bounds.north) / 2,
			lon: (flight.bounds.west + flight.bounds.east) / 2
		});
	});

	/**
	 * The flight checked against the airspace the app has: asked for when
	 * the flight opens and again whenever that airspace changes, as when the
	 * server's arrives. Without it the details are shorter, not wrong.
	 */
	let airspaceCheck = $state.raw<FlightAirspace>();
	$effect(() => {
		const airspace = app.airspace;
		if (!inLogbook || airspace?.revision === undefined) return;
		// Only the answer to the latest question counts.
		let current = true;
		airspace.check(flight.id).then(
			(checked) => {
				if (current) airspaceCheck = checked;
			},
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/**
	 * The ground under the flight, for its profile: read from the map's
	 * elevation tiles, which are fetched when the flight opens. Until they
	 * are there, and where there are none, the profile is as it was.
	 */
	let ground = $state.raw<readonly GroundUnderFlight[]>([]);
	$effect(() => {
		ground = [];
		// Only the answer about this flight counts, and a flight that is
		// closed has no more tiles fetched for it.
		let current = true;
		const asked = app.groundUnder(flight);
		asked.ground.then(
			(found) => {
				if (current) ground = found;
			},
			() => {}
		);
		return () => {
			current = false;
			asked.letGo();
		};
	});

	/**
	 * The page of what else was flown from where this flight took off, on a
	 * site with a server to ask. The recording says where that was.
	 */
	const fromHere = $derived(
		app.account?.hasServer && analysis?.takeoff ? placeHref(analysis.takeoff.position) : undefined
	);

	/** How many airspaces the map draws, or nothing while it is zoomed too far out for any. */
	let airspaceDrawn = $state<number>();
	/** Whose airspace is on the map. Their licences ask for this wherever it is shown. */
	const airspaceCredit = $derived(
		airspaceDrawn === undefined ? '' : creditLine(app.airspace?.credits ?? [], text)
	);

	/** The position in the track the pilot is inspecting, if any. */
	let cursor = $state<number>();
	/** Whether the two flights are shown side by side in numbers. */
	let sideBySide = $state(false);
	/** The other flight on the map: its track, and its glider at the marked moment. */
	const other = $derived(
		beside
			? {
					track: beside.flight.track,
					at: cursor === undefined ? undefined : otherAt(beside, flight.track[cursor].seconds)
				}
			: undefined
	);
	/** Whether the marker is moving through the flight by itself. */
	let playing = $state(false);
	let showingDetails = $state(false);

	// Playing moves the marker from where it is to the end of the flight in
	// about half a minute, whatever the flight's length.
	$effect(() => {
		if (!playing) return;
		const last = flight.track.length - 1;
		if (last < 1) {
			playing = false;
			return;
		}
		const step = Math.max(1, Math.floor(flight.track.length / 450));
		// From the end of the flight, playing starts over.
		untrack(() => {
			if ((cursor ?? 0) >= last) cursor = 0;
		});
		// Each step starts from where the marker is, so the pilot can drag
		// it elsewhere while it plays.
		const timer = setInterval(() => {
			const next = Math.min((cursor ?? 0) + step, last);
			cursor = next;
			if (next >= last) playing = false;
		}, 66);
		return () => clearInterval(timer);
	});
	let terrain3d = $state(false);
	/** Whether the camera follows the marked moment from behind, in 3D. */
	let follow = $state(false);
	/** Whether the ground is photographs from above: the pilot's choice for every map. */
	let aerial = $state(aerialOn(globalThis.localStorage));
	let map = $state<FlightMap>();

	/** How much of the map the top bar and the panel cover, measured in pixels. */
	let barHeight = $state(0);
	let dockHeight = $state(0);
	let dock: HTMLDivElement;

	/** Shows a thermal, a glide or an entry into airspace the pilot picked from a list. */
	function mark(trackIndex: number) {
		playing = false;
		cursor = Math.min(trackIndex, flight.track.length - 1);
		showingDetails = false;
		// On a wide screen the list is further down the same column as the
		// profile. Bring the profile, where the mark now is, back into view.
		dock.scrollTo({ top: 0, behavior: 'smooth' });
	}

	// The sheet belongs to the narrow layout. If the window grows while it is
	// open, the details are on screen anyway.
	$effect(() => {
		if (wide) showingDetails = false;
	});
</script>

<div class="screen" class:wide>
	<div class="map-area" style:--map-covered-bottom="{wide ? 0 : dockHeight}px">
		<FlightMap
			bind:this={map}
			{flight}
			cursor={cursor === undefined ? undefined : flight.track[cursor]}
			score={analysis?.score}
			coveredTop={wide ? 0 : barHeight}
			coveredBottom={wide ? 0 : dockHeight}
			{terrain3d}
			{aerial}
			airspace={app.airspace}
			onairspace={(count) => (airspaceDrawn = count)}
			{other}
			{analysis}
			{ground}
			follow={terrain3d && follow}
		/>
		<!-- A map says nothing to a screen reader: this much it can be told. -->
		{#if airspaceDrawn !== undefined && !app.airspace?.isEmpty}
			<p class="visually-hidden">
				Airspace on this part of the map, as it is today: {text.count(airspaceDrawn)}.
			</p>
		{/if}
		<!-- Whose the photographs are, while they show and the map's own
		     credits are folded behind their button. -->
		{#if airspaceCredit || (aerial && !wide)}
			<p class="credits">
				{#if aerial && !wide}<span>{AERIAL_CREDIT}</span>{/if}
				{#if airspaceCredit}<span>{airspaceCredit}</span>{/if}
			</p>
		{/if}

		<div class="bar" bind:clientHeight={barHeight}>
			<MapButton label={back.label} icon="back" href={back.href} />
			<Menu label="More" items={actions} floating />
		</div>

		<div class="controls">
			<MapButton
				label="Terrain in 3D"
				icon="mountain"
				pressed={terrain3d}
				onclick={() => {
					terrain3d = !terrain3d;
					if (!terrain3d) follow = false;
				}}
			/>
			<!-- In 3D: the whole flight from a good angle, and a camera that
			     follows the marked moment from behind. -->
			{#if terrain3d}
				<MapButton label="Whole flight" icon="track" onclick={() => map?.wholeFlight()} />
				<MapButton
					label="Follow the glider"
					icon="glider"
					pressed={follow}
					onclick={() => (follow = !follow)}
				/>
			{/if}
			<MapButton
				label="Aerial view"
				icon="layers"
				pressed={aerial}
				onclick={() => {
					aerial = !aerial;
					keepAerial(globalThis.localStorage, aerial);
				}}
			/>
			<div class="zoom">
				<MapButton label="Zoom in" icon="plus" onclick={() => map?.zoomBy(1)} />
				<MapButton label="Zoom out" icon="minus" onclick={() => map?.zoomBy(-1)} />
			</div>
		</div>
	</div>

	<div class="dock" bind:this={dock} bind:clientHeight={dockHeight}>
		<div class="side" class:glass={!wide}>
			<div class="top">
				<a class="back" href={back.href}>
					<Icon name="back" />
					{back.label}
				</a>
				<Menu label="More" items={actions} />
			</div>
			<div class="summary">
				<FlightPanel
					{flight}
					{ground}
					bind:cursor
					bind:playing
					{whose}
					{beside}
					onbeside={() => (sideBySide = true)}
					score={analysis?.score}
					{onleague}
					ondetails={wide ? undefined : () => (showingDetails = true)}
				/>
			</div>
			{#if wide}
				<FlightDetails {flight} {analysis} airspace={airspaceCheck} onselect={mark} {fromHere} />
			{/if}
		</div>
	</div>
</div>

{#if !wide}
	<Sheet open={showingDetails} title="Flight details" onclose={() => (showingDetails = false)}>
		<FlightDetails {flight} {analysis} airspace={airspaceCheck} onselect={mark} {fromHere} />
	</Sheet>
{/if}

{#if beside}
	<ComparisonSheet
		open={sideBySide}
		{beside}
		onnearest={() => {
			const nearest = beside.comparison.nearest;
			if (nearest) mark(nearestIndex(flight.track, nearest.seconds));
			sideBySide = false;
		}}
		onend={() => {
			sideBySide = false;
			onendbeside?.();
		}}
		onclose={() => (sideBySide = false)}
	/>
{/if}

<style>
	/* Narrow: the map is the screen; bar, buttons and panel float on it. */
	.screen {
		position: relative;
		height: 100dvh;
		overflow: hidden;
	}

	.map-area {
		position: absolute;
		inset: 0;
	}

	/* Whose airspace is drawn, in the manner of the map's own credits and
	   directly above them. */
	.credits {
		position: absolute;
		left: calc(0.75rem + var(--safe-left));
		bottom: calc(var(--map-covered-bottom, 0px) + 2.875rem);
		max-width: calc(100% - 6rem);
		padding: 0.125rem 0.5rem;
		border-radius: 0.75rem;
		background: var(--on-map-paper);
		-webkit-backdrop-filter: blur(0.75rem);
		backdrop-filter: blur(0.75rem);
		/* The map is light in both appearances, and so is what is written on it. */
		color: var(--on-map-ink);
		font-size: var(--caption);
		line-height: 1.3;
	}

	.wide .credits {
		bottom: 2.375rem;
	}

	/* Each credit a line of its own. */
	.credits span {
		display: block;
	}

	.bar {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		display: flex;
		justify-content: space-between;
		padding: calc(0.75rem + var(--safe-top)) calc(0.75rem + var(--safe-right)) 0.75rem
			calc(0.75rem + var(--safe-left));
		pointer-events: none;
	}

	.controls {
		position: absolute;
		top: calc(4.5rem + var(--safe-top));
		right: calc(0.75rem + var(--safe-right));
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

	.dock {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		display: flex;
		justify-content: center;
		padding: 0.5rem calc(0.5rem + var(--safe-right)) calc(0.5rem + var(--safe-bottom))
			calc(0.5rem + var(--safe-left));
		/* The map shows through beside a panel that is narrower than the screen. */
		pointer-events: none;
	}

	.side {
		width: 100%;
		max-width: 34rem;
		padding: 1.125rem 1.125rem 1.25rem;
		border-radius: var(--radius-panel);
		pointer-events: auto;
		/* The grey text of everything in the panel, readable on glass. */
		--metric-secondary: var(--glass-text-secondary);
	}

	.top {
		display: none;
	}

	/* Wide: the panel is a column beside the map, with the details under it. */
	.screen.wide {
		display: grid;
		grid-template-columns: 26rem minmax(0, 1fr);
	}

	.wide .map-area {
		position: relative;
		inset: auto;
		grid-column: 2;
		grid-row: 1;
	}

	.wide .bar {
		display: none;
	}

	/* Compact on a wide screen: the 3D tools and the zoom close together. */
	.wide .controls {
		top: 1rem;
		right: 1rem;
		gap: var(--space-2);
	}

	.wide .zoom {
		display: flex;
	}

	.wide .dock {
		position: static;
		grid-column: 1;
		grid-row: 1;
		display: block;
		padding: 0;
		overflow-y: auto;
		background: var(--background);
		border-right: 0.5px solid var(--separator);
		pointer-events: auto;
	}

	.wide .side {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		max-width: none;
		padding: 1rem 1.5rem 2rem;
		border-radius: 0;
		--metric-secondary: var(--text-secondary);
	}

	.wide .top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin: 0 -0.5rem -0.75rem;
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		min-height: var(--target);
		padding: 0 0.75rem 0 0.25rem;
		border-radius: var(--radius-control);
		color: var(--link);
	}

	@media (hover: hover) {
		.back:hover {
			background: var(--fill);
		}
	}

	.wide .summary {
		padding: 1.25rem;
		border-radius: var(--radius-card);
		background: var(--surface);
		/* There is room for a taller profile here. */
		--profile-height: 11rem;
	}
</style>
