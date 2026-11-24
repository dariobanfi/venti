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
	import PilotFace from '#lib/community/PilotFace.svelte';
	import type { AccountView, FlightVisibility, GliderClassKind } from '#lib/core/index.ts';
	import ConfirmDialog from '#lib/design/ConfirmDialog.svelte';
	import Dialog from '#lib/design/Dialog.svelte';
	import Icon from '#lib/design/Icon.svelte';
	import { goToGoogle } from './google';
	import { PICTURE_KINDS, pictureToSend } from './picture';
	import GoogleButton from './GoogleButton.svelte';
	import type { AccountModel } from './model.svelte';

	const app = getApp();

	interface Props {
		account: AccountModel;
		signedIn: AccountView;
	}

	let { account, signedIn }: Props = $props();

	/** What is new on the account, on every device: the server's count. */
	const unread = $derived(getApp().unread);

	/** The core's words around follows, cheers and comments: the row's name. */
	const said = $derived(getApp().social?.words);
	// The alarms' row is named in the core's words, as the alarms' page is.
	$effect(() => void app.alerts?.words().catch(() => {}));

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

	/** The core's words of the pilot's picture and of the pilots near a place. */
	const pictureWords = $derived(app.said?.picture);
	/** Why the picture did not change, or the switch of the pilots near did not, in words. */
	let pictureRefusal = $state.raw<string>();
	let shownNearRefusal = $state.raw<string>();
	/** While a chosen picture is made ready and sent. */
	let sendingPicture = $state(false);
	let picker = $state<HTMLInputElement>();

	async function choosePicture(input: HTMLInputElement) {
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		sendingPicture = true;
		try {
			const { bytes, mediaType } = await pictureToSend(file);
			pictureRefusal = await account.setPicture(bytes, mediaType);
		} finally {
			sendingPicture = false;
		}
	}

	async function removePicture() {
		pictureRefusal = await account.removePicture();
	}

	async function switchShownNear(shown: boolean) {
		shownNearRefusal = await account.setShownNearPlaces(shown);
	}

	/** The core's words of who sees the pilot's flights. */
	const visibilityWords = $derived(app.said?.flightVisibility);
	/** Read as on for an account an earlier build stored, as the server has it. */
	const newFlightsPublic = $derived(signedIn.newFlightsPublic ?? true);
	/** "Make all public" or "Make all private" while its question is asked, and the question. */
	let asking = $state.raw<{ visibility: FlightVisibility; question: string }>();
	/** Why who sees the flights did not change, in words. */
	let visibilityRefusal = $state.raw<string>();

	async function switchNewFlights(open: boolean) {
		visibilityRefusal = await account.setNewFlightsPublic(open);
	}

	/**
	 * Asks the core's question before every flight changes: with their
	 * number where My flights has read every page, else without one.
	 */
	async function askAllFlights(visibility: FlightVisibility) {
		const mine = app.myFlights;
		const known = mine?.phase.is === 'loaded' && !mine.next ? mine.flights.length : 0;
		asking = { visibility, question: await account.allFlightsQuestion(known, visibility) };
	}

	async function setAllFlights() {
		const visibility = asking?.visibility;
		asking = undefined;
		if (!visibility) return;
		visibilityRefusal = await account.setAllFlights(visibility);
		// My flights shows each flight as the server now has it.
		if (!visibilityRefusal) void app.myFlights?.reload();
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
	<!-- One row: the pilot's picture, public beside their name wherever it
	     stands, with the pencil that changes it; the name and the handle. -->
	<div class="group-card who" aria-label="Picture" role="group">
		<div class="picture">
			{#if pictureWords}
				<button
					class="change"
					type="button"
					aria-label={pictureWords.change}
					aria-busy={sendingPicture}
					disabled={account.busy || sendingPicture || signedIn.needsSignIn}
					onclick={() => picker?.click()}
				>
					<PilotFace picture={signedIn.picturePath} name={signedIn.displayName} size="m" />
					<span class="pencil"><Icon name="pencil" /></span>
				</button>
				<input
					class="visually-hidden"
					type="file"
					accept={PICTURE_KINDS}
					tabindex="-1"
					aria-label={pictureWords.change}
					bind:this={picker}
					onchange={(event) => choosePicture(event.currentTarget)}
				/>
			{:else}
				<PilotFace picture={signedIn.picturePath} name={signedIn.displayName} size="m" />
			{/if}
		</div>
		<dl class="names">
			<div>
				<dt class="visually-hidden">Name</dt>
				<dd class="name">{signedIn.displayName}</dd>
			</div>
			<div>
				<dt class="visually-hidden">Handle</dt>
				<dd class="handle">@{signedIn.handle}</dd>
			</div>
		</dl>
		{#if sendingPicture}
			<span class="quiet" role="status">Sending…</span>
		{:else if pictureWords && signedIn.picturePath}
			<button
				class="remove"
				type="button"
				disabled={account.busy || signedIn.needsSignIn}
				onclick={removePicture}
			>
				{pictureWords.remove}
			</button>
		{/if}
	</div>
	{#if pictureWords}
		<p class="group-note">{pictureWords.publicLine}</p>
	{/if}
	{#if pictureRefusal}
		<p class="group-note refusal" role="alert">{pictureRefusal}</p>
	{/if}
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
					disabled={account.busy || signedIn.needsSignIn}
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
			<span>{app.said?.glider.classTitle}</span>
			<select
				value={signedIn.gliderClass ?? ''}
				disabled={account.busy || signedIn.needsSignIn}
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
					disabled={account.busy || signedIn.needsSignIn}
					onclick={unlinkGoogle}
				>
					{words.unlinkGoogle}
				</button>
			{:else}
				<button
					class="group-row"
					type="button"
					disabled={account.busy || signedIn.needsSignIn}
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

<!-- The pilot's flights are the account's: the website keeps none of its own. -->
<section class="group" aria-label="Logbook">
	<div class="group-card">
		<a class="group-row" href={myFlightsHref()}>
			<span>My flights</span>
			<span class="chevron"><Icon name="forward" /></span>
		</a>
	</div>
</section>

<!-- Who sees the pilot's flights: every word is the core's. -->
{#if visibilityWords}
	<section class="group" aria-label="Who sees your flights">
		<div class="group-card">
			<label class="group-row toggle">
				<span class="lines">
					<span>{visibilityWords.newFlightsPublic}</span>
					<span class="line" id="account-new-flights-line"
						>{visibilityWords.newFlightsPublicLine}</span
					>
				</span>
				<input
					type="checkbox"
					role="switch"
					aria-describedby="account-new-flights-line"
					checked={newFlightsPublic}
					disabled={account.busy || signedIn.needsSignIn}
					onchange={(event) => switchNewFlights(event.currentTarget.checked)}
				/>
			</label>
			{#if pictureWords}
				<label class="group-row toggle">
					<span class="lines">
						<span>{pictureWords.shownNearPlaces}</span>
						<span class="line" id="account-shown-near-line">{pictureWords.shownNearPlacesLine}</span
						>
					</span>
					<input
						type="checkbox"
						role="switch"
						aria-describedby="account-shown-near-line"
						checked={signedIn.shownNearPlaces ?? true}
						disabled={account.busy || signedIn.needsSignIn}
						onchange={(event) => switchShownNear(event.currentTarget.checked)}
					/>
				</label>
			{/if}
			<button
				class="group-row action"
				type="button"
				disabled={account.busy || signedIn.needsSignIn}
				onclick={() => askAllFlights('public')}
			>
				{visibilityWords.makeAllPublic}
			</button>
			<button
				class="group-row action"
				type="button"
				disabled={account.busy || signedIn.needsSignIn}
				onclick={() => askAllFlights('private')}
			>
				{visibilityWords.makeAllPrivate}
			</button>
		</div>
		{#if shownNearRefusal}
			<p class="group-note refusal" role="alert">{shownNearRefusal}</p>
		{/if}
		{#if visibilityRefusal}
			<p class="group-note refusal" role="alert">{visibilityRefusal}</p>
		{:else if account.allFlights}
			<p class="group-note" role="status">{account.allFlights.line}</p>
		{/if}
	</section>
{/if}

<ConfirmDialog
	open={asking !== undefined}
	title={asking?.question ?? ''}
	confirm={asking?.visibility === 'private'
		? (visibilityWords?.makeAllPrivate ?? '')
		: (visibilityWords?.makeAllPublic ?? '')}
	destructive={false}
	onconfirm={setAllFlights}
	oncancel={() => (asking = undefined)}
/>

{#if signedIn.needsSignIn}
	<section class="group" aria-label="Session">
		<p class="group-card group-row group-warning" role="alert">
			<Icon name="warning" />
			<span>This browser was signed out. Sign out here and sign in again.</span>
		</p>
	</section>
{/if}

<!-- What else the account holds, in the phones' order. The rows' names are the core's, as the pages' titles are. -->
<section class="group" aria-label="Access">
	<div class="group-card">
		{#await account.programsForm() then programs}
			<a class="group-row" href={programsHref()}>
				<span>{programs.title}</span>
				<span class="chevron"><Icon name="forward" /></span>
			</a>
		{:catch}
			<!-- Without the core's words there is no row. -->
		{/await}
		<a class="group-row" href={alertsHref()}>
			<span>{app.alerts?.said?.title ?? 'Alerts'}</span>
			<span class="trailing">
				{#if unread && unread.alarms > 0}
					<span class="badge numeric" aria-label="{unread.alarms} new">{unread.alarms}</span>
				{/if}
				<span class="chevron"><Icon name="forward" /></span>
			</span>
		</a>
		<!-- What the pilot was told of follows, cheers and comments. -->
		{#if said}
			<a class="group-row" href={notificationsHref()}>
				<span>{said.notificationsTitle}</span>
				<span class="trailing">
					{#if unread && unread.notifications > 0}
						<span class="badge numeric" aria-label="{unread.notifications} new"
							>{unread.notifications}</span
						>
					{/if}
					<span class="chevron"><Icon name="forward" /></span>
				</span>
			</a>
		{/if}
	</div>
</section>

<section class="group" aria-label="Leaving">
	<div class="group-card">
		<button
			class="group-row"
			type="button"
			disabled={account.busy}
			onclick={() => (confirmingSignOut = true)}
		>
			Sign out
		</button>
		<button
			class="group-row destructive"
			type="button"
			disabled={account.busy}
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
				links stop working. {signedIn.googleEmail
					? 'Enter your password, or confirm with Google, to go on.'
					: 'Enter your password to go on.'}
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
				{#if account.googleWords}
					<div class="with-google-button">
						<GoogleButton
							label={account.googleWords.continueWithGoogle}
							disabled={account.busy}
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
			<button class="choice destructive" type="submit" disabled={account.busy || password === ''}>
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

	/* The switch's name with its one line under it. */
	.lines {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}

	.line {
		color: var(--text-secondary);
		font-size: var(--footnote);
	}

	.action {
		color: var(--link);
	}

	/* One row: the picture, the name over the handle, and the quiet way
	   to take the picture away while there is one. */
	.who {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
	}

	/* No hairlines inside the row: it is one row. */
	.who > *,
	.names > * {
		box-shadow: none;
	}

	.picture {
		flex: none;
		display: flex;
	}

	/* The whole face is the button, the pencil on its corner says what it does. */
	.change {
		position: relative;
		display: flex;
		border-radius: var(--radius-pill);
	}

	.change:disabled {
		opacity: 0.6;
	}

	.pencil {
		position: absolute;
		right: calc(-1 * var(--space-1));
		bottom: calc(-1 * var(--space-1));
		display: grid;
		place-items: center;
		width: var(--size-icon);
		height: var(--size-icon);
		border-radius: var(--radius-pill);
		border: 2px solid var(--surface);
		background: var(--accent);
		color: var(--on-accent);
	}

	.pencil :global(svg) {
		width: 0.875rem;
		height: 0.875rem;
	}

	.names {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		margin: 0;
	}

	.names dd {
		margin: 0;
		overflow-wrap: anywhere;
	}

	.name {
		font-weight: 600;
		color: var(--text);
	}

	.handle,
	.quiet {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.remove {
		flex: none;
		min-height: var(--target);
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.google-email {
		overflow-wrap: anywhere;
	}

	/* Google's button, as wide as the dialog. */
	.with-google-button {
		display: flex;
		flex-direction: column;
		margin-block: var(--space-3) var(--space-2);
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
