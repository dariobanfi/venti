<!--
	"Programs with access": the tokens a pilot gives a logger, a script or
	another maker's app, so that it can act for them without their password.
	The list, the form that gives access, a token's secret shown once, and
	the question before an access is withdrawn.

	Every word about what a token is and may do is the core's (`TokenForm`,
	`TokenView`, `TokenCreated`, `TokenWithdrawal`), and so is every refusal.

	The secret of a token that was just made is held here, in memory, while
	it is shown, and nowhere else: this page has no saved state, writes
	nothing to the browser's storage or the address, and lets go of the
	secret when the pilot says it is copied and when the page is left. Until
	then the page is not left for another of the app's.
-->
<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import GiveAccessForm from '#lib/account/GiveAccessForm.svelte';
	import TokenSecret from '#lib/account/TokenSecret.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { ProgramView, TokenCreated, TokenForm } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const account = $derived(app.account);
	const id = $props.id();

	/** The core's words for the screen and the form. */
	let form = $state.raw<TokenForm>();
	/** The programs with access, once the server has said. */
	let programs = $state.raw<ProgramView[]>();
	/** Why the list could not be read or an access not withdrawn, in words. */
	let problem = $state.raw<string>();
	let giving = $state(false);
	/** A token that was just made, with its secret: while it is shown. */
	let created = $state.raw<TokenCreated>();
	let secret = $state<TokenSecret>();
	/** The program whose access the pilot is asked about; it keeps its words while the question closes. */
	let asked = $state.raw<ProgramView>();
	let asking = $state(false);
	let withdrawing = $state(false);

	async function read(from: NonNullable<typeof account>) {
		try {
			form ??= await from.programsForm();
			programs = await from.programs();
			problem = undefined;
		} catch (error) {
			problem = (error as Error).message;
		}
	}

	// Asked for when the page opens, and when the pilot has signed in.
	$effect(() => {
		const from = account;
		if (from?.signedIn) untrack(() => void read(from));
	});

	function given(made: TokenCreated) {
		giving = false;
		created = made;
	}

	function copied() {
		created = undefined;
		if (account) void read(account);
	}

	// The secret is shown once: the app's other pages wait until the pilot
	// says it is copied. Closing the tab asks, in the browser's own words.
	beforeNavigate((navigation) => {
		if (!created) return;
		navigation.cancel();
		secret?.hold();
	});

	onDestroy(() => {
		created = undefined;
	});

	/** When a token ends, or that it has: the core's day after the word its notes name for it. */
	function ended(token: ProgramView['token']): string {
		return `${token.hasEnded ? 'Ended' : 'Ends'} ${token.ends}`;
	}

	function ask(program: ProgramView) {
		asked = program;
		asking = true;
	}

	async function withdraw() {
		asking = false;
		if (!account || !asked || withdrawing) return;
		withdrawing = true;
		try {
			await account.withdrawAccess(asked.token.id);
			problem = undefined;
		} catch (error) {
			problem = (error as Error).message;
		} finally {
			withdrawing = false;
		}
		try {
			programs = await account.programs();
		} catch {
			// The list stays as it was, with what went wrong above it.
		}
	}
</script>

<svelte:head>
	<title>{form?.title ?? 'Account'} · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={accountHref()}>
			<Icon name="back" />
			Account
		</a>
	</header>
	{#if form}
		<h1>{form.title}</h1>
	{/if}

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if account && !account.signedIn}
		<p class="lead">Sign in to your account first.</p>
	{:else if created}
		<TokenSecret bind:this={secret} {created} ondone={copied} />
	{:else if form}
		<p class="lead">{form.intro}</p>

		{#if problem}
			<p class="group-note refusal" role="alert">{problem}</p>
		{/if}

		{#if programs}
			<section class="group" aria-label={form.title}>
				{#if programs.length === 0}
					<p class="group-card group-row none">{form.empty}</p>
				{:else}
					<ul class="programs" aria-label={form.title}>
						{#each programs as program (program.token.id)}
							{@const token = program.token}
							<li class="group-card">
								<div class="about">
									<span>{token.name}</span>
									<span class="lesser">{token.scopesLine}</span>
									<span class="lesser numeric">
										Made {token.made} · Last used {token.lastUsed} ·
										<span class:ended={token.hasEnded}>{ended(token)}</span>
									</span>
								</div>
								<button
									class="group-row destructive"
									type="button"
									disabled={withdrawing}
									aria-describedby="{id}-{token.id}"
									onclick={() => ask(program)}
								>
									{program.withdrawal.confirm}
								</button>
								<span class="visually-hidden" id="{id}-{token.id}">{token.name}</span>
							</li>
						{/each}
					</ul>
				{/if}
				<p class="group-note">{form.never}</p>
			</section>

			<div class="give">
				<button class="button-prominent" type="button" onclick={() => (giving = true)}>
					{form.submit}
				</button>
			</div>
		{:else if !problem}
			<p class="group-note" role="status">Asking Venti…</p>
		{/if}
	{/if}
</div>

{#if form && account}
	<GiveAccessForm open={giving} {account} {form} ongiven={given} onclose={() => (giving = false)} />
{/if}

<ConfirmDialog
	open={asking}
	title={asked?.withdrawal.title ?? ''}
	message={asked?.withdrawal.message ?? ''}
	confirm={asked?.withdrawal.confirm ?? ''}
	cancel={asked?.withdrawal.cancel}
	onconfirm={withdraw}
	oncancel={() => (asking = false)}
/>

<style>
	.lead {
		margin-top: var(--space-2);
		padding: 0 var(--space-4);
		color: var(--text-secondary);
	}

	.programs {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	/* A program: its name, what it may do, and its dates, one under the other. */
	.about {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-4);
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.ended,
	.refusal {
		color: var(--danger);
	}

	.none {
		color: var(--text-secondary);
	}

	.give {
		display: flex;
		justify-content: flex-end;
		margin-top: var(--space-5);
	}
</style>
