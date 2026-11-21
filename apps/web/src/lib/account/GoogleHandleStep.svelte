<!--
	The step after a first sign-in with Google: the pilot chooses a handle,
	and the account is made. The handle the server suggests stands in the
	field with the core's line of where it is from, the name from the Google
	account in the name's field, and a field for an invite code where the
	server takes accounts by invite. Every word is the core's
	(`googleWords`, `GoogleSignUp`), and so is every refusal.
-->
<script lang="ts">
	import type { GoogleSignUp, GoogleWords } from '#lib/core/index.ts';
	import type { AccountModel } from './model.svelte';

	interface Props {
		account: AccountModel;
		words: GoogleWords;
		signUp: GoogleSignUp;
		/** Back to the account screen's first step, without an account. */
		oncancel: () => void;
	}

	let { account, words, signUp, oncancel }: Props = $props();

	// What the server suggested, for the pilot to keep or change.
	// svelte-ignore state_referenced_locally
	let handle = $state(signUp.suggestedHandle ?? '');
	// svelte-ignore state_referenced_locally
	let displayName = $state(signUp.name ?? '');
	let invite = $state('');
	/** Why the account was not made, in words. */
	let refusal = $state.raw<string>();

	const byInvite = $derived(account.registration === 'invite');

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		refusal = undefined;
		refusal = await account.createGoogleAccount({
			signup: signUp.signup,
			handle,
			displayName,
			invite: byInvite ? invite : ''
		});
	}
</script>

<form onsubmit={submit} novalidate>
	<section class="group" aria-labelledby="google-handle">
		<h2 class="step-title" id="google-handle">{words.chooseHandle}</h2>
		<p class="lead">{words.chooseHandleLine}</p>
		<div class="group-card">
			<label class="field">
				<span>Handle</span>
				<input
					name="username"
					autocomplete="username"
					autocapitalize="none"
					autocorrect="off"
					spellcheck="false"
					aria-describedby={signUp.suggestionLine ? 'google-suggestion' : undefined}
					bind:value={handle}
				/>
			</label>
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
			{#if byInvite}
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
		{#if signUp.suggestionLine}
			<p class="group-note" id="google-suggestion">{signUp.suggestionLine}</p>
		{/if}
		{#if refusal}
			<p class="group-note refusal" role="alert">{refusal}</p>
		{/if}
	</section>

	<div class="submit">
		<button class="button-plain" type="button" disabled={account.working} onclick={oncancel}>
			Cancel
		</button>
		<button class="button-prominent" type="submit" disabled={account.working || handle === ''}>
			{account.working ? 'Working…' : words.createAccount}
		</button>
	</div>
</form>

<style>
	.step-title {
		padding: 0 var(--space-4);
		font-size: var(--title);
	}

	.lead {
		margin: var(--space-1) 0 var(--space-3);
		padding: 0 var(--space-4);
		color: var(--text-secondary);
	}

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		flex-direction: row-reverse;
		gap: var(--space-3);
		margin-top: var(--space-5);
	}

	.submit .button-prominent {
		flex: 1;
	}

	/* On a wide screen the button is as wide as its words, not as the form. */
	@media (min-width: 56rem) {
		.submit {
			justify-content: flex-start;
		}

		.submit .button-prominent {
			flex: none;
			min-width: 12rem;
		}
	}
</style>
