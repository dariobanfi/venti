<!--
	Today: the take-offs around a place, and what the forecast says about
	flying from each, as the design pack's Today screen and the iPhone's.

	The head is the pack's valley strip with the day and, in the hero style,
	the day's line of the take-off that looks best. The best take-off is a
	card with its hours; the others are rows. A tap leads to the take-off's
	page. The three words are what the forecast says by fixed limits; the
	core decides them (`site_outlook`), and the screen says that they are
	not advice to fly.

	Which take-offs are listed, their order on a day and which is the best
	are the core's too (`Core.todayTakeoffs`, `todayOrder`): the pilot's own
	first, then those around the place. Where there are both, each group
	stands under the core's name for it: two lists on a phone, two groups
	of rows in the table of a wide screen.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { placeHref } from '#lib/app/routes.ts';
	import type { OutlookDay, OutlookHour } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';
	import { type Fix, problemText, watchPosition } from '#lib/map/position.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import ThermalLine from '#lib/today/ThermalLine.svelte';
	import ThermalMark from '#lib/today/ThermalMark.svelte';
	import { thermalStep } from '#lib/design/palette.ts';
	import { FIRST_REGION, NOTHING_YET, type TodayPlace } from '#lib/today/model.svelte.ts';
	import { arranged, firstDay } from '#lib/today/order.ts';

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
		// Asked anew after the pilot kept or forgot a take-off.
		today?.show(position ?? FIRST_REGION, position !== undefined, app.kept?.revision ?? 0);
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

	/** The days to choose from, by the first take-off that has a forecast: four at most. */
	const days = $derived(
		shown.places.find((place) => place.outlook)?.outlook?.days.slice(0, 4) ?? []
	);
	/** As the core names them by the calendar: "Today", "Tomorrow", "Sat". */
	const dayNames = $derived(days.map((day) => day.name));
	/** The take-offs as the core orders them on the day, and the one it calls the best. */
	const order = $derived(arranged(shown.places, shown.orders[dayIndex], dayIndex));
	const best = $derived(order.best);
	/** Every take-off in the core's order, for the table of a wide screen. */
	const listed = $derived(order.listed);
	/** The take-off chosen in that table, by its identifier; the best until one is. */
	let chosenSite = $state<string>();
	/** The take-off whose hours the card shows: the one chosen on a wide screen, else the best. */
	const card = $derived.by(() => {
		const place = wide.current ? shown.places.find((one) => one.site.id === chosenSite) : undefined;
		const day = place?.outlook?.days[dayIndex];
		return place && day ? { place, day } : best;
	});
	/** The others, in the core's order. */
	const others = $derived(listed.filter((place) => place !== best?.place));
	/** Whether the two groups are named: where the pilot's own and the others both have a take-off. */
	const grouped = $derived(
		shown.places.some((place) => place.isKept) && shown.places.some((place) => !place.isKept)
	);

	/** Take-offs under the core's name for their group; one group without a name where there is one kind only. */
	function groups(places: TodayPlace[]): { title: string | undefined; places: TodayPlace[] }[] {
		if (!grouped) return [{ title: undefined, places }];
		return [
			{ title: shown.keptTitle, places: places.filter((place) => place.isKept) },
			{ title: shown.nearTitle, places: places.filter((place) => !place.isKept) }
		].filter((group) => group.places.length > 0);
	}
	/**
	 * Whether the best take-off's hours show their climbs: only where one of
	 * them has something to climb in. A day with none says so in its line.
	 */
	const climbs = $derived(card?.day.hours.some((hour) => (hour.climbMs ?? 0) > 0) ?? false);
	/** Whose forecast it is, and of when: from any take-off that has one. */
	const outlook = $derived(shown.places.find((place) => place.outlook)?.outlook);
	/**
	 * Which model the day shown is read from, its run and what it is: the
	 * best take-off's day's own, since a day after the fine model's run
	 * ends is the coarser one's; the outlook's where the day has none.
	 */
	const reading = $derived.by(() => {
		const day = best?.day;
		return {
			note: day?.note || outlook?.note || '',
			model: day?.model || outlook?.model || '',
			runUnixS: day?.model ? day.runUnixS : (outlook?.runUnixS ?? 0)
		};
	});
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

	/** An hour for a screen reader: its status, its wind and its thermals. */
	function spoken(hour: OutlookHour): string {
		const verdict =
			hour.verdict === 'flyable'
				? 'Flyable'
				: hour.verdict === 'marginal'
					? `Marginal: ${hour.why}`
					: `Not flyable: ${hour.why}`;
		if (hour.climbMs === undefined) return verdict;
		if (hour.climbMs <= 0) return `${verdict}, no thermals to climb in`;
		const top = hour.thermalTopM === undefined ? '' : ` to ${text.metres(hour.thermalTopM)} m`;
		return `${verdict}, thermals ${text.forecastClimb(hour.climbMs)} m/s${top}`;
	}

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
		{#if best?.day.thermalWords}
			<p class="kind">{best.day.thermalWords}</p>
		{/if}
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
					{shown.nearTitle} of {shown.nearYou ? 'your position' : 'this place'} the flying sites’ source
					knows no take-off, and none is kept as yours.
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
				{#if wide.current}
					<!-- Every take-off as a table; a row chosen fills the card beside it. -->
					<div class="card table">
						<table aria-label="Take-offs">
							<thead>
								<tr>
									<th scope="col">Take-off</th>
									<th scope="col">Kind of day</th>
									<th scope="col">Status</th>
									<th scope="col">Wind</th>
									<th scope="col" class="number">Away</th>
								</tr>
							</thead>
							{#each groups(listed) as group (group.title)}
								<tbody>
									{#if group.title}
										<tr class="group-head">
											<th scope="rowgroup" colspan="5">{group.title}</th>
										</tr>
									{/if}
									{#each group.places as place (place.site.id)}
										{@const day = place.outlook?.days[dayIndex]}
										{@const current = card?.place === place}
										<tr class:current onclick={() => (chosenSite = place.site.id)}>
											<th scope="row">
												<button
													type="button"
													aria-pressed={current}
													onclick={() => (chosenSite = place.site.id)}
												>
													{place.site.name}
												</button>
											</th>
											<td>
												{#if day?.thermalWords}
													<span class="kind-of-day">
														<ThermalMark climbMs={day.climbMs ?? 0} />
														{day.thermalWords}
													</span>
												{/if}
											</td>
											<td class="status">
												{#if day}
													<StatusChip verdict={day.verdict} status={day.status} />
													{#if day.why}<span class="lesser">{day.why}</span>{/if}
												{:else}
													<span class="lesser">
														{place.problem ??
															(place.outlook
																? 'No forecast for this day'
																: 'Fetching the forecast…')}
													</span>
												{/if}
											</td>
											<td class="numeric">
												{#if day?.windKmh}{day.windKmh} km/h{#if day.windFrom}{' '}from {day.windFrom}{/if}{/if}
											</td>
											<td class="number numeric">
												{#if place.distanceM !== undefined}{text.kilometres(place.distanceM)} km{/if}
											</td>
										</tr>
									{/each}
								</tbody>
							{/each}
						</table>
					</div>
				{/if}

				{#if card}
					{@const day = card.day}
					{@const href = placeHref(card.place.site.position, card.place.site.id)}
					<svelte:element
						this={wide.current ? 'section' : 'a'}
						class="card best"
						href={wide.current ? undefined : href}
						aria-label="{card.place.site.name}: {day.status}"
					>
						{#if grouped && !wide.current}
							<!-- Which of the two groups the best take-off is of: it stands apart from their lists. -->
							<p class="of-group">{card.place.isKept ? shown.keptTitle : shown.nearTitle}</p>
						{/if}
						<div class="title">
							<h2>
								{#if wide.current}
									<a {href}>{card.place.site.name}</a>
								{:else}
									{card.place.site.name}
								{/if}
							</h2>
							<StatusChip verdict={day.verdict} status={day.status} />
						</div>
						{#if day.why}
							<p class="why">{day.why}</p>
						{/if}
						{#if day.thermals}
							<ThermalLine {day} />
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
										{#if climbs}
											<span class="climb">
												{hour.climbMs === undefined ? '–' : text.forecastClimb(hour.climbMs)}
											</span>
											<span class="bar thermal t{thermalStep(hour.climbMs ?? 0)}"></span>
										{/if}
										<span class="visually-hidden">{spoken(hour)}</span>
									</li>
								{/each}
							</ol>
							<p class="legend">
								Wind in km/h at 10 m; the bar is the hour’s status by its height.{#if climbs}{' '}Under
									it the climb to expect in thermals, in m/s, with a bar that grows with it.{/if}
							</p>
						{:else}
							<p class="why">No daylight hours left on this day.</p>
						{/if}
					</svelte:element>
				{:else if waiting}
					<p class="waiting card" role="status">
						Fetching the forecast for the take-offs. Venti fetches a new place’s forecast first,
						which takes a moment.
					</p>
				{/if}

				{#if !wide.current}
					{#each groups(others) as group (group.title)}
						<section>
							{#if group.title}
								<h2 class="group-title">{group.title}</h2>
							{/if}
							<ul class="card others" aria-label={group.title ?? 'Other take-offs'}>
								{#each group.places as place (place.site.id)}
									{@const day = place.outlook?.days[dayIndex]}
									<li>
										<a href={placeHref(place.site.position, place.site.id)}>
											<span class="sign"><Icon name="takeoff" /></span>
											<span class="text">
												<span class="name">{place.site.name}</span>
												{#if day?.thermalWords}
													<span class="kind-of-day">
														<ThermalMark climbMs={day.climbMs ?? 0} />
														{day.thermalWords}
													</span>
												{/if}
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
													{#if place.distanceM !== undefined}
														· {text.kilometres(place.distanceM)} km away
													{/if}
												</span>
											</span>
											{#if day}
												<StatusChip verdict={day.verdict} status={day.status} />
											{/if}
										</a>
									</li>
								{/each}
							</ul>
						</section>
					{/each}
				{/if}
			</div>

			{#if shown.keptMore}
				<p class="note">{shown.keptMore}</p>
			{/if}
			{#if shown.nearProblem}
				<p class="note" role="status">{shown.nearProblem}</p>
			{/if}

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
					<p>{reading.note}</p>
					<p class="numeric">
						{reading.model}, run of {text.day(reading.runUnixS)}
						{text.time(reading.runUnixS)}. Venti last looked for a newer run at {text.time(
							outlook.fetchedUnixS
						)}.
					</p>
					{#each outlook.credits as credit (credit.source)}
						<p><a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a></p>
					{/each}
				{/if}
				<!-- Whose the take-offs are, as their licences ask wherever they are shown. -->
				{#each shown.credits as credit (credit.source)}
					<p><a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a></p>
				{/each}
				<p>
					Flyable, marginal and not flyable are what the forecast says by fixed limits of wind,
					gusts, rain and cloud, and the wind directions the take-off’s source gives. The kind of
					day is what the model’s heat and temperatures say of thermals, by fixed limits too. They
					are not advice to fly: look at the place, the sky and the wind there.
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

	.day,
	.kind {
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

	/* Wide: every take-off as a table, and beside it the hours of the one
	   chosen, not a phone's column stretched. */
	.body.wide {
		max-width: 80rem;
	}

	.wide .columns {
		display: grid;
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		gap: var(--space-4);
		align-items: start;
	}

	.wide .best {
		padding: var(--space-4);
		gap: var(--space-3);
	}

	.table {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-m);
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	table th,
	table td {
		height: 2rem;
		padding: var(--space-1) var(--space-2);
		text-align: left;
		white-space: nowrap;
	}

	thead th {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
		box-shadow: inset 0 -1px 0 var(--color-border);
	}

	tbody tr:not(.group-head) {
		cursor: pointer;
	}

	tbody tr + tr > *,
	tbody + tbody tr:first-child > * {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	/* The name of a group of rows: the pilot's own, and those around. */
	.group-head th {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	tbody th button {
		font: inherit;
		font-weight: 600;
		color: inherit;
		text-align: left;
	}

	tr.current {
		background: var(--fill);
	}

	table .number {
		text-align: right;
	}

	/* What speaks against the day, under its status: the cell may wrap. */
	table td.status {
		white-space: normal;
		min-width: 12rem;
	}

	table td .lesser {
		display: block;
		font-size: var(--caption);
	}

	table :global(.status) {
		min-height: 1.5rem;
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

	h2 a {
		color: var(--link);
	}

	/* The name of a group over its list, as over a group of settings. */
	h2.group-title,
	.of-group {
		font-size: var(--subheadline);
		font-weight: 600;
		color: var(--text-secondary);
	}

	.note {
		margin-top: var(--space-4);
		padding: 0 var(--space-4);
		font-size: var(--footnote);
		color: var(--text-secondary);
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

	.climb {
		margin-top: var(--space-1);
		font-weight: 600;
	}

	/* The climb's bar: taller for a stronger step of the thermal scale, in its colour. */
	.bar.thermal {
		border-radius: 0.125rem;
	}

	.t1 {
		--tall: 0.17;
		background: var(--thermal-1);
	}

	.t2 {
		--tall: 0.33;
		background: var(--thermal-2);
	}

	.t3 {
		--tall: 0.5;
		background: var(--thermal-3);
	}

	.t4 {
		--tall: 0.67;
		background: var(--thermal-4);
	}

	.t5 {
		--tall: 0.83;
		background: var(--thermal-5);
	}

	.t6 {
		--tall: 1;
		background: var(--thermal-6);
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

	.text .lesser,
	.kind-of-day {
		font-size: var(--subheadline);
	}

	.kind-of-day {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	@media (hover: hover) {
		.others a:hover,
		a.best:hover {
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
