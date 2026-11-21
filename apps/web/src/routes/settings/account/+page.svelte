<!--
	The pilot's account on this site's Venti server: sign in or create one, see
	how the logbook and the server stand, sign out, delete. The iPhone app's
	`AccountScreen.swift` is its twin.

	Google comes back here after a pilot went there to sign in, to link the
	account or to delete it (`#lib/account/google.ts`): the answer is taken
	out of the address at once and handed to the account.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import { type GoogleReturn, takeGoogleReturn } from '#lib/account/google.ts';
	import SignedIn from '#lib/account/SignedIn.svelte';
	import SignInForm from '#lib/account/SignInForm.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { settingsHref } from '#lib/app/routes.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const account = $derived(app.account);

	// What the server says of the account can change while a browser stays
	// signed in (its name, whether it is a partner for flying sites): it is
	// asked whenever this page opens.
	$effect(() => {
		const model = account;
		untrack(() => void model?.ask());
	});

	/** What Google sent back, out of the address bar before anything else. */
	let fromGoogle: GoogleReturn | undefined = browser ? takeGoogleReturn() : undefined;
	$effect(() => {
		const model = account;
		if (!model || !fromGoogle) return;
		const returned = fromGoogle;
		fromGoogle = undefined;
		untrack(() => void model.finishGoogle(returned));
	});

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
			This copy of the app is served without Venti behind it, so there is no account to sign in to.
			Your flights stay in this browser.
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
