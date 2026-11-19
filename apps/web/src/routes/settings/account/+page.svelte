<!--
	The pilot's account on this site's Venti server: sign in or create one, see
	how the logbook and the server stand, sign out, delete. The iPhone app's
	`AccountScreen.swift` is its twin.
-->
<script lang="ts">
	import { browser } from '$app/env';
	import SignedIn from '#lib/account/SignedIn.svelte';
	import SignInForm from '#lib/account/SignInForm.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { settingsHref } from '#lib/app/routes.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const account = $derived(app.account);

	/** The server is the one this site is served from. */
	const host = browser ? location.host : '';
</script>

<svelte:head>
	<title>Account · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={settingsHref()}>
			<Icon name="back" />
			Settings
		</a>
	</header>
	<h1>Account</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if account?.account}
		<SignedIn {account} signedIn={account.account} />
	{:else if account?.hasServer === false}
		<p class="lead">
			This site has no Venti server behind it, so there is no account to sign in to. Your flights
			stay in this browser.
		</p>
	{:else if account}
		<SignInForm {account} {host} />
	{/if}
</div>

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}
</style>
