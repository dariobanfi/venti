<!--
	The account this browser is signed in to: who it is, how the logbook and
	the server stand, a button to sync now, and the ways out: sign out, and
	delete the account. The iPhone app's `SignedInView` is its twin.

	Every number and every refusal here is the core's. The screen adds the
	time of the last sync in the reader's own clock.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { alertsHref, programsHref } from '#lib/app/routes.ts';
	import type { AccountView } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Dialog from '#lib/design/Dialog.svelte';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import type { AccountModel } from './model.svelte';

	interface Props {
		account: AccountModel;
		signedIn: AccountView;
	}

	let { account, signedIn }: Props = $props();

	const text = format();

	/** What the alarms fired that no notification told of. */
	const unseen = $derived(getApp().alerts?.unseen ?? 0);

	/** The server as the pilot knows it: its host name. */
	const host = $derived(hostOf(signedIn.server));

	function hostOf(address: string): string {
		try {
			return new URL(address).host;
		} catch {
			return address;
		}
	}

	let confirmingSignOut = $state(false);
	let deleting = $state(false);
	let password = $state('');
	/** Why the account was not deleted, in words. */
	let refusal = $state.raw<string>();

	async function signOut() {
		confirmingSignOut = false;
		await account.signOut();
	}

	function askToDelete() {
		password = '';
		refusal = undefined;
		deleting = true;
	}

	async function deleteAccount(event: SubmitEvent) {
		event.preventDefault();
		const entered = password;
		password = '';
		refusal = await account.deleteAccount(entered);
		// Gone: the page shows the form to sign in, and the dialog goes with
		// this view. Refused: the dialog stays, with the reason.
	}
</script>

<section class="group" aria-label="Account">
	<dl class="group-card">
		<div class="group-row">
			<dt>Handle</dt>
			<dd>@{signedIn.handle}</dd>
		</div>
		<div class="group-row">
			<dt>Name</dt>
			<dd>{signedIn.displayName}</dd>
		</div>
		<div class="group-row">
			<dt>Address</dt>
			<dd>{host}</dd>
		</div>
	</dl>
</section>

<section class="group" aria-labelledby="account-logbook">
	<h2 class="group-title" id="account-logbook">Logbook</h2>
	<div class="group-card">
		{#if signedIn.needsSignIn}
			<p class="group-row group-warning" role="alert">
				<Icon name="warning" />
				<span>This browser was signed out. Sign out here and sign in again.</span>
			</p>
		{/if}
		<dl class="numeric">
			<div class="group-row">
				<dt>Flights on Venti</dt>
				<dd>{text.count(signedIn.flightsOnServer)}</dd>
			</div>
			{#if signedIn.flightsToSend > 0}
				<div class="group-row">
					<dt>Waiting to be sent</dt>
					<dd>{text.count(signedIn.flightsToSend)}</dd>
				</div>
			{/if}
			{#if signedIn.lastSyncUnixS !== undefined}
				<div class="group-row">
					<dt>In step since</dt>
					<dd>
						{text.dayInYear(signedIn.lastSyncUnixS)}, {text.time(signedIn.lastSyncUnixS)}
					</dd>
				</div>
			{/if}
			{#if account.lastSync}
				<div class="group-row">
					<dt>Last sync</dt>
					<dd>{account.lastSync}</dd>
				</div>
			{/if}
		</dl>
		<!-- A session that has ended is said once, above, with what to do about it. -->
		{#if account.problem && !signedIn.needsSignIn}
			<p class="group-row group-warning" role="alert">
				<Icon name="warning" />
				<span>{account.problem}</span>
			</p>
		{/if}
		<button
			class="group-row"
			type="button"
			disabled={account.working || signedIn.needsSignIn}
			onclick={() => account.sync({ force: true, quietly: false })}
		>
			{account.working ? 'Working…' : 'Sync now'}
		</button>
	</div>
	<p class="group-note">
		Venti syncs by itself when you open this site and after the logbook changes. Your flights stay
		private on Venti until you share one.
	</p>
</section>

<!-- What else the account holds. The row's name is the core's, as the page's title is. -->
<section class="group" aria-label="Access">
	<div class="group-card">
		<a class="group-row" href={alertsHref()}>
			<span>Alerts</span>
			<span class="trailing">
				{#if unseen > 0}
					<span class="badge numeric" aria-label="{unseen} new">{unseen}</span>
				{/if}
				<span class="chevron"><Icon name="forward" /></span>
			</span>
		</a>
		{#await account.programsForm() then programs}
			<a class="group-row" href={programsHref()}>
				<span>{programs.title}</span>
				<span class="chevron"><Icon name="forward" /></span>
			</a>
		{:catch}
			<!-- Without the core's words there is no row. -->
		{/await}
	</div>
</section>

<section class="group" aria-label="Leaving">
	<div class="group-card">
		<button
			class="group-row"
			type="button"
			disabled={account.working}
			onclick={() => (confirmingSignOut = true)}
		>
			Sign out
		</button>
		<button
			class="group-row destructive"
			type="button"
			disabled={account.working}
			onclick={askToDelete}
		>
			Delete account
		</button>
	</div>
</section>

<ConfirmDialog
	open={confirmingSignOut}
	title="Sign out?"
	message="Your flights stay in this browser and on Venti."
	confirm="Sign out"
	onconfirm={signOut}
	oncancel={() => (confirmingSignOut = false)}
/>

<Dialog
	open={deleting}
	presentation="alert"
	decision
	labelledby="delete-title"
	describedby="delete-message"
	onclose={() => (deleting = false)}
>
	<form onsubmit={deleteAccount} novalidate>
		<div class="words">
			<h2 id="delete-title">Delete your account?</h2>
			<p id="delete-message">
				The account and every flight on Venti are deleted at once and for good, and your shared
				links stop working. The flights in this browser stay. Enter your password to go on.
			</p>
			<!-- For the browser's password manager, which fills a password in for a name. -->
			<input
				class="visually-hidden"
				type="text"
				name="username"
				autocomplete="username"
				value={signedIn.handle}
				tabindex="-1"
				aria-hidden="true"
				readonly
			/>
			<!-- svelte-ignore a11y_autofocus -->
			<input
				class="password"
				type="password"
				name="password"
				autocomplete="current-password"
				placeholder="Password"
				aria-label="Password"
				autofocus
				bind:value={password}
			/>
			{#if refusal}
				<p class="refusal" role="alert">{refusal}</p>
			{/if}
		</div>
		<div class="choices">
			<button
				class="choice destructive"
				type="submit"
				disabled={account.working || password === ''}
			>
				Delete account
			</button>
			<button class="choice cancel" type="button" onclick={() => (deleting = false)}>Cancel</button>
		</div>
	</form>
</Dialog>

<style>
	.trailing {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.chevron {
		color: var(--icon-quiet);
		font-size: 0.75rem;
	}

	/* The words of a sync are a phrase, not a number: they may wrap. */
	dd {
		max-width: 60%;
	}

	.words {
		padding: 1.25rem 1.25rem 1rem;
		text-align: center;
	}

	h2 {
		font-size: var(--body);
		font-weight: 600;
	}

	.words p {
		margin-top: 0.375rem;
		font-size: var(--footnote);
	}

	.password {
		width: 100%;
		min-height: var(--target);
		margin-top: 0.875rem;
		padding: 0 0.75rem;
		border: 0.5px solid var(--separator);
		border-radius: var(--radius-control);
		background: var(--background);
		color: var(--text);
		font: inherit;
	}

	.words .refusal {
		margin-top: 0.625rem;
		color: var(--danger);
	}

	.choices {
		display: flex;
		flex-direction: column;
	}

	.choice {
		min-height: var(--target);
		border-top: 0.5px solid var(--separator);
	}

	.destructive {
		color: var(--danger);
	}

	.cancel {
		color: var(--link);
		font-weight: 600;
	}

	@media (hover: hover) {
		.choice:not(:disabled):hover {
			background: var(--fill);
		}
	}
</style>
