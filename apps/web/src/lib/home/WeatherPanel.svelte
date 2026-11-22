<!--
	The weather on the home page: for the pilot's position when the browser
	gives it, else for the place this browser remembers (the position Today
	or this panel last had, or the take-off the pilot chose here), else for
	the region's middle. It shows the best take-off there as Today's core
	judges it (a chosen take-off itself): its verdict, the kind of day, the
	wind and the next hours, with the way to Today.

	It drives `App.today`, so the home page's take-offs are of the same
	place. The browser is asked for the position only when it has been
	allowed already, or when the pilot asks; a refusal is one line and a
	choice of place, not a dead end.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { mapHref, placeHref, todayHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import { type Fix, problemText, watchPosition } from '#lib/map/position.ts';
	import HourStrip from '#lib/today/HourStrip.svelte';
	import { FIRST_REGION, NOTHING_YET } from '#lib/today/model.svelte.ts';
	import { arranged, firstDay } from '#lib/today/order.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import ThermalMark from '#lib/today/ThermalMark.svelte';
	import HomePanel from './HomePanel.svelte';
	import { rememberedPlace, rememberPlace, type WeatherPlace } from './place.ts';

	interface Props {
		class?: string;
	}

	let { class: className = '' }: Props = $props();

	const app = getApp();
	const text = format();

	/** How many of the day's hours to come the strip shows. */
	const HOURS = 8;

	/** What a refusal says. */
	const REFUSED = 'Your browser does not give Venti your position.';

	/** The position of this visit, once the browser gave it. */
	let fix = $state.raw<Fix>();
	/** What this browser remembers from before. */
	let remembered = $state.raw<WeatherPlace | undefined>(browser ? rememberedPlace() : undefined);
	let locating = $state(false);
	/** Why there is no position, in a line. */
	let problem = $state<string>();
	const refused = $derived(problem !== undefined);

	const place = $derived<WeatherPlace | undefined>(
		fix ? { at: fix, kind: 'position' } : remembered
	);

	const today = $derived(app.today);
	const shown = $derived(today?.state ?? NOTHING_YET);
	$effect(() => {
		today?.show(place?.at ?? FIRST_REGION, place?.kind === 'position', app.kept?.revision ?? 0);
	});

	// Where the browser has been allowed to give the position, it is used
	// without asking; where it refused before, that is said at once.
	$effect(() => {
		let current = true;
		navigator.permissions
			?.query({ name: 'geolocation' })
			.then((status) => {
				if (!current) return;
				if (status.state === 'granted') locate();
				else if (status.state === 'denied') problem = REFUSED;
			})
			.catch(() => {});
		return () => {
			current = false;
		};
	});

	function locate() {
		locating = true;
		problem = undefined;
		const stop = watchPosition(
			navigator.geolocation,
			(found) => {
				stop();
				locating = false;
				fix = found;
				remembered = rememberPlace({ at: found, kind: 'position' });
			},
			(why) => {
				locating = false;
				problem = why === 'denied' ? REFUSED : problemText(why);
			}
		);
	}

	/** A take-off chosen from the list: the weather is for it from now on. */
	function choose(id: string) {
		const chosen = shown.places.find((one) => one.site.id === id);
		if (!chosen) return;
		fix = undefined;
		remembered = rememberPlace({
			at: chosen.site.position,
			kind: 'chosen',
			name: chosen.site.name
		});
	}

	const dayIndex = $derived(firstDay(shown.places));
	const order = $derived(arranged(shown.places, shown.orders[dayIndex], dayIndex));
	/** The take-off shown: the one chosen, else the core's best of the day. */
	const shownPlace = $derived.by(() => {
		if (place?.kind === 'chosen') {
			const chosen = shown.places.find((one) => one.site.name === place.name);
			const day = chosen?.outlook?.days[dayIndex];
			if (chosen && day) return { place: chosen, day };
		}
		return order.best;
	});
	const chosenId = $derived(
		place?.kind === 'chosen'
			? (shown.places.find((one) => one.site.name === place.name)?.site.id ?? '')
			: ''
	);
	const waiting = $derived(
		shown.phase === 'idle' ||
			shown.phase === 'loading' ||
			(!shownPlace && shown.places.some((one) => !one.outlook && !one.problem))
	);
	const failed = $derived(
		shown.phase === 'failed'
			? shown.problem
			: !shownPlace && !waiting
				? shown.places.find((one) => one.problem)?.problem
				: undefined
	);

	/** Where the weather is for, in a few words: "near you". */
	const where = $derived(
		place?.kind === 'chosen'
			? 'the take-off you chose'
			: place?.kind === 'position'
				? fix
					? 'near you'
					: 'near your last position'
				: 'around the Tegernsee'
	);
	const day = $derived(shownPlace?.day);
	const climbs = $derived(day?.hours.some((hour) => (hour.climbMs ?? 0) > 0) ?? false);
</script>

<HomePanel
	id="home-weather"
	title="Weather"
	class={className}
	more={{ href: todayHref(), words: 'Today' }}
	loading={waiting}
	problem={failed}
	absent={shown.phase === 'absent'}
	again={() => today?.again()}
>
	<p class="where">
		{day ? `${day.name}, ${where}` : `${where.charAt(0).toUpperCase()}${where.slice(1)}`}
	</p>
	{#if !shownPlace || !day}
		<p class="quiet">No take-offs near.</p>
	{:else}
		<div class="site">
			<a href={placeHref(shownPlace.place.site.position, shownPlace.place.site.id)}
				>{shownPlace.place.site.name}</a
			>
			<StatusChip verdict={day.verdict} status={day.status} />
		</div>
		{#if day.thermalWords || day.headline !== day.status}
			<p class="kind">
				{#if day.thermalWords}<ThermalMark climbMs={day.climbMs ?? 0} />
					<span>{day.thermalWords}</span>{/if}
				{#if day.headline !== day.status}<span class="quiet">{day.headline}</span>{/if}
			</p>
		{/if}
		<dl class="readings">
			<div>
				<dt>{day.windFrom ? `Wind from ${day.windFrom}` : 'Wind'}</dt>
				<dd class="numeric">
					{day.windKmh || '–'}{#if day.windKmh}<small>km/h</small>{/if}
				</dd>
			</div>
			<div>
				<dt>Gusts</dt>
				<dd class="numeric">
					{day.gustKmh || '–'}{#if day.gustKmh}<small>km/h</small>{/if}
				</dd>
			</div>
			<div>
				<dt>Cloud base</dt>
				<dd class="numeric">
					{day.cloudBaseM === undefined
						? '–'
						: text.metres(day.cloudBaseM)}{#if day.cloudBaseM !== undefined}<small>m</small>{/if}
				</dd>
			</div>
		</dl>
		{#if day.hours.length > 0}
			<HourStrip hours={day.hours.slice(0, HOURS)} {climbs} />
		{/if}
		<p class="credit">
			{#if app.said}{climbs
					? app.said.today.hourStripNoteWithThermals
					: app.said.today.hourStripNote}{/if}
			{day.model} of {text.time(day.runUnixS)}
		</p>
	{/if}

	<!-- Where the weather is for: the position, or a place to choose. -->
	{#if refused}
		<p class="refused">{problem}</p>
	{/if}
	{#if !fix}
		<div class="choose">
			{#if !refused}
				<button class="button-plain" type="button" disabled={locating} onclick={locate}>
					{locating ? 'Finding your position…' : 'Use my position'}
				</button>
			{/if}
			{#if shown.places.length > 0}
				<select
					aria-label="Choose a take-off"
					value={chosenId}
					onchange={(event) => choose(event.currentTarget.value)}
				>
					<option value="" disabled>Choose a take-off</option>
					{#each shown.places as one (one.site.id)}
						<option value={one.site.id}>{one.site.name}</option>
					{/each}
				</select>
			{/if}
			{#if refused}
				<a class="button-plain" href={mapHref(place?.at ?? FIRST_REGION)}>On the map</a>
			{/if}
		</div>
	{/if}
</HomePanel>

<style>
	.where {
		margin-top: calc(-1 * var(--space-2));
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.quiet {
		color: var(--text-secondary);
	}

	.site {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.site a {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--title);
		font-weight: 600;
	}

	.kind {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		font-weight: 500;
	}

	.readings {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
		margin: var(--space-1) 0;
	}

	.readings div {
		display: flex;
		flex-direction: column-reverse;
	}

	.readings dt {
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
	}

	.readings dd {
		font-size: var(--number);
		font-weight: 600;
		white-space: nowrap;
	}

	.readings small {
		margin-left: 0.2em;
		font-size: 0.55em;
		font-weight: 500;
		color: var(--text-secondary);
	}

	.credit {
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	.refused {
		margin-top: var(--space-2);
		padding-top: var(--space-2);
		box-shadow: inset 0 1px 0 var(--separator);
		color: var(--text-secondary);
	}

	.choose {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}

	.refused + .choose {
		margin-top: 0;
	}

	.choose:first-child,
	.credit + .choose {
		margin-top: var(--space-2);
		padding-top: var(--space-2);
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.choose .button-plain {
		margin-left: calc(-1 * var(--space-3));
	}

	select {
		min-width: 0;
		max-width: 100%;
		min-height: var(--target);
		padding: 0 var(--space-3);
		border: 1px solid var(--color-border-strong);
		border-radius: var(--radius-control);
		background: var(--surface);
		color: var(--text);
		font: inherit;
	}

	@media (hover: hover) {
		.site a:hover {
			text-decoration: underline;
		}
	}
</style>
