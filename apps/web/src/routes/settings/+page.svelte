<!--
	Settings: what belongs to the pilot and to this site's Venti server rather
	than to a flight. On the iPhone the same rows stand among the cockpit's
	settings, which a browser has no use for.
-->
<script lang="ts">
	import { inALine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, airspaceHref } from '#lib/app/routes.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const text = format();
	const account = $derived(app.account);
	const airspace = $derived(app.airspace);

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
			<h2 class="group-title" id="settings-server">Venti server</h2>
			<div class="group-card">
				<a class="group-row" href={accountHref()}>
					<span>Account</span>
					<span class="trailing">
						<span class="group-value">{accountSaid}</span>
						<span class="chevron"><Icon name="forward" /></span>
					</span>
				</a>
			</div>
			<p class="group-note">
				{#if account?.hasServer === false}
					This site has no Venti server behind it. The map has no live wind, and there is no account
					to sign in to. Your flights stay in this browser.
				{:else}
					The server this site comes from gives the map its live wind. With an account it also keeps
					your flights, for your other devices and for sharing.
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
				The maps draw the airspace the server has for where you look, and that of files you bring
				in.
			</p>
		</section>

		{#if app.logbook}
			<section class="group" aria-labelledby="settings-about">
				<h2 class="group-title" id="settings-about">About</h2>
				<dl class="group-card">
					<div class="group-row">
						<dt>Version</dt>
						<dd>{app.logbook.coreVersion}</dd>
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
</style>
