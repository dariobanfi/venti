<!--
	Settings: what belongs to the pilot and to this site's Venti server rather
	than to a flight. On the iPhone the same rows stand among the cockpit's
	settings, which a browser has no use for. The pilot's own take-offs are
	kept and forgotten on Today and on a site's page, not here.
-->
<script lang="ts">
	import { browser } from '$app/environment';
	import { inALine } from '#lib/airspace/words.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, airspaceHref } from '#lib/app/routes.ts';
	import {
		applyTheme,
		keepTheme,
		keptTheme,
		THEME_CHOICES,
		type ThemeChoice
	} from '#lib/app/theme.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Segmented from '#lib/design/Segmented.svelte';

	const app = getApp();
	const text = format();
	const account = $derived(app.account);
	const airspace = $derived(app.airspace);
	/** What is new on the account: the server's count, the bubble on the pilot's picture. */
	const unread = $derived(account?.signedIn ? app.unread : undefined);

	/** Light, dark or the system's, kept in this browser. */
	const storage = () => {
		try {
			return browser ? localStorage : undefined;
		} catch {
			return undefined;
		}
	};
	let theme = $state<string>(keptTheme(storage()));
	$effect(() => {
		const chosen = theme as ThemeChoice;
		applyTheme(chosen, document);
		keepTheme(chosen, storage());
	});

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
						{#if unread?.badge}
							<span class="badge numeric" aria-label={unread.label}>{unread.badge}</span>
						{/if}
						<span class="group-value">{accountSaid}</span>
						<span class="chevron"><Icon name="forward" /></span>
					</span>
				</a>
			</div>
			{#if account?.hasServer === false}
				<p class="group-note">This copy of the app is served without Venti behind it.</p>
			{/if}
		</section>

		<section class="group" aria-labelledby="settings-appearance">
			<h2 class="group-title" id="settings-appearance">Appearance</h2>
			<Segmented label="Appearance" options={THEME_CHOICES} bind:value={theme} />
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
		</section>
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
