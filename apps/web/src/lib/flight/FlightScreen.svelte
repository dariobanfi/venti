<!--
	One flight: its track on the map, coloured by climb, with a panel of its
	numbers and altitude profile.

	On a narrow screen the map fills the screen and the panel floats over its
	lower part, as on the iPhone. On a wide one the map has the left two
	thirds with the altitude profile under it across their width, and a
	column at the right holds the flight's numbers with everything else
	known about it under them. Either way the map is told how much of it is covered, and fits the
	track into the part that shows.
-->
<script lang="ts">
	import { type Snippet, untrack } from 'svelte';
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
	import { aerialOn, keepAerial } from '#lib/map/layers.ts';
	import MapButton from '#lib/map/MapButton.svelte';
	import { type Beside, otherAt } from './beside';
	import { nearestIndex } from './chart';
	import ComparisonSheet from './ComparisonSheet.svelte';
	import FlightChart from './FlightChart.svelte';
	import FlightDetails from './FlightDetails.svelte';
	import FlightMap from './FlightMap.svelte';
	import FlightPanel from './FlightPanel.svelte';
	import { FIRST_SPEED, type ReplaySpeed, replayed } from './glider';

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
		/** Whose flight it is, for another pilot's: the name, and the way to that pilot's flights. */
		whose?: { name: string; href: string; picture?: string };
		/**
		 * The site it took off from, where one is known: its name, and the
		 * site's identifier where the server named it.
		 */
		takeoffSite?: { name: string; id?: string };
		/** A second flight laid beside this one. */
		beside?: Beside;
		/** Takes the second flight away again. */
		onendbeside?: () => void;
		/**
		 * What other pilots said of a public flight, its cheers and comments:
		 * a section of the column beside the flight on a wide screen, a
		 * sheet on a narrow one, opened by a row of the panel that reads
		 * `socialLine` and titled `socialTitle`.
		 */
		social?: Snippet;
		socialLine?: string;
		socialTitle?: string;
		/**
		 * Checks the flight against the airspace the app has: for a flight
		 * of the pilot's own, from its file. Another pilot's is not checked.
		 */
		checkAirspace?: () => Promise<FlightAirspace>;
		/** Hands the flight to a league: for a flight of the pilot's own. */
		onleague?: () => void;
	}

	let {
		flight,
		analysis,
		wide,
		back,
		actions,
		whose,
		takeoffSite,
		beside,
		onendbeside,
		social,
		socialLine = '',
		socialTitle = '',
		checkAirspace,
		onleague
	}: Props = $props();

	/** Whether the sheet of cheers and comments is open, on a narrow screen. */
	let showingSocial = $state(false);

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
		if (!checkAirspace || airspace?.revision === undefined) return;
		// Only the answer to the latest question counts.
		let current = true;
		checkAirspace().then(
			(checked) => {
				if (current) airspaceCheck = checked;
			},
			// Still fetching some of the airspace: asked again when it is there.
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
		app.account?.hasServer && analysis?.takeoff
			? placeHref(analysis.takeoff.position, takeoffSite?.id ?? '')
			: undefined
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

	/** How fast the flight plays, times its own time: the pilot's choice. */
	let speed = $state<ReplaySpeed>(FIRST_SPEED);

	// Playing moves the marker from where it is to the end of the flight at
	// the speed chosen, by the flight's own clock: a step each fifteenth of
	// a second, as far as the time that passed takes it.
	$effect(() => {
		if (!playing) return;
		const track = flight.track;
		const last = track.length - 1;
		const times = speed;
		if (last < 1) {
			playing = false;
			return;
		}
		// From the end of the flight, playing starts over.
		let playhead = untrack(() => {
			if ((cursor ?? 0) >= last) cursor = 0;
			return track[cursor ?? 0].seconds;
		});
		let shown = untrack(() => cursor ?? 0);
		let before = performance.now();
		const timer = setInterval(() => {
			const now = performance.now();
			// The pilot may drag the marker elsewhere while it plays: it goes on from there.
			if (cursor !== undefined && cursor !== shown) playhead = track[cursor].seconds;
			const next = replayed(track, playhead, (now - before) / 1000, times);
			before = now;
			playhead = next.playhead;
			shown = next.index;
			cursor = next.index;
			if (next.index >= last) playing = false;
		}, 66);
		return () => clearInterval(timer);
	});

	// Pressing Play in 3D brings the camera behind the glider, to follow it.
	$effect(() => {
		if (!playing || !terrain3d) return;
		untrack(() => {
			follow = true;
			map?.chase();
		});
	});
	let terrain3d = $state(false);
	/**
	 * Whether the camera chases the glider from behind and above, in 3D:
	 * from the start, and again whenever Play is pressed, so that the
	 * glider stays in frame while the flight plays. Moving the map by hand
	 * lets go; "Follow the glider" comes back.
	 */
	let follow = $state(true);
	/** Whether the ground is photographs from above: the pilot's choice for every map. */
	let aerial = $state(aerialOn(globalThis.localStorage));
	/** Whose photographs can show where the map has come to rest, as the map says. */
	let photos = $state('');
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
			onletgo={() => (follow = false)}
			onphotos={(line) => (photos = line)}
		/>
		<!-- A map says nothing to a screen reader: this much it can be told. -->
		{#if airspaceDrawn !== undefined && !app.airspace?.isEmpty}
			<p class="visually-hidden">
				Airspace on this part of the map, as it is today: {text.count(airspaceDrawn)}.
			</p>
		{/if}
		<!-- Whose the photographs are, while they show and the map's own
		     credits are folded behind their button. -->
		{#if airspaceCredit || (aerial && !wide && photos)}
			<p class="credits">
				{#if aerial && !wide && photos}<span>{photos}</span>{/if}
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
				text="3D"
				pressed={terrain3d}
				onclick={() => {
					terrain3d = !terrain3d;
					// Each time in 3D starts by following the glider.
					follow = true;
				}}
			/>
			<!-- In 3D: the whole flight from a good angle, and a camera that
			     follows the marked moment from behind. -->
			{#if terrain3d}
				<MapButton
					label="Whole flight"
					icon="track"
					onclick={() => {
						// The whole flight is a view of its own: the camera lets go of the glider.
						follow = false;
						map?.wholeFlight();
					}}
				/>
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

	{#if wide}
		<div class="under-map">
			<FlightChart
				{flight}
				{ground}
				bind:cursor
				{playing}
				{beside}
				onbeside={() => (sideBySide = true)}
			/>
		</div>
	{/if}

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
					bind:speed
					{whose}
					{beside}
					onbeside={() => (sideBySide = true)}
					score={analysis?.score}
					{onleague}
					ondetails={wide ? undefined : () => (showingDetails = true)}
					chart={!wide}
				/>
			</div>
			{#if social && wide}
				<div class="summary said">{@render social()}</div>
			{:else if social && socialTitle}
				<button class="said-row" type="button" onclick={() => (showingSocial = true)}>
					<span>{socialLine || socialTitle}</span>
					<Icon name="forward" />
				</button>
			{/if}
			{#if wide}
				<FlightDetails
					{flight}
					{analysis}
					airspace={airspaceCheck}
					onselect={mark}
					{fromHere}
					takeoffName={takeoffSite?.name}
				/>
			{/if}
		</div>
	</div>
</div>

{#if !wide}
	<Sheet open={showingDetails} title="Flight details" onclose={() => (showingDetails = false)}>
		<FlightDetails
			{flight}
			{analysis}
			airspace={airspaceCheck}
			onselect={mark}
			{fromHere}
			takeoffName={takeoffSite?.name}
		/>
	</Sheet>
{/if}

{#if social && !wide}
	<Sheet open={showingSocial} title={socialTitle} onclose={() => (showingSocial = false)}>
		<div class="said-sheet">{@render social()}</div>
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
		height: calc(100dvh - var(--nav-top));
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

	/* What other pilots said, on a narrow screen: one row under the numbers
	   that opens the sheet. */
	.said-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		width: 100%;
		min-height: var(--target);
		margin-top: var(--space-2);
		color: var(--link);
		font-size: var(--subheadline);
		text-align: left;
	}

	.said-row :global(svg) {
		flex: none;
		font-size: var(--caption);
		color: var(--icon-quiet);
	}

	.said-sheet {
		padding: var(--space-2) var(--space-4) var(--space-5);
	}

	/* Wide: the map at the left two thirds with the profile under it, the
	   numbers and the details in a column at the right. */
	.screen.wide {
		display: grid;
		grid-template-columns: minmax(0, 2fr) minmax(22rem, 1fr);
		grid-template-rows: minmax(0, 1fr) auto;
	}

	.wide .map-area {
		position: relative;
		inset: auto;
		grid-column: 1;
		grid-row: 1;
	}

	.under-map {
		grid-column: 1;
		grid-row: 2;
		padding: var(--space-3) var(--space-4);
		background: var(--surface);
		border-top: 1px solid var(--separator);
		--profile-height: 9rem;
	}

	.under-map :global(.chart-card) {
		padding: 0;
		border: 0;
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
		grid-column: 2;
		grid-row: 1 / 3;
		display: block;
		padding: 0;
		overflow-y: auto;
		background: var(--background);
		border-left: 0.5px solid var(--separator);
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
