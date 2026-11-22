<!--
	The page on which a pilot lets a program read their flights, or does not:
	an assistant asked the Venti server, which checked the request, kept it
	under an identifier nobody can guess and linked here
	(`/connect/?request=venti_oar_…`; `docs/api.md`, "Agreeing in the web
	app"). The pilot answers with the session this browser has, however they
	signed in, and types no password.

	Every word, every state and both requests are the core's
	(`crates/core/src/agreement.rs`); the page writes none. It reads the
	request without a token, answers it with the session's, and after an
	answer the server took it sends the browser to the program at once.

	The identifier is read once from the page's own address and held by the
	page's model, in memory. It is written nowhere: not to the browser's
	storage, not to another address, not to the console. The page sends no
	referrer, so neither the program nor anyone else is told this address,
	and nothing in the app links here. A pilot who is not signed in goes to
	the account's page and comes back through the tab's history, where this
	address already is (`#lib/account/way-back.ts`).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { browser } from '$app/env';
	import { goto } from '$app/navigation';
	import { keepWayBack } from '#lib/account/way-back.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { ConnectModel } from '#lib/connect/model.svelte.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const uid = $props.id();

	/** The request's identifier, from this page's own address. */
	const id = browser ? (new URLSearchParams(location.search).get('request') ?? '') : '';

	let model = $state.raw<ConnectModel>();
	const shown = $derived(model?.page);
	const words = $derived(model?.words);
	/** Whether the request waits for an answer: what is asked is shown. */
	const waits = $derived(shown?.state === 'asking' || shown?.state === 'signInFirst');

	$effect(() => {
		let left = false;
		let made: ConnectModel | undefined;
		void app.agreement(id).then((opened) => {
			if (left) return;
			made = opened;
			model = opened;
			// The account as it is stored now: the pilot may have signed in
			// since this page was last drawn, as on the way back from Google.
			void app.account?.refresh();
		});
		// A page the browser kept and shows again knows nothing of what
		// happened since: the account is read again, and the request with it.
		const again = (event: PageTransitionEvent) => {
			if (!event.persisted) return;
			void app.account?.refresh();
			void made?.read();
		};
		addEventListener('pageshow', again);
		return () => {
			left = true;
			removeEventListener('pageshow', again);
			made?.close();
		};
	});

	/** Who is signed in: the request is read for them, and again when that changes. */
	const who = $derived(app.account?.signedIn ? app.account.account?.handle : undefined);
	$effect(() => {
		const opened = model;
		void who;
		if (opened) untrack(() => void opened.read());
	});

	/**
	 * To the app's own sign-in, and back: the account's page returns to this
	 * address, which stays in the tab's history and goes nowhere else.
	 */
	async function toSignIn() {
		await goto(accountHref());
		keepWayBack();
	}
</script>

<svelte:head>
	<title>{shown?.title ? `${shown.title} · Venti` : 'Venti'}</title>
	<!-- Nobody is told this page's address: not the program the browser goes to, not anyone else. -->
	<meta name="referrer" content="no-referrer" />
</svelte:head>

<div class="settings-page connect">
	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if !shown}
		{#if words}
			<p class="waiting" role="status">{words.loading}</p>
		{/if}
	{:else}
		<h1>{shown.title}</h1>
		<p class="lead">{shown.message}</p>

		{#if waits}
			{#if shown.warning}
				<p class="warning" role="note">
					<Icon name="warning" />
					<span>{shown.warning}</span>
				</p>
			{/if}

			<section class="group" aria-labelledby="{uid}-may-read">
				<h2 class="group-title" id="{uid}-may-read">{shown.mayReadLabel}</h2>
				<ul class="group-card">
					{#each shown.mayRead as line (line)}
						<li class="fact">{line}</li>
					{/each}
				</ul>
			</section>

			<section class="group">
				<dl class="group-card">
					{#each shown.rows as row (row.label)}
						<div class="fact">
							<dt>{row.label}</dt>
							<dd>{row.text}</dd>
						</div>
					{/each}
				</dl>
				{#if shown.account}
					<p class="group-note" data-testid="answers-as">{shown.account}</p>
				{/if}
				<p class="group-note">{shown.withdraw}</p>
			</section>
		{/if}

		{#if model?.said}
			<p class="group-note refusal" role="alert">{model.said}</p>
		{/if}

		{#if words && model}
			{#if shown.state === 'asking'}
				<div class="actions">
					<button
						class="button-prominent"
						type="button"
						disabled={model.answering !== undefined}
						onclick={() => void model?.answer(true)}
					>
						{model.answering === 'allow' ? words.answering : words.allow}
					</button>
					<button
						class="button-plain"
						type="button"
						disabled={model.answering !== undefined}
						onclick={() => void model?.answer(false)}
					>
						{model.answering === 'refuse' ? words.answering : words.refuse}
					</button>
				</div>
			{:else if shown.state === 'signInFirst'}
				<div class="actions">
					<button class="button-prominent" type="button" onclick={toSignIn}>
						{words.signIn}
					</button>
				</div>
			{:else if shown.state === 'failed'}
				<div class="actions">
					<button
						class="button-plain"
						type="button"
						disabled={model.reading}
						onclick={() => void model?.read()}
					>
						{model.reading ? words.loading : words.retry}
					</button>
				</div>
			{/if}
		{/if}
	{/if}
</div>

<style>
	/* One plain column, as the account's page: no way back above the title. */
	.connect {
		padding-top: calc(var(--space-6) + var(--safe-top));
	}

	.lead {
		padding: 0 var(--space-4);
		color: var(--text-secondary);
	}

	.waiting {
		padding: var(--space-5);
		text-align: center;
		color: var(--text-secondary);
	}

	/* The pack's status line in the caution manner: its sign, the words, its colour. */
	.warning {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		margin-top: var(--space-5);
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-s);
		background: var(--color-caution-soft);
		color: var(--color-caution);
		font-weight: 500;
	}

	.warning :global(svg) {
		flex: none;
		width: 1.25em;
		height: 1.25em;
		margin-top: 0.1em;
	}

	.fact {
		padding: var(--space-3) var(--space-4);
		overflow-wrap: anywhere;
	}

	.fact dt {
		font-size: var(--subheadline);
		font-weight: 500;
		color: var(--text-secondary);
	}

	.refusal {
		color: var(--danger);
	}

	.actions {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-top: var(--space-5);
	}

	/* On a wide screen the buttons are as wide as their words, side by side. */
	@media (min-width: 56rem) {
		.actions {
			flex-direction: row;
			gap: var(--space-3);
		}

		.actions .button-prominent {
			min-width: 12rem;
		}
	}
</style>
