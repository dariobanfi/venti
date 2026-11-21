<!--
	Signing in, or creating an account: one form with a switch between the
	two, as on the iPhone. It says what an account does before it asks for
	anything, and where a password is chosen it says that a forgotten one
	cannot be recovered.

	Who may create an account is the server's to say, and the form follows:
	a field for an invite code where the server takes accounts by invite, and
	where it does not say; none where anyone may register; and no form at all
	where the server takes no new accounts.

	Whether what was typed will do is the core's to say, and the server's:
	their refusals stand under the fields in their own words. The browser's
	own checks are off, so there is one voice.
-->
<script lang="ts">
	import Segmented from '#lib/design/Segmented.svelte';
	import { goToGoogle } from './google';
	import GoogleHandleStep from './GoogleHandleStep.svelte';
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
	/** The server takes no new accounts: there is nothing to fill in. */
	const closed = $derived(isNew && account.registration === 'closed');
	/**
	 * Whether a new account may need an invite code: the server says so, or
	 * says nothing, as one older than the question, which may ask for one.
	 */
	const mayNeedInvite = $derived(
		account.registration === 'invite' || account.registration === undefined
	);

	/** Where this browser keeps whether the pilot opened the form behind Google's button. */
	const WITH_PASSWORD = 'venti.account-with-password';

	/** Whether the pilot opened the form behind Google's button, as this browser remembers. */
	let withPassword = $state(remembered());

	function remembered(): boolean {
		try {
			return localStorage.getItem(WITH_PASSWORD) === '1';
		} catch {
			return false;
		}
	}

	function toggleForm() {
		withPassword = !withPassword;
		try {
			if (withPassword) localStorage.setItem(WITH_PASSWORD, '1');
			else localStorage.removeItem(WITH_PASSWORD);
		} catch {
			// Not remembered: it opens closed next time.
		}
	}

	const offersGoogle = $derived(account.googleSignIn === true && account.googleWords !== undefined);
	/** The form shows once it is known whether "Continue with Google" stands above it. */
	const formShown = $derived(account.googleSignIn === false || (offersGoogle && withPassword));
	/** Why the sign-in with Google did not work, in words. */
	const googleRefusal = $derived(
		account.googleRefusal && account.googleRefusal.intent !== 'link'
			? account.googleRefusal.words
			: undefined
	);

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
			? await account.createAccount({
					handle,
					displayName,
					password,
					invite: mayNeedInvite ? invite : ''
				})
			: await account.signIn(handle, password);
	}
</script>

{#if account.googleStep && account.googleWords}
	<GoogleHandleStep
		{account}
		words={account.googleWords}
		signUp={account.googleStep}
		oncancel={() => (account.googleStep = undefined)}
	/>
{:else}
	<p class="lead">
		With an account, your flights are copied to Venti. They appear on your other devices, and you
		can share one with a link. A flight stays private until you share it.
	</p>

	{#if offersGoogle && account.googleWords}
		<section class="group google" aria-label={account.googleWords.continueWithGoogle}>
			<button
				class="button-prominent"
				type="button"
				disabled={account.working}
				onclick={() => goToGoogle('signIn')}
			>
				{account.googleWords.continueWithGoogle}
			</button>
			{#if googleRefusal}
				<p class="group-note refusal" role="alert">{googleRefusal}</p>
			{/if}
			<button
				class="button-plain with-password"
				type="button"
				aria-expanded={withPassword}
				onclick={toggleForm}
			>
				{account.googleWords.withPassword}
			</button>
		</section>
	{/if}

	{#if formShown}
		<form onsubmit={submit} novalidate>
			<section class="group" aria-labelledby="account-server">
				<h2 class="group-title" id="account-server">{host}</h2>
				<div class="group-card">
					<div class="switch">
						<Segmented label="What to do" options={MODES} bind:value={mode} />
					</div>
					{#if closed}
						<p class="closed">
							Venti takes no new accounts at the moment. An account you already have signs in as
							before.
						</p>
					{:else}
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
						{#if isNew && mayNeedInvite}
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
					{/if}
				</div>
				{#if isNew && !closed}
					<p class="group-note">
						A handle is 3 to 20 letters, digits or underscores. A password needs at least 15
						characters; a few words in a row are easy to remember. A forgotten password cannot be
						recovered: Venti knows no email address. Handle and name are public once you share a
						flight.
					</p>
					{#if account.registration === 'invite'}
						<p class="group-note">
							Venti takes new accounts by invite for now, and hands the codes out itself.
						</p>
					{:else if mayNeedInvite}
						<p class="group-note">
							An invite code is needed while Venti takes accounts by invite, and Venti hands them
							out.
						</p>
					{/if}
				{/if}
				{#if refusal}
					<p class="group-note refusal" role="alert">{refusal}</p>
				{/if}
			</section>

			{#if !closed}
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
			{/if}
		</form>
	{/if}
{/if}

<style>
	.lead {
		margin-top: 0.5rem;
		padding: 0 1rem;
		color: var(--text-secondary);
	}

	.switch {
		padding: 0.75rem 1rem;
	}

	.google {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: var(--space-3);
	}

	.with-password {
		align-self: center;
	}

	/* On a wide screen the buttons are as wide as their words, not as the column. */
	@media (min-width: 56rem) {
		.google {
			align-items: flex-start;
		}

		.google .button-prominent {
			min-width: 12rem;
		}

		.with-password {
			align-self: flex-start;
		}
	}

	.refusal {
		color: var(--danger);
	}

	.closed {
		padding: 0.25rem 1rem 1rem;
		color: var(--text-secondary);
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
