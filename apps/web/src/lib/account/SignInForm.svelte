<!--
	Signing in, or creating an account: one form with a switch between the
	two, as on the iPhone. It says what an account does before it asks for
	anything, and where a password is chosen it says that a forgotten one
	cannot be recovered.

	Whether what was typed will do is the core's to say, and the server's:
	their refusals stand under the fields in their own words. The browser's
	own checks are off, so there is one voice.
-->
<script lang="ts">
	import Segmented from '#lib/design/Segmented.svelte';
	import type { AccountModel } from './model.svelte';

	interface Props {
		account: AccountModel;
		/** The server the account is on, as the pilot knows it: its host name. */
		host: string;
	}

	let { account, host }: Props = $props();

	const MODES = ['Sign in', 'New account'] as const;
	let mode = $state<string>(MODES[0]);
	const isNew = $derived(mode === 'New account');

	let handle = $state('');
	let displayName = $state('');
	let password = $state('');
	let invite = $state('');
	/** Why the last attempt did not work, in words. */
	let refusal = $state.raw<string>();

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		refusal = undefined;
		refusal = isNew
			? await account.createAccount({ handle, displayName, password, invite })
			: await account.signIn(handle, password);
	}
</script>

<p class="lead">
	With an account, your flights are copied to this site’s Venti server. They appear on your other
	devices, and you can share one with a link. A flight stays private until you share it.
</p>

<form onsubmit={submit} novalidate>
	<section class="group" aria-labelledby="account-server">
		<h2 class="group-title" id="account-server">{host}</h2>
		<div class="group-card">
			<div class="switch">
				<Segmented label="What to do" options={MODES} bind:value={mode} />
			</div>
			<label class="field">
				<span>Handle</span>
				<input
					name="username"
					autocomplete="username"
					autocapitalize="none"
					autocorrect="off"
					spellcheck="false"
					bind:value={handle}
				/>
			</label>
			{#if isNew}
				<label class="field">
					<span>Name</span>
					<input
						name="name"
						autocomplete="name"
						placeholder="Shown to others"
						aria-label="Name shown to others"
						bind:value={displayName}
					/>
				</label>
			{/if}
			<label class="field">
				<span>Password</span>
				<input
					name="password"
					type="password"
					autocomplete={isNew ? 'new-password' : 'current-password'}
					bind:value={password}
				/>
			</label>
			{#if isNew}
				<label class="field">
					<span>Invite code</span>
					<input
						name="invite"
						autocomplete="off"
						autocapitalize="none"
						autocorrect="off"
						spellcheck="false"
						bind:value={invite}
					/>
				</label>
			{/if}
		</div>
		{#if isNew}
			<p class="group-note">
				A handle is 3 to 20 letters, digits or underscores. A password needs at least 15 characters;
				a few words in a row are easy to remember. A forgotten password cannot be recovered: the
				server knows no email address. Handle and name are public once you share a flight.
			</p>
			<p class="group-note">
				The invite code is for a server that takes accounts by invite. Whoever runs it hands them
				out.
			</p>
		{/if}
		{#if refusal}
			<p class="group-note refusal" role="alert">{refusal}</p>
		{/if}
	</section>

	<div class="submit">
		<button
			class="button-prominent"
			type="submit"
			disabled={account.working || handle === '' || password === ''}
		>
			{#if account.working}
				Working…
			{:else}
				{isNew ? 'Create account' : 'Sign in'}
			{/if}
		</button>
	</div>
</form>

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	.switch {
		padding: 0.75rem 1rem;
	}

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		margin-top: 1.5rem;
	}

	.submit button {
		flex: 1;
	}

	/* On a wide screen the button is as wide as its words, not as the form. */
	@media (min-width: 56rem) {
		.submit button {
			flex: none;
			min-width: 12rem;
		}
	}
</style>
