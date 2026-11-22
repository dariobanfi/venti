<!--
	Home: what is going on, in the first region and on Venti. At the top
	what Venti stands for, in four lines, and the season's flights that
	stand out as large cards; then panels side by side on a wide screen, as
	a sports data site has them, one under the other on a phone: the
	weather for the pilot's place, today's take-offs there as the core
	judges them, who is in the air now, Venti's news, the top of the
	season's ranking, today's best flights (or the newest), what pilots
	added to the map, and, for a pilot who is signed in, what their alarms
	fired and what the pilots they follow flew.

	Every word about a verdict, a rule or a count is the core's; the
	panels' titles are the page's. The map, which was here, is at `/map`:
	an address of the map with a place (`/?lat=…&lon=…`) is sent there.

	What another pilot wrote, a name or a place, is set as text.
-->
<script lang="ts">
	import { MediaQuery } from 'svelte/reactivity';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { getApp } from '#lib/app/app.svelte.ts';
	import {
		alertsHref,
		everyoneHref,
		mapHref,
		newsHref,
		newsPostHref,
		pilotHref,
		placeHref,
		publicFlightHref,
		rankingHref,
		todayHref
	} from '#lib/app/routes.ts';
	import { queryOf } from '#lib/community/pages.ts';
	import PublicFlightRow from '#lib/community/PublicFlightRow.svelte';
	import { distance, shape } from '#lib/community/words.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { dayOf } from '#lib/groups/days.ts';
	import Achievements from '#lib/home/Achievements.svelte';
	import HomePanel from '#lib/home/HomePanel.svelte';
	import StandsFor from '#lib/home/StandsFor.svelte';
	import WeatherPanel from '#lib/home/WeatherPanel.svelte';
	import DraftMark from '#lib/news/DraftMark.svelte';
	import NewsPicture from '#lib/news/NewsPicture.svelte';
	import { postMoment, posts } from '#lib/news/posts.ts';
	import StatusChip from '#lib/today/StatusChip.svelte';
	import { NOTHING_YET } from '#lib/today/model.svelte.ts';
	import { arranged, firstDay } from '#lib/today/order.ts';

	const app = getApp();
	const text = format();

	// The map's old address: a shared link or a site's page sent the pilot
	// to the map around a place.
	if (browser && page.url.searchParams.has('lat') && page.url.searchParams.has('lon')) {
		void goto(`${mapHref()}${page.url.search}`, { replaceState: true });
	}

	const home = $derived(app.home);
	const todayDate = dayOf(new Date());
	$effect(() => home?.show(todayDate));

	/* Today: the core's take-offs where the weather panel looks, ordered by the core. */
	const today = $derived(app.today);
	const shown = $derived(today?.state ?? NOTHING_YET);
	const dayIndex = $derived(firstDay(shown.places));
	const dayName = $derived(
		shown.places.find((place) => place.outlook)?.outlook?.days[dayIndex]?.name ?? 'Today'
	);
	const order = $derived(arranged(shown.places, shown.orders[dayIndex], dayIndex));
	const takeoffs = $derived(order.listed.slice(0, 5));

	/* Alerts: what the pilot's alarms fired lately, signed in only. */
	const signedIn = $derived(Boolean(app.account?.signedIn));
	const fired = $derived(signedIn ? (app.alerts?.fired ?? []).slice(0, 3) : []);

	/* In the air: Venti's pilots, and the free-flight ones of other networks. */
	const live = $derived(home?.live);
	const freeFlight = $derived(
		live?.phase === 'loaded'
			? (live.value.others?.networks ?? []).flatMap((network) =>
					network.aircraft.filter((aircraft) => aircraft.isFreeFlight)
				)
			: []
	);
	const liveCredits = $derived(
		live?.phase === 'loaded' ? (live.value.others?.credits ?? []).map((credit) => credit.text) : []
	);

	/* Following: the newest flights of the pilots this pilot follows, signed in only. */
	const social = $derived(app.social);
	const own = $derived(signedIn ? app.account?.account?.handle : undefined);
	$effect(() => social?.showFeed(own));
	const feed = $derived(social?.feed);
	const followed = $derived((feed?.rows ?? []).slice(0, 5));

	/**
	 * A wide screen has the flights as tables; a phone has them as rows in
	 * the feed's manner, the row everyone's flights use there.
	 */
	const wide = new MediaQuery('min-width: 56rem', false);

	/** The newest news: the first large, with its picture, two more as lines. */
	const [newest, ...older] = posts.slice(0, 3);

	const todaysBest = everyoneHref(queryOf({ order: 'best', from: todayDate, to: todayDate }));
</script>

<svelte:head>
	<title>Venti</title>
</svelte:head>

<div class="desk home">
	<header class="page-head">
		<div>
			<h1>What is going on</h1>
			{#if browser}<p class="sub">{text.day(Date.now() / 1000)}</p>{/if}
		</div>
		<div class="actions">
			<a class="button-plain" href={todayHref()}>Today</a>
			<a class="button-prominent" href={mapHref()}>
				<Icon name="map" />
				<span>Open the map</span>
			</a>
		</div>
	</header>

	<div class="columns">
		<StandsFor class="span-12" />

		<Achievements
			class="span-12"
			shown={home?.achievements}
			again={() => home?.again('achievements', todayDate)}
		/>

		<WeatherPanel class="span-4" />

		<!-- Today's take-offs -->
		<HomePanel
			id="home-today"
			title={dayName === 'Today' ? 'Take-offs today' : `Take-offs, ${dayName}`}
			class="span-4"
			more={{ href: todayHref(), words: 'Today' }}
			loading={shown.phase === 'idle' || shown.phase === 'loading'}
			problem={shown.phase === 'failed' ? shown.problem : undefined}
			absent={shown.phase === 'absent'}
			again={() => today?.again()}
		>
			{#if takeoffs.length === 0}
				<p class="quiet">No take-offs around here.</p>
			{:else}
				<table class="rows fit takeoffs" aria-label="Take-offs">
					<tbody>
						{#each takeoffs as place (place.site.id)}
							{@const day = place.outlook?.days[dayIndex]}
							<tr>
								<th scope="row" class="name">
									<a href={placeHref(place.site.position, place.site.id)}>{place.site.name}</a>
								</th>
								<td class="verdict">
									{#if day}
										<StatusChip verdict={day.verdict} status={day.status} />
									{:else}
										<span class="quiet">
											{place.problem ??
												(place.outlook ? 'No forecast for this day' : 'Fetching the forecast…')}
										</span>
									{/if}
								</td>
								<td class="number wind">
									{#if day?.windKmh}{day.windKmh} km/h{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
				<!-- The best take-off's line, where it says more than its chip
				     does ("Good until 14:00"): the verdict alone is in every row. -->
				{#if order.best && order.best.day.headline !== order.best.day.status}
					<p class="line">{order.best.day.headline}</p>
				{/if}
			{/if}
		</HomePanel>

		<div class="span-4 stack">
			<!-- Who is in the air -->
			<HomePanel
				id="home-live"
				title="In the air now"
				more={{ href: mapHref(undefined, 'pilots'), words: 'On the map' }}
				loading={!live || live.phase === 'loading'}
				problem={live?.phase === 'failed' ? live.problem : undefined}
				absent={live?.phase === 'absent'}
				again={() => home?.again('live', todayDate)}
			>
				{#if live?.phase === 'loaded'}
					{@const venti = live.value.map.gliders}
					{#if live.value.others?.countWords}
						<p class="count">{live.value.others.countWords}</p>
					{:else if venti.length === 0}
						<p class="quiet">Nobody shows a flight in the air here now.</p>
					{/if}
					{#if venti.length > 0 || freeFlight.length > 0}
						<ul class="list">
							{#each venti.slice(0, 5) as glider (glider.id)}
								<li>
									<a href={pilotHref(glider.handle)}>{glider.name}</a>
									<span class="quiet numeric">{text.metres(glider.altitudeM)} m</span>
								</li>
							{/each}
							{#each freeFlight.slice(0, 5 - Math.min(5, venti.length)) as aircraft (aircraft.id)}
								<li>
									<span>{aircraft.name ?? aircraft.kindWords}</span>
									<span class="quiet">{aircraft.line}</span>
								</li>
							{/each}
						</ul>
					{/if}
					{#if liveCredits.length > 0}
						<p class="credit">{liveCredits.join(' · ')}</p>
					{/if}
				{/if}
			</HomePanel>
			{#if signedIn}
				<!-- What the pilot's alarms fired -->
				<HomePanel id="home-alerts" title="Your alarms" more={{ href: alertsHref(), words: 'All' }}>
					{#if fired.length === 0}
						<p class="quiet">Nothing fired lately.</p>
					{:else}
						<ul class="list fired">
							{#each fired as line (`${line.alertId}-${line.firedAtUnixS}`)}
								<li>
									<span class="strong">{line.title}</span>
									<span class="quiet">{line.line}</span>
								</li>
							{/each}
						</ul>
					{/if}
				</HomePanel>
			{/if}
		</div>

		<!-- Venti's news: posts the owner writes, kept in the repository. -->
		<HomePanel
			id="home-news"
			title="News"
			class="span-8"
			more={older.length > 0 ? { href: newsHref(), words: 'All news' } : undefined}
		>
			{#if !newest}
				<p class="quiet">No news yet.</p>
			{:else}
				<article class="lead">
					{#if newest.image}
						<a class="picture" href={newsPostHref(newest.slug)} tabindex="-1" aria-hidden="true">
							<NewsPicture svg={newest.image} alt={newest.imageAlt} />
						</a>
					{/if}
					<div class="lead-words">
						<p class="date">
							{text.day(postMoment(newest.date))}
							{#if newest.draft}<DraftMark />{/if}
						</p>
						<h3 class="post-title">
							<a href={newsPostHref(newest.slug)}>{newest.title}</a>
						</h3>
						{#if newest.summary}<p class="quiet">{newest.summary}</p>{/if}
					</div>
				</article>
				{#if older.length > 0}
					<ul class="list">
						{#each older as post (post.slug)}
							<li>
								<a href={newsPostHref(post.slug)}>{post.title}</a>
								<span class="quiet">{text.day(postMoment(post.date))}</span>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</HomePanel>

		<!-- The top of the season's ranking -->
		<HomePanel
			id="home-ranking"
			title={home?.ranking.phase === 'loaded' && home.ranking.value.season
				? `Ranking ${home.ranking.value.season}`
				: 'Ranking'}
			class="span-4"
			more={{ href: rankingHref(), words: 'The whole ranking' }}
			loading={!home || home.ranking.phase === 'loading'}
			problem={home?.ranking.phase === 'failed' ? home.ranking.problem : undefined}
			absent={home?.ranking.phase === 'absent'}
			again={() => home?.again('ranking', todayDate)}
		>
			{#if home?.ranking.phase === 'loaded'}
				{#if home.ranking.value.pilots.length === 0}
					<p class="quiet">Nobody is ranked in this season yet.</p>
				{:else}
					<table class="rows numeric fit" aria-label="Ranking">
						<tbody>
							{#each home.ranking.value.pilots as pilot (pilot.handle)}
								<tr>
									<td class="place">{pilot.place}</td>
									<th scope="row" class="name">
										<a href={pilotHref(pilot.handle)}>{pilot.name}</a>
									</th>
									<td class="quiet words">{pilot.counted}</td>
									<td class="number">{text.points(pilot.points)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			{/if}
		</HomePanel>

		<!-- For a signed-in pilot: what the pilots they follow flew, with the
		     core's lines of cheers and comments. -->
		{#if own && social?.words && feed}
			<HomePanel
				id="home-following"
				title={social.words.feedFollowing}
				class="span-12"
				more={{ href: everyoneHref('?feed=following'), words: 'All' }}
				loading={!feed.known && !feed.problem}
				problem={feed.problem}
				again={() => social?.againFeed()}
			>
				{#if followed.length === 0}
					<p class="quiet">{feed.emptyLine}</p>
				{:else if !wide.current}
					<!-- A phone: the feed's rows, the core's cheers and comments under each. -->
					<ul class="feed" aria-label={social.words.feedFollowing}>
						{#each followed as row (row.flight.id)}
							<PublicFlightRow
								flight={row.flight}
								named
								under={[row.cheersLine, row.commentsLine].filter(Boolean).join(' · ')}
							/>
						{/each}
					</ul>
				{:else}
					<div class="scroll">
						<table class="rows numeric" aria-label={social.words.feedFollowing}>
							<thead>
								<tr>
									<th scope="col">Pilot</th>
									<th scope="col">Take-off</th>
									<th scope="col" class="number">Distance</th>
									<th scope="col" class="number">Points</th>
									<th scope="col" class="number">Day</th>
									<th scope="col">{social.words.cheersTitle}</th>
									<th scope="col">{social.words.commentsTitle}</th>
								</tr>
							</thead>
							<tbody>
								{#each followed as row (row.flight.id)}
									{@const flight = row.flight}
									{@const triangle = shape(flight)}
									<tr>
										<th scope="row" class="name">
											<a href={pilotHref(flight.handle)}>{flight.pilot}</a>
										</th>
										<td class="words">
											{#if flight.takeoff}
												<a class="link" href={placeHref(flight.takeoff, flight.takeoffSiteId ?? '')}
													>{flight.takeoffName ??
														text.place(flight.takeoff.lat, flight.takeoff.lon)}</a
												>
											{:else if flight.takeoffName}
												{flight.takeoffName}
											{/if}
										</td>
										<td class="number">
											{#if triangle}
												<span class="shape" role="img" aria-label={triangle}
													><Icon name="triangle" /></span
												>
											{/if}
											<a class="link" href={publicFlightHref(flight.id)}
												>{distance(flight, text)} km</a
											>
										</td>
										<td class="number">{flight.score ? text.points(flight.score.points) : '–'}</td>
										<td class="number">{text.day(flight.startUnixS)}</td>
										<td class="words quiet">{row.cheersLine}</td>
										<td class="words quiet">{row.commentsLine}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</HomePanel>
		{/if}

		<!-- The day's best flights, or the newest -->
		<HomePanel
			id="home-flights"
			title={home?.flights.phase === 'loaded' && !home.flights.value.ofToday
				? 'Flown lately'
				: 'Best flights today'}
			class="span-12"
			more={home?.flights.phase === 'loaded' && home.flights.value.ofToday
				? { href: todaysBest, words: 'All of today' }
				: { href: everyoneHref(), words: app.said?.pages.everyonesFlights ?? '' }}
			loading={!home || home.flights.phase === 'loading'}
			problem={home?.flights.phase === 'failed' ? home.flights.problem : undefined}
			absent={home?.flights.phase === 'absent'}
			again={() => home?.again('flights', todayDate)}
		>
			{#if home?.flights.phase === 'loaded'}
				{@const flights = home.flights.value}
				{#if !flights.ofToday && flights.flights.length > 0}
					<p class="quiet">Nobody has shared a flight of today yet. The newest:</p>
				{/if}
				{#if flights.flights.length === 0}
					<p class="quiet">Nobody has shared a flight on Venti yet.</p>
				{:else if !wide.current}
					<ul class="feed" aria-label="Flights">
						{#each flights.flights as flight (flight.id)}
							<PublicFlightRow {flight} named />
						{/each}
					</ul>
				{:else}
					<div class="scroll">
						<table class="rows numeric" aria-label="Flights">
							<thead>
								<tr>
									<th scope="col">Pilot</th>
									<th scope="col">Take-off</th>
									<th scope="col" class="number">Distance</th>
									<th scope="col" class="number">Points</th>
									<th scope="col" class="number">{flights.ofToday ? 'Time' : 'Day'}</th>
								</tr>
							</thead>
							<tbody>
								{#each flights.flights as flight (flight.id)}
									{@const triangle = shape(flight)}
									<tr>
										<th scope="row" class="name">
											<a href={pilotHref(flight.handle)}>{flight.pilot}</a>
										</th>
										<td class="words">
											{#if flight.takeoff}
												<a class="link" href={placeHref(flight.takeoff, flight.takeoffSiteId ?? '')}
													>{flight.takeoffName ??
														text.place(flight.takeoff.lat, flight.takeoff.lon)}</a
												>
											{:else if flight.takeoffName}
												{flight.takeoffName}
											{/if}
										</td>
										<td class="number">
											{#if triangle}
												<span class="shape" role="img" aria-label={triangle}
													><Icon name="triangle" /></span
												>
											{/if}
											<a class="link" href={publicFlightHref(flight.id)}
												>{distance(flight, text)} km</a
											>
										</td>
										<td class="number">{flight.score ? text.points(flight.score.points) : '–'}</td>
										<td class="number">
											{flights.ofToday ? text.time(flight.startUnixS) : text.day(flight.startUnixS)}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			{/if}
		</HomePanel>

		<!-- What pilots added to the map -->
		<HomePanel
			id="home-community"
			title="From the community"
			class="span-12"
			loading={!home || home.community.phase === 'loading'}
			problem={home?.community.phase === 'failed' ? home.community.problem : undefined}
			absent={home?.community.phase === 'absent'}
			again={() => home?.again('community', todayDate)}
		>
			{#if home?.community.phase === 'loaded'}
				{@const community = home.community.value}
				<div class="halves">
					<section aria-labelledby="home-places">
						<h3 id="home-places">
							Places from pilots <span class="quiet numeric">{community.placesCount}</span>
						</h3>
						{#if community.places.length === 0}
							<p class="quiet">No place from pilots in this region yet.</p>
						{:else}
							<ul class="list">
								{#each community.places as site (site.id)}
									<li>
										<a href={mapHref(site.position)}>{site.name}</a>
										<span class="quiet">
											{site.byPilots?.confirmedInWords || site.byPilots?.addedInWords || ''}
										</span>
									</li>
								{/each}
							</ul>
						{/if}
					</section>
					<section aria-labelledby="home-candidates">
						<h3 id="home-candidates">
							Take-offs without a site <span class="quiet numeric">{community.candidatesCount}</span
							>
						</h3>
						{#if community.candidates.length === 0}
							<p class="quiet">None in this region.</p>
						{:else}
							<ul class="list">
								{#each community.candidates as candidate (`${candidate.position.lat},${candidate.position.lon}`)}
									<li>
										<a href={placeHref(candidate.position)}>{candidate.altitudeWords}</a>
										<span class="quiet">{candidate.words}</span>
									</li>
								{/each}
							</ul>
						{/if}
						{#if community.candidatesMore}
							<p class="credit">{community.candidatesMore}</p>
						{/if}
					</section>
				</div>
			{/if}
		</HomePanel>
	</div>
</div>

<style>
	.sub {
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.stack {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}

	.quiet {
		color: var(--text-secondary);
	}

	.strong {
		font-weight: 600;
	}

	.line,
	.count {
		font-weight: 500;
	}

	.credit {
		color: var(--text-secondary);
		font-size: var(--caption);
	}

	/* A phone's rows reach the panel's edges, their words under its title. */
	.feed {
		margin: 0 calc(-1 * var(--space-4)) calc(-1 * var(--space-2));
	}

	/* Dense rows: a table of 32 px rows with hairlines between them. */
	.scroll {
		overflow-x: auto;
	}

	.rows {
		width: 100%;
		border-collapse: collapse;
	}

	.rows th,
	.rows td {
		height: 2.25rem;
		padding: 0 var(--space-2);
		text-align: left;
		font-weight: 400;
		white-space: nowrap;
	}

	.rows th:first-child,
	.rows td:first-child {
		padding-left: 0;
	}

	.rows th:last-child,
	.rows td:last-child {
		padding-right: 0;
	}

	.rows thead th {
		height: 1.75rem;
		color: var(--text-secondary);
		font-size: var(--subheadline);
		font-weight: 600;
	}

	.rows tbody tr {
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.rows .name {
		font-weight: 600;
		max-width: 12rem;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.rows .words {
		max-width: 14rem;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.rows .number {
		text-align: right;
	}

	/* A table that fits its panel: the name takes what is left, and is cut short. */
	.fit .name {
		width: 100%;
		max-width: 0;
	}

	/* At a phone's width a take-off's name takes a second line before it is
	   cut: three "Startmöglichkei…" in a row name nothing. */
	@media (max-width: 55.99rem) {
		.takeoffs .name {
			padding-block: var(--space-1);
			white-space: normal;
		}

		.takeoffs .name a {
			display: -webkit-box;
			-webkit-box-orient: vertical;
			-webkit-line-clamp: 2;
			line-clamp: 2;
			overflow: hidden;
			overflow-wrap: anywhere;
		}
	}

	.rows .place {
		width: 2rem;
		color: var(--text-secondary);
	}

	.rows .verdict {
		width: 1%;
	}

	.link {
		color: var(--link);
	}

	.shape {
		display: inline-flex;
		vertical-align: -0.125rem;
		color: var(--text-secondary);
	}

	.list {
		display: flex;
		flex-direction: column;
	}

	.list li {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0 var(--space-3);
		padding: var(--space-2) 0;
		box-shadow: inset 0 1px 0 var(--separator);
	}

	.list li > :first-child {
		font-weight: 600;
	}

	.list.fired li {
		flex-direction: column;
		align-items: flex-start;
	}

	/* The newest post: its picture beside its words on a wide screen, over them on a phone. */
	.lead {
		display: grid;
		gap: var(--space-3);
	}

	.lead-words {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}

	.picture {
		display: block;
	}

	.date {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-secondary);
		font-size: var(--subheadline);
	}

	.post-title {
		margin: 0;
		font-size: var(--title);
		line-height: 1.3;
	}

	.post-title a:hover {
		text-decoration: underline;
	}

	.halves {
		display: grid;
		gap: var(--space-4);
	}

	h3 {
		margin: 0 0 var(--space-1);
		font-size: var(--body);
		font-weight: 600;
	}

	@media (hover: hover) {
		.rows a:hover,
		.list a:hover {
			text-decoration: underline;
		}
	}

	@media (max-width: 23.99rem) {
		.wind {
			display: none;
		}
	}

	@media (min-width: 56rem) {
		.lead {
			grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
			align-items: center;
			gap: var(--space-5);
		}

		.halves {
			grid-template-columns: 1fr 1fr;
			gap: var(--space-6);
		}

		.stack {
			gap: var(--space-5);
		}
	}
</style>
