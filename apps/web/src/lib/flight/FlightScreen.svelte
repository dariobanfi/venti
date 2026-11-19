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
	import ShareFlightSheet from '#lib/account/ShareFlightSheet.svelte';
	import { creditLine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { logbookHref } from '#lib/app/routes.ts';
	import type { FlightAnalysis, FlightView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Menu, { type MenuItem } from '#lib/design/Menu.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import MapButton from '#lib/map/MapButton.svelte';
	import FlightDetails from './FlightDetails.svelte';
	import FlightMap from './FlightMap.svelte';
	import FlightPanel from './FlightPanel.svelte';

	interface Props {
		flight: FlightView;
		/** What happened in the flight, once it has been worked out. */
		analysis: FlightAnalysis | undefined;
		/** Whether the screen is wide enough for the panel to stand beside the map. */
		wide: boolean;
		/** Hands the pilot the flight's file. */
		ondownload: () => void;
		/** Asks to delete the flight. The page confirms first. */
		ondelete: () => void;
	}

	let { flight, analysis, wide, ondownload, ondelete }: Props = $props();

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

	/** How many airspaces the map draws, or nothing while it is zoomed too far out for any. */
	let airspaceDrawn = $state<number>();
	/** Whose airspace is on the map. Their licences ask for this wherever it is shown. */
	const airspaceCredit = $derived(
		airspaceDrawn === undefined ? '' : creditLine(app.airspace?.credits ?? [], text)
	);

	/** What can be done with the flight besides looking at it. */
	const actions: MenuItem[] = $derived([
		{ label: 'Share with a link', icon: 'link', onselect: () => (sharing = true) },
		{ label: 'Download flight file', icon: 'download', onselect: ondownload },
		{ label: 'Delete flight', icon: 'trash', destructive: true, onselect: ondelete }
	]);

	/** The position in the track the pilot is inspecting, if any. */
	let cursor = $state<number>();
	/** Whether the marker is moving through the flight by itself. */
	let playing = $state(false);
	let showingDetails = $state(false);
	/** Whether the sheet that says who can see the flight is open. */
	let sharing = $state(false);

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
	let map = $state<FlightMap>();

	/** How much of the map the top bar and the panel cover, measured in pixels. */
	let barHeight = $state(0);
	let dockHeight = $state(0);
	let dock: HTMLDivElement;

	/** Shows a thermal or glide the pilot picked from the list. */
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
			airspace={app.airspace}
			onairspace={(count) => (airspaceDrawn = count)}
		/>
		<!-- A map says nothing to a screen reader: this much it can be told. -->
		{#if airspaceDrawn !== undefined && !app.airspace?.isEmpty}
			<p class="visually-hidden">
				Airspace on this part of the map, as it is today: {text.count(airspaceDrawn)}.
			</p>
		{/if}
		{#if airspaceCredit}
			<p class="credits">{airspaceCredit}</p>
		{/if}

		<div class="bar" bind:clientHeight={barHeight}>
			<MapButton label="Logbook" icon="back" href={logbookHref()} />
			<Menu label="More" items={actions} floating />
		</div>

		<div class="controls">
			<MapButton
				label="Terrain in 3D"
				icon="mountain"
				pressed={terrain3d}
				onclick={() => (terrain3d = !terrain3d)}
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
				<a class="back" href={logbookHref()}>
					<Icon name="back" />
					Logbook
				</a>
				<Menu label="More" items={actions} />
			</div>
			<div class="summary">
				<FlightPanel
					{flight}
					bind:cursor
					bind:playing
					ondetails={wide ? undefined : () => (showingDetails = true)}
				/>
			</div>
			{#if wide}
				<FlightDetails {flight} {analysis} onselect={mark} />
			{/if}
		</div>
	</div>
</div>

{#if !wide}
	<Sheet open={showingDetails} title="Flight details" onclose={() => (showingDetails = false)}>
		<FlightDetails {flight} {analysis} onselect={mark} />
	</Sheet>
{/if}

<ShareFlightSheet open={sharing} flight={flight.id} onclose={() => (sharing = false)} />

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
		background: rgb(255 255 255 / 0.78);
		-webkit-backdrop-filter: blur(0.75rem);
		backdrop-filter: blur(0.75rem);
		/* The map is light in both appearances, and so is what is written on it. */
		color: #3a3a3c;
		font-size: var(--caption);
		line-height: 1.3;
	}

	.wide .credits {
		bottom: 2.375rem;
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

	.wide .controls {
		top: 1rem;
		right: 1rem;
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
		color: var(--accent);
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
