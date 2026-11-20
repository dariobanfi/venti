<!--
	Today: the take-offs around a place, and what the forecast says about
	flying from each, as the design pack's Today screen and the iPhone's.

	The head is the pack's valley strip with the day and, in the hero style,
	the day's line of the take-off that looks best. The best take-off is a
	card with its hours; the others are rows. A tap leads to the take-off's
	page. The three words are what the forecast says by fixed limits; the
	core decides them (`site_outlook`), and the screen says that they are
	not advice to fly.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { placeHref } from '#lib/app/routes.ts';
	import type { OutlookDay } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import { type Fix, problemText, watchPosition } from '#lib/map/position.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import { bestOn, FIRST_REGION, firstDay, NOTHING_YET } from '#lib/today/model.svelte.ts';

	const app = getApp();
	const text = format();
	const wide = new MediaQuery('min-width: 56rem', false);

	const today = $derived(app.today);
	const shown = $derived(today?.state ?? NOTHING_YET);

	/** The pilot's own position, once asked for and given. */
	let position = $state.raw<Fix>();
	let positionProblem = $state<string>();
	let locating = $state(false);

	$effect(() => {
		today?.show(position ?? FIRST_REGION, position !== undefined);
	});

	function usePosition() {
		locating = true;
		positionProblem = undefined;
		const stop = watchPosition(
			navigator.geolocation,
			(fix) => {
				stop();
				locating = false;
				position = fix;
			},
			(problem) => {
				locating = false;
				positionProblem = problemText(problem);
			}
		);
	}

	/** The day chosen; until the pilot chooses, the first with flying hours left. */
	let chosen = $state<number>();
	const dayIndex = $derived(chosen ?? firstDay(shown.places));

	/** The days to choose from, by the first take-off that has a forecast. */
	const days = $derived(
		shown.places.find((place) => place.outlook)?.outlook?.days.slice(0, 3) ?? []
	);
	/** As the core names them by the calendar: "Today", "Tomorrow", "Sat". */
	const dayNames = $derived(days.map((day) => day.name));
	const best = $derived(bestOn(shown.places, dayIndex));
	const others = $derived(shown.places.filter((place) => place !== best?.place));
	/** Whose forecast it is, and of when: from any take-off that has one. */
	const outlook = $derived(shown.places.find((place) => place.outlook)?.outlook);
	const waiting = $derived(
		shown.phase === 'loaded' && shown.places.some((place) => !place.outlook && !place.problem)
	);

	/** The day in the head: "Saturday, near you". */
	const dayLine = $derived.by(() => {
		const day = days[dayIndex];
		const name = text.weekday(day?.startUnixS ?? shown.nowUnixS ?? Date.now() / 1000);
		return `${name}, ${shown.nearYou ? 'near you' : 'around the Tegernsee'}`;
	});

	/** How tall an hour's bar stands: tall for flyable, low for not. */
	const BAR = { flyable: 1, marginal: 0.6, notFlyable: 0.25 } as const;

	function readings(day: OutlookDay): { value: string; unit: string; name: string }[] {
		return [
			{
				value: day.windKmh || '–',
				unit: day.windKmh ? 'km/h' : '',
				name: day.windFrom ? `Wind from ${day.windFrom}` : 'Wind'
			},
			{ value: day.gustKmh || '–', unit: day.gustKmh ? 'km/h' : '', name: 'Gusts' },
			{
				value: day.cloudBaseM === undefined ? '–' : text.metres(day.cloudBaseM),
				unit: day.cloudBaseM === undefined ? '' : 'm',
				name: 'Cloud base'
			}
		];
	}
</script>

<svelte:head>
	<title>Today · Venti</title>
</svelte:head>

<div class="page">
	<header class="valley">
		<p class="day">{dayLine}</p>
		<h1 class="hero">{best?.day.headline ?? 'Today'}</h1>
	</header>

	<div class="body" class:wide={wide.current}>
		{#if app.failure}
			<div class="nothing">
				<Icon name="warning" />
				<h2>Venti could not open its data</h2>
				<p>{app.failure}</p>
			</div>
		{:else if shown.phase === 'absent'}
			<div class="nothing">
				<Icon name="forecast" />
				<h2>Not connected to Venti</h2>
				<p>
					Today’s forecast for the take-offs comes from Venti, and this copy of the app is served
					without Venti behind it.
				</p>
			</div>
		{:else if shown.phase === 'failed'}
			<div class="nothing">
				<Icon name="warning" />
				<h2>Today’s forecast could not be fetched</h2>
				<p>{shown.problem}</p>
				<button class="button-plain" type="button" onclick={() => today?.again()}>
					Try again
				</button>
			</div>
		{:else if shown.phase === 'loaded' && shown.places.length === 0}
			<div class="nothing">
				<Icon name="pin" />
				<h2>No take-offs near</h2>
				<p>
					The flying sites’ source knows no take-off within 30 km of {shown.nearYou
						? 'your position'
						: 'this place'}.
				</p>
			</div>
		{:else if shown.phase !== 'loaded'}
			<p class="waiting" role="status">Looking for the take-offs around…</p>
		{:else}
			{#if dayNames.length > 1}
				<div class="days">
					<Segmented
						label="Day"
						options={dayNames}
						bind:value={
							() => dayNames[dayIndex] ?? dayNames[0], (name) => (chosen = dayNames.indexOf(name))
						}
					/>
				</div>
			{/if}

			<div class="columns">
				{#if best}
					{@const day = best.day}
					<a
						class="card best"
						href={placeHref(best.place.site.position, best.place.site.id)}
						aria-label="{best.place.site.name}: {day.status}"
					>
						<div class="title">
							<h2>{best.place.site.name}</h2>
							<StatusChip verdict={day.verdict} status={day.status} />
						</div>
						{#if day.why}
							<p class="why">{day.why}</p>
						{/if}
						<dl class="readings">
							{#each readings(day) as reading (reading.name)}
								<div>
									<dt>{reading.name}</dt>
									<dd class="numeric">
										{reading.value}{#if reading.unit}<small>{reading.unit}</small>{/if}
									</dd>
								</div>
							{/each}
						</dl>
						{#if day.hours.length > 0}
							<ol class="hours numeric" aria-label="Hour by hour">
								{#each day.hours as hour (hour.timeUnixS)}
									<li>
										<span class="hour">{hour.hour}</span>
										{#if hour.windFromDeg !== undefined}
											<span
												class="arrow"
												style:transform="rotate({hour.windFromDeg + 180}deg)"
												aria-hidden="true"
											>
												<Icon name="arrowWind" />
											</span>
										{:else}
											<span class="arrow" aria-hidden="true"></span>
										{/if}
										<span class="speed">{hour.windKmh || '–'}</span>
										<span
											class="bar {hour.verdict}"
											style:--tall={BAR[hour.verdict]}
											title={hour.why || 'Flyable'}
										></span>
										<span class="visually-hidden">
											{hour.verdict === 'flyable'
												? 'Flyable'
												: hour.verdict === 'marginal'
													? `Marginal: ${hour.why}`
													: `Not flyable: ${hour.why}`}
										</span>
									</li>
								{/each}
							</ol>
							<p class="legend">
								Wind in km/h at 10 m; the bar is the hour’s status by its height.
							</p>
						{:else}
							<p class="why">No daylight hours left on this day.</p>
						{/if}
					</a>
				{:else if waiting}
					<p class="waiting card" role="status">
						Fetching the forecast for the take-offs. Venti fetches a new place’s forecast first,
						which takes a moment.
					</p>
				{/if}

				<ul class="card others" aria-label="Other take-offs">
					{#each others as place (place.site.id)}
						{@const day = place.outlook?.days[dayIndex]}
						<li>
							<a href={placeHref(place.site.position, place.site.id)}>
								<span class="sign"><Icon name="takeoff" /></span>
								<span class="text">
									<span class="name">{place.site.name}</span>
									<span class="lesser">
										{#if day}
											{day.why || day.headline}
										{:else if place.problem}
											{place.problem}
										{:else if place.outlook}
											No forecast for this day
										{:else}
											Fetching the forecast…
										{/if}
										· {text.kilometres(place.distanceM)} km away
									</span>
								</span>
								{#if day}
									<StatusChip verdict={day.verdict} status={day.status} />
								{/if}
							</a>
						</li>
					{/each}
				</ul>
			</div>

			{#if !shown.nearYou}
				<div class="locate">
					<button class="button-plain" type="button" disabled={locating} onclick={usePosition}>
						<Icon name="location" />
						{locating ? 'Finding your position…' : 'Use my position'}
					</button>
					{#if positionProblem}
						<p class="lesser" role="status">{positionProblem}</p>
					{/if}
				</div>
			{/if}

			<footer>
				{#if outlook}
					<p>{outlook.note}</p>
					<p class="numeric">
						{outlook.model}, run of {text.day(outlook.runUnixS)}
						{text.time(outlook.runUnixS)}. Venti last looked for a newer run at {text.time(
							outlook.fetchedUnixS
						)}.
					</p>
					{#each outlook.credits as credit (credit.source)}
						<p><a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a></p>
					{/each}
				{/if}
				<p>
					Flyable, marginal and not flyable are what the forecast says by fixed limits of wind,
					gusts, rain and cloud, and the wind directions the take-off’s source gives. They are not
					advice to fly: look at the place, the sky and the wind there.
				</p>
			</footer>
		{/if}
	</div>
</div>

<style>
	.page {
		min-height: 100dvh;
		padding-bottom: calc(1.5rem + var(--nav-bottom));
	}

	/* The pack's valley strip, behind the day and its line: art for an
	   arrival, with no data on it. The ink is the brand's on its light sky. */
	.valley {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-1);
		min-height: 9.5rem;
		padding: calc(var(--space-5) + var(--safe-top)) var(--gutter) var(--space-6);
		background: url('../../../../../design/assets/illustration/valley-strip.svg') center bottom /
			cover no-repeat;
		color: var(--brand-ink);
		text-align: center;
	}

	.day {
		font-size: var(--body);
		font-weight: 500;
	}

	/* The pack's hero style, in rem so that the reader's text size counts. */
	.hero {
		font-size: 2.25rem;
		font-style: italic;
		font-weight: 900;
		line-height: 1.05;
		letter-spacing: -0.01em;
	}

	.body {
		max-width: 64rem;
		margin: 0 auto;
		padding: var(--space-5) calc(var(--gutter) + var(--safe-right)) 0
			calc(var(--gutter) + var(--safe-left));
	}

	.days {
		max-width: 28rem;
		margin: 0 auto var(--space-5);
	}

	.columns {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}

	/* Wide: the best take-off beside the others, not a phone's column stretched. */
	.wide .columns {
		display: grid;
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		align-items: start;
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
	}

	.best {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-5);
	}

	.title {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--title);
	}

	.why,
	.lesser,
	.legend {
		color: var(--text-secondary);
	}

	.legend {
		font-size: var(--caption);
	}

	.readings {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
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

	.hours {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(2rem, 1fr);
		gap: 0.125rem;
		overflow-x: auto;
		text-align: center;
		font-size: var(--subheadline);
	}

	.hours li {
		display: grid;
		justify-items: center;
		gap: 0.3125rem;
	}

	.hour {
		color: var(--text-secondary);
	}

	.arrow {
		display: grid;
		place-items: center;
		width: 1.125rem;
		height: 1.125rem;
	}

	.arrow :global(svg) {
		width: 1.125rem;
		height: 1.125rem;
	}

	.speed {
		font-weight: 600;
	}

	/* The hour's status by the bar's height as well as its colour. */
	.bar {
		align-self: end;
		width: 100%;
		height: calc(1.5rem * var(--tall));
		margin-top: calc(1.5rem * (1 - var(--tall)));
		border-radius: 0.25rem;
	}

	.bar.flyable {
		background: var(--color-good);
	}

	.bar.marginal {
		background: var(--color-caution);
	}

	.bar.notFlyable {
		background: var(--color-danger);
	}

	.others li + li {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.others a {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: 4rem;
		padding: var(--space-2) var(--space-4);
	}

	.sign {
		flex: none;
		color: var(--text-secondary);
	}

	.text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.text .lesser {
		font-size: var(--subheadline);
	}

	@media (hover: hover) {
		.others a:hover,
		.best:hover {
			background: var(--fill);
		}
	}

	.locate {
		display: flex;
		flex-direction: column;
		align-items: center;
		margin-top: var(--space-5);
	}

	.locate button {
		gap: var(--space-2);
	}

	footer {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-6);
		padding: 0 var(--space-4);
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	footer a {
		color: var(--link);
	}

	.waiting {
		padding: var(--space-5);
		text-align: center;
		color: var(--text-secondary);
	}

	.nothing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		max-width: 24rem;
		margin: var(--space-7) auto;
		text-align: center;
		color: var(--text-secondary);
		font-size: 2.25rem;
	}

	.nothing h2 {
		color: var(--text);
	}

	.nothing p {
		font-size: var(--body);
	}

	.nothing button {
		font-size: var(--body);
	}
</style>
