<!--
	"Alert me" on a wind station's page and on a take-off's page: it opens the
	form of an alarm filled with the station or the place. An alarm lives on
	the account, so signed out it is a line that leads to signing in. On a
	site without a Venti server there is nothing.

	Its words are the core's (`alertFormWords`).
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { AlertDraft } from '#lib/core/index.ts';
	import AlertForm from './AlertForm.svelte';

	interface Props {
		/** The draft the form starts from, made when it opens. */
		start: () => AlertDraft;
		/** A card of its own, or a row of the card it stands in. */
		look: 'card' | 'row';
	}

	let { start, look }: Props = $props();

	const app = getApp();
	const account = $derived(app.account);
	const alerts = $derived(app.alerts);

	let open = $state(false);
	let draft = $state.raw<AlertDraft>();

	$effect(() => {
		void alerts?.words().catch(() => {});
	});
	const words = $derived(alerts?.said);

	function begin() {
		draft = start();
		open = true;
	}
</script>

{#if account && alerts && words && account.hasServer !== false}
	{#if account.signedIn}
		<button class="entry {look}" type="button" onclick={begin}>{words.alertMe}</button>
		{#if draft}
			<AlertForm {open} {alerts} start={draft} onclose={() => (open = false)} />
		{/if}
	{:else}
		<a class="entry {look}" href={accountHref()}>{words.signedOut}</a>
	{/if}
{/if}

<style>
	.entry {
		display: flex;
		align-items: center;
		width: 100%;
		min-height: var(--target);
		padding: 0.5rem 1rem;
		color: var(--link);
		text-align: left;
		text-decoration: none;
	}

	.card {
		background: var(--surface);
		border-radius: var(--radius-card);
	}

	.row {
		box-shadow: inset 0 0.5px 0 var(--separator);
	}

	@media (hover: hover) {
		.entry:hover {
			background: color-mix(in srgb, var(--surface), var(--text) 6%);
		}
	}
</style>
