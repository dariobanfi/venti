<!--
	Settings: what belongs to the pilot and to this site's Venti server rather
	than to a flight. On the iPhone the same rows stand among the cockpit's
	settings, which a browser has no use for.

	"Your take-offs" lists the sites the pilot marked as their own on a
	site's page: the core keeps them in this browser (`Core.keptSites`) and
	says so in its own sentence under the list. The phones have the list in
	their settings beside the waypoints; a browser has no waypoints, so it
	stands here as a group of its own.
-->
<script lang="ts">
	import { inALine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, airspaceHref, placeHref } from '#lib/app/routes.ts';
	import type { KeptSite } from '#lib/core/index.ts';
	import { compassPoint, format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import { asOf, detail } from '#lib/sites/words.ts';

	const app = getApp();
	const text = format();
	const account = $derived(app.account);
	const airspace = $derived(app.airspace);
	const kept = $derived(app.kept);
	/** What the alarms fired that no notification told of. */
	const unseen = $derived(account?.signedIn ? (app.alerts?.unseen ?? 0) : 0);

	/** The pilot's position, where Today was given it: the list then says how far each site is. */
	const from = $derived.by(() => {
		const today = app.today?.state;
		return today?.nearYou ? { lat: today.place.lat, lon: today.place.lon } : undefined;
	});

	$effect(() => {
		// Read again after a site was kept or forgotten, here or in another tab.
		void kept?.revision;
		kept?.read(from).catch(() => {});
	});

	/** A kept site in a line, with how far away and where it lies when a place is known. */
	function keptDetail(one: KeptSite): string {
		const away =
			one.distanceM === undefined
				? ''
				: ` · ${text.kilometres(one.distanceM)} km${
						one.bearingDeg === undefined ? '' : ` ${compassPoint(one.bearingDeg)}`
					}`;
		return `${detail(one.site, text)}${away}`;
	}

	/** The account in a word or two, for the row that leads to it. */
	const accountSaid = $derived.by(() => {
		if (!account) return '';
		if (!account.account) return 'Not signed in';
		return account.account.needsSignIn ? 'Signed out' : `@${account.account.handle}`;
	});
</script>

<svelte:head>
	<title>Settings · Venti</title>
</svelte:head>

<div class="settings-page">
	<header></header>
	<h1>Settings</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else}
		<section class="group" aria-labelledby="settings-server">
			<h2 class="group-title" id="settings-server">Venti</h2>
			<div class="group-card">
				<a class="group-row" href={accountHref()}>
					<span>Account</span>
					<span class="trailing">
						{#if unseen > 0}
							<span class="badge numeric" aria-label="{unseen} new alerts">{unseen}</span>
						{/if}
						<span class="group-value">{accountSaid}</span>
						<span class="chevron"><Icon name="forward" /></span>
					</span>
				</a>
			</div>
			<p class="group-note">
				{#if account?.hasServer === false}
					This copy of the app is served without Venti behind it. The map has no live wind, and
					there is no account to sign in to. Your flights stay in this browser.
				{:else}
					Venti gives the map its live wind. With an account it also keeps your flights, for your
					other devices and for sharing.
				{/if}
			</p>
		</section>

		<section class="group" aria-labelledby="settings-maps">
			<h2 class="group-title" id="settings-maps">Maps</h2>
			<div class="group-card">
				<a class="group-row" href={airspaceHref()}>
					<span>Airspace</span>
					<span class="trailing">
						<span class="group-value numeric">
							{airspace ? inALine(airspace.kept, airspace.files, text) : ''}
						</span>
						<span class="chevron"><Icon name="forward" /></span>
					</span>
				</a>
			</div>
			<p class="group-note">
				The maps draw the airspace Venti has for where you look, and that of files you bring in.
			</p>
		</section>

		{#if kept?.list}
			{@const list = kept.list}
			<section class="group" aria-labelledby="settings-takeoffs">
				<h2 class="group-title" id="settings-takeoffs">Your take-offs</h2>
				<ul class="group-card" aria-labelledby="settings-takeoffs">
					{#each list.sites as one (one.site.id)}
						<li class="kept">
							<a class="entry" href={placeHref(one.site.position, one.site.id)}>
								<span>{one.site.name}</span>
								<span class="lesser numeric">{keptDetail(one)}</span>
								{#if one.asOfUnixS !== undefined}
									<span class="lesser numeric">{asOf(one.asOfUnixS, text)}</span>
								{/if}
							</a>
							<button
								class="forget"
								type="button"
								aria-label="Forget {one.site.name}"
								title="Forget"
								onclick={() => void kept.forget(one.site.id).catch(() => {})}
							>
								<Icon name="close" />
							</button>
						</li>
					{:else}
						<li class="group-row none">None kept</li>
					{/each}
				</ul>
				<p class="group-note">
					Today lists your take-offs first, however far away. A site’s page, on the map or from
					Today, has “Keep as my take-off”.
				</p>
				<p class="group-note">{list.note}</p>
				{#each list.credits as credit (credit.source)}
					<p class="group-note">
						<a href={credit.url} target="_blank" rel="noopener noreferrer">{credit.text}</a>
					</p>
				{/each}
			</section>
		{/if}

		{#if app.myFlights}
			<section class="group" aria-labelledby="settings-about">
				<h2 class="group-title" id="settings-about">About</h2>
				<dl class="group-card">
					<div class="group-row">
						<dt>Version</dt>
						<dd>{app.myFlights.coreVersion}</dd>
					</div>
				</dl>
			</section>
		{/if}
	{/if}
</div>

<style>
	.trailing {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		min-width: 0;
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	/* A kept site: what is known of it, one under the other, and the way to forget it. */
	.kept {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.entry {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		padding: var(--space-3) var(--space-4);
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.none {
		color: var(--text-secondary);
	}

	.forget {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--target);
		height: var(--target);
		margin-right: var(--space-2);
		border-radius: var(--radius-pill);
		color: var(--text-secondary);
	}

	.group-note a {
		color: var(--link);
	}

	@media (hover: hover) {
		.entry:hover,
		.forget:hover {
			background: var(--fill);
		}
	}
</style>
