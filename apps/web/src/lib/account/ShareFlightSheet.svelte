<!--
	Who can see a flight, and the link to it: the iPhone app's
	`ShareFlightSheet`. Nothing is shared until the pilot says so here.

	Which settings exist and what the link is come from the core. This sheet
	shows them, passes the pilot's choice on, and copies the link.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { CommentsSwitch, FlightSharing, FlightVisibility } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { VISIBILITY_CHOICES } from './visibility';

	interface Props {
		open: boolean;
		/** The flight's identifier in the logbook. */
		flight: string;
		onclose: () => void;
	}

	let { open, flight, onclose }: Props = $props();

	const app = getApp();
	const account = $derived(app.account);

	/** How the flight is shared, once the core has said. Nothing if the server does not have it. */
	let sharing = $state.raw<FlightSharing>();
	/** Whether the core has been asked since the sheet opened. */
	let asked = $state(false);
	/** Why the last change did not happen, in words. */
	let refusal = $state.raw<string>();
	let copied = $state<'yes' | 'no'>();

	// What the core knows is read each time the sheet opens: a sync may have
	// sent the flight since.
	$effect(() => {
		if (!open || !account) return;
		let current = true;
		asked = false;
		refusal = undefined;
		copied = undefined;
		void account.sharing(flight).then((known) => {
			if (!current) return;
			sharing = known;
			asked = true;
		});
		return () => {
			current = false;
		};
	});

	/*
	 * Whether a public flight takes comments: the core's switch, its title
	 * and its line, for the flight by its address on the server.
	 */
	const social = $derived(app.social);
	const serverId = $derived(sharing?.visibility === 'public' ? sharing.flightId : '');
	let commenting = $state.raw<CommentsSwitch>();
	let commentingRefusal = $state.raw<string>();
	$effect(() => {
		const core = social?.core;
		const id = serverId;
		commenting = undefined;
		commentingRefusal = undefined;
		if (!open || !core || !id) return;
		let current = true;
		void core
			.comments(id)
			.then((list) => core.commentsSwitch(list.open))
			.then(
				(known) => {
					if (current) commenting = known;
				},
				// Without the answer there is no switch.
				() => {}
			);
		return () => {
			current = false;
		};
	});

	async function takeComments(on: boolean) {
		if (!social || !serverId) return;
		commentingRefusal = undefined;
		try {
			commenting = await social.core.setCommentsOpen(serverId, on);
		} catch (error) {
			commentingRefusal = error instanceof Error ? error.message : String(error);
			// The switch goes back to what holds.
			commenting = commenting && { ...commenting };
		}
	}

	let choices = $state<HTMLDivElement>();

	async function choose(visibility: FlightVisibility) {
		if (!account || sharing?.visibility === visibility) return;
		refusal = undefined;
		copied = undefined;
		try {
			sharing = await account.share(flight, visibility);
		} catch (error) {
			refusal = error instanceof Error ? error.message : String(error);
		}
		// The browser ticked what was clicked. If the server did not follow,
		// the buttons go back to what holds.
		for (const button of choices?.querySelectorAll('input') ?? []) {
			button.checked = button.value === sharing?.visibility;
		}
	}

	async function syncNow() {
		if (!account) return;
		await account.sync({ force: true, quietly: false });
		sharing = await account.sharing(flight);
	}

	async function copyLink() {
		if (!sharing) return;
		try {
			await navigator.clipboard.writeText(sharing.link);
			copied = 'yes';
		} catch {
			// Browsers hand the clipboard only to pages served securely.
			copied = 'no';
		}
	}
</script>

<Sheet {open} title="Share flight" {onclose}>
	{#if !account}
		<p class="status" role="status">Opening…</p>
	{:else if !account.signedIn}
		<div class="group-card words">
			<p>To share a flight with a link, sign in to Venti under Settings, Account.</p>
			<a class="button-plain" href={accountHref()}>Account</a>
		</div>
	{:else if sharing}
		<section class="group first" aria-labelledby="share-who">
			<h3 class="group-title" id="share-who">Who can see this flight</h3>
			<div class="group-card" role="radiogroup" aria-labelledby="share-who" bind:this={choices}>
				{#each VISIBILITY_CHOICES as choice (choice.visibility)}
					{@const chosen = sharing.visibility === choice.visibility}
					<label class="choice" class:working={account.working}>
						<input
							type="radio"
							name="visibility"
							value={choice.visibility}
							checked={chosen}
							aria-describedby="share-{choice.visibility}"
							onchange={() => choose(choice.visibility)}
						/>
						<span class="pair">
							<span>{choice.title}</span>
							<span class="lesser" id="share-{choice.visibility}">{choice.explanation}</span>
						</span>
						<span class="tick" class:chosen><Icon name="check" /></span>
					</label>
				{/each}
			</div>
			{#if refusal}
				<p class="group-note refusal" role="alert">{refusal}</p>
			{/if}
		</section>

		{#if commenting}
			{#key commenting}
				<section class="group" aria-label={commenting.title}>
					<div class="group-card">
						<label class="group-row toggle">
							<span>{commenting.title}</span>
							<input
								type="checkbox"
								role="switch"
								aria-describedby="share-comments"
								checked={commenting.open}
								onchange={(event) => takeComments(event.currentTarget.checked)}
							/>
						</label>
					</div>
					<p class="group-note" id="share-comments">{commenting.line}</p>
					{#if commentingRefusal}
						<p class="group-note refusal" role="alert">{commentingRefusal}</p>
					{/if}
				</section>
			{/key}
		{/if}

		{#if sharing.visibility !== 'private'}
			<section class="group" aria-labelledby="share-link">
				<h3 class="group-title" id="share-link">Link</h3>
				<div class="group-card">
					<a class="group-row link" href={sharing.link} target="_blank" rel="noopener">
						{sharing.link}
					</a>
					<button class="group-row" type="button" onclick={copyLink}>
						Copy the link
						<Icon name={copied === 'yes' ? 'check' : 'copy'} />
					</button>
				</div>
				<p class="group-note" role="status">
					{#if copied === 'yes'}
						Copied.
					{:else if copied === 'no'}
						This browser did not let the page copy. Select the link above and copy it.
					{/if}
				</p>
			</section>
		{/if}
	{:else if asked}
		<div class="group-card words">
			<p>This flight is not on Venti yet.</p>
			{#if account.problem}
				<p class="refusal" role="alert">{account.problem}</p>
			{/if}
			<button class="button-plain" type="button" disabled={account.working} onclick={syncNow}>
				{account.working ? 'Working…' : 'Sync now'}
			</button>
		</div>
	{/if}
</Sheet>

<style>
	.first {
		margin-top: 0.25rem;
	}

	/* A few words and one thing to do about them. */
	.words {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.25rem;
		margin-top: 0.25rem;
		padding: 1rem 1rem 0.5rem;
	}

	.words .button-plain {
		margin-left: -0.75rem;
	}

	.status {
		padding: 1rem;
		color: var(--text-secondary);
	}

	.choice {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: var(--target);
		padding: 0.625rem 1rem;
		cursor: pointer;
	}

	.choice.working {
		cursor: default;
		opacity: 0.6;
	}

	/* The radio button itself is not drawn, but it takes the keyboard: the
	   tick at the end of the row says which one is chosen. */
	.choice input {
		position: absolute;
		opacity: 0;
	}

	.choice:has(:focus-visible) {
		outline: 0.1875rem solid var(--color-focus);
		outline-offset: -0.1875rem;
		border-radius: 0.75rem;
	}

	.pair {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		min-width: 0;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.tick {
		color: var(--link);
		visibility: hidden;
	}

	.tick.chosen {
		visibility: visible;
	}

	.link {
		color: var(--link);
		font-size: var(--footnote);
		/* An address has no spaces to break at. */
		overflow-wrap: anywhere;
		user-select: all;
	}

	button.group-row :global(svg) {
		font-size: 0.875em;
	}

	.refusal {
		color: var(--danger);
	}

	@media (hover: hover) {
		.choice:not(.working):hover {
			background: var(--fill);
		}
	}
</style>
