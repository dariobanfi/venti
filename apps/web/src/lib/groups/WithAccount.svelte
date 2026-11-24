<!--
	What every screen about groups and plans has in common: there is nothing
	to show without an account on this site's Venti server, and the screen
	says so instead of staying empty. Signed in, it shows what it is given.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { GroupsModel } from './model.svelte';
	import { NO_SERVER } from './words';

	let { children }: { children: Snippet<[GroupsModel]> } = $props();

	const app = getApp();
	const account = $derived(app.account);
	const groups = $derived(app.groups);
</script>

{#if app.failure}
	<p class="group-note">Venti could not open its data. {app.failure}</p>
{:else if account?.hasServer === false}
	<p class="lead">{NO_SERVER}</p>
{:else if account && groups}
	{#if account.signedIn}
		{@render children(groups)}
	{:else}
		<div class="group-card words">
			<p>{app.said?.community.plansSignedOut}</p>
			<a class="button-plain" href={accountHref()}>{app.said?.common.signIn}</a>
		</div>
	{/if}
{:else}
	<p class="lead" role="status">Opening…</p>
{/if}

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	/* A few words and one thing to do about them. */
	.words {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.25rem;
		margin-top: 0.5rem;
		padding: 1rem 1rem 0.5rem;
	}

	/* The line that parts the rows of a card is not for these two: a
	   sentence and its link are one thing. */
	.words .button-plain {
		margin-left: -0.75rem;
		box-shadow: none;
	}
</style>
