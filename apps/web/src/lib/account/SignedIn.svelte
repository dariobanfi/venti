<!--
	The account this browser is signed in to: who it is, the way to the
	pilot's flights on it, what else it holds, and the ways out: sign out,
	and delete the account. The iPhone app's `SignedInView` is its twin.

	Every refusal here is the core's.

	Where the server signs pilots in with Google: the Google account's email
	the account is linked to, shown to the pilot alone, with the core's
	"Unlink Google"; or the core's "Link Google", which takes the pilot to
	Google (`google.ts`) and back. An account linked to Google can be deleted
	by confirming with Google, in place of a password it may not have: the
	core's "Continue with Google" in the dialog goes to Google, and the
	account is deleted when Google comes back.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { alertsHref, myFlightsHref, notificationsHref, programsHref } from '#lib/app/routes.ts';
	import { countries } from '#lib/community/country.ts';
	import Flag from '#lib/community/Flag.svelte';
	import type { AccountView, GliderClassKind } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Dialog from '#lib/design/Dialog.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import { goToGoogle } from './google';
	import GoogleButton from './GoogleButton.svelte';
	import type { AccountModel } from './model.svelte';

	const app = getApp();

	interface Props {
		account: AccountModel;
		signedIn: AccountView;
	}

	let { account, signedIn }: Props = $props();

	/** What the alarms fired that no notification told of. */
	const unseen = $derived(getApp().alerts?.unseen ?? 0);

	/** The core's words around follows, cheers and comments: the row's name. */
	const said = $derived(getApp().social?.words);

	/** The server as the pilot knows it: its host name. */
	const host = $derived(hostOf(signedIn.server));

	function hostOf(address: string): string {
		try {
			return new URL(address).host;
		} catch {
			return address;
		}
	}

	/** Every country, by name, for the picker. */
	const everyCountry = countries();
	/** The glider classes in the core's names, "None" first. */
	const classes = $derived(app.said?.gliderClasses ?? []);
	/** Why the country or the class was not set, in words. */
	let profileRefusal = $state.raw<string>();

	async function chooseCountry(code: string) {
		profileRefusal = await account.setCountry(code || undefined);
	}

	async function chooseClass(kind: string) {
		profileRefusal = await account.setGliderClass(
			(kind || undefined) as GliderClassKind | undefined
		);
	}

	let confirmingSignOut = $state(false);
	let deleting = $state(false);
	let password = $state('');
	/** Why the account was not deleted, in words. */
	let refusal = $state.raw<string>();

	/** Why unlinking Google did not work, in words. */
	let unlinkRefusal = $state.raw<string>();
	/** Why what the pilot went to Google for did not work, in words, or why unlinking did not. */
	const googleRefusal = $derived(unlinkRefusal ?? account.googleRefusal?.words);

	async function unlinkGoogle() {
		account.googleRefusal = undefined;
		unlinkRefusal = await account.unlinkGoogle();
	}

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

<!-- What is shown with the pilot's name and flights, and what new flights take. -->
<section class="group" aria-labelledby="account-profile">
	<h2 class="group-title" id="account-profile">Shown with your flights</h2>
	<div class="group-card">
		<label class="group-row setting">
			<span>Country</span>
			<span class="picked">
				<Flag code={signedIn.country} />
				<select
					value={signedIn.country ?? ''}
					disabled={account.working || signedIn.needsSignIn}
					onchange={(event) => chooseCountry(event.currentTarget.value)}
				>
					<option value="">{classes[0]?.name ?? 'None'}</option>
					{#each everyCountry as country (country.code)}
						<option value={country.code}>{country.name}</option>
					{/each}
				</select>
			</span>
		</label>
		<label class="group-row setting">
			<span>{app.said?.glider.classTitle ?? 'Glider class'}</span>
			<select
				value={signedIn.gliderClass ?? ''}
				disabled={account.working || signedIn.needsSignIn}
				onchange={(event) => chooseClass(event.currentTarget.value)}
			>
				{#each classes as one (one.kind ?? '')}
					<option value={one.kind ?? ''}>{one.name}</option>
				{/each}
			</select>
		</label>
	</div>
	<p class="group-note">
		Your country’s flag stands beside your name on your public flights and your page. Flights you
		bring in from now on take the glider’s class.
	</p>
	{#if profileRefusal}
		<p class="group-note refusal" role="alert">{profileRefusal}</p>
	{/if}
</section>

{#if account.googleWords && (signedIn.googleEmail || account.googleSignIn)}
	{@const words = account.googleWords}
	<section class="group" aria-labelledby="account-google">
		<h2 class="group-title" id="account-google">Google</h2>
		<div class="group-card">
			{#if signedIn.googleEmail}
				<p class="group-row google-email">{signedIn.googleEmail}</p>
				<button
					class="group-row"
					type="button"
					disabled={account.working || signedIn.needsSignIn}
					onclick={unlinkGoogle}
				>
					{words.unlinkGoogle}
				</button>
			{:else}
				<button
					class="group-row"
					type="button"
					disabled={account.working || signedIn.needsSignIn}
					onclick={() => goToGoogle('link')}
				>
					{words.linkGoogle}
				</button>
			{/if}
		</div>
		{#if googleRefusal}
			<p class="group-note refusal" role="alert">{googleRefusal}</p>
		{/if}
	</section>
{/if}

{#if signedIn.needsSignIn}
	<section class="group" aria-label="Session">
		<p class="group-card group-row group-warning" role="alert">
			<Icon name="warning" />
			<span>This browser was signed out. Sign out here and sign in again.</span>
		</p>
	</section>
{/if}

<!-- What else the account holds. The row's name is the core's, as the page's title is. -->
<section class="group" aria-label="Access">
	<div class="group-card">
		<!-- The pilot's flights are the account's: the website keeps none of its own. -->
		<a class="group-row" href={myFlightsHref()}>
			<span>My flights</span>
			<span class="chevron"><Icon name="forward" /></span>
		</a>
		<a class="group-row" href={alertsHref()}>
			<span>Alerts</span>
			<span class="trailing">
				{#if unseen > 0}
					<span class="badge numeric" aria-label="{unseen} new">{unseen}</span>
				{/if}
				<span class="chevron"><Icon name="forward" /></span>
			</span>
		</a>
		<!-- What the pilot was told of follows, cheers and comments. -->
		{#if said}
			<a class="group-row" href={notificationsHref()}>
				<span>{said.notificationsTitle}</span>
				<span class="chevron"><Icon name="forward" /></span>
			</a>
		{/if}
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
	message="Your flights stay on your Venti account."
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
				links stop working. Enter your password to go on.
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
			{#if signedIn.googleEmail && deleting}
				<p class="with-google">An account linked to Google may confirm with Google instead.</p>
				{#if account.googleWords}
					<div class="with-google-button">
						<GoogleButton
							label={account.googleWords.continueWithGoogle}
							disabled={account.working}
							onclick={() => goToGoogle('delete')}
						/>
					</div>
				{/if}
			{/if}
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
			<button class="choice cancel" type="button" onclick={() => (deleting = false)}
				>{app.said?.common.cancel}</button
			>
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

	/* A choice in a row: the menu at the row's end. */
	.setting select {
		max-width: 60%;
		padding: var(--space-1) var(--space-2);
		border: 0;
		border-radius: var(--radius-control);
		background: var(--fill);
		color: var(--text);
		font: inherit;
	}

	.picked {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-2);
		min-width: 0;
		max-width: 60%;
	}

	.picked select {
		max-width: 100%;
	}

	.google-email {
		overflow-wrap: anywhere;
	}

	/* Google's button, as wide as the dialog. */
	.with-google-button {
		display: flex;
		flex-direction: column;
	}

	.with-google {
		margin-block: var(--space-3) var(--space-2);
		color: var(--text-secondary);
	}

	.refusal {
		color: var(--danger);
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
