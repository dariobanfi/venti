<!--
	The form for telling whoever runs the server that a flight, a pilot's page,
	a live track, a group or a plan should not be there. Each kind of thing
	has the core's form for it: a group's reasons are not a flight's.

	A report goes to the operator and nobody else, with the name of the
	account it comes from: the form says so before anything is sent. The
	reasons, the line about each, the notice and every refusal are the core's;
	the iPhone app's `ReportSheet.swift` is the same form. A browser's account
	is on the server the site is served from, which is the server the thing is
	on, so there is no "another server" to tell the pilot about.
-->
<script lang="ts">
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { ReportForm, ReportReason, ReportSubject } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';
	import { overLength } from './report';

	interface Props {
		open: boolean;
		/** What is reported. */
		subject: ReportSubject;
		/** What the form is called: "Report this flight". */
		title: string;
		onclose: () => void;
	}

	let { open, subject, title, onclose }: Props = $props();

	const app = getApp();
	const account = $derived(app.account);
	const text = format();
	const id = $props.id();

	/** What a report can say. The core's, asked for when the form first opens. */
	let form = $state.raw<ReportForm>();
	let reason = $state<ReportReason>();
	let words = $state('');
	let sending = $state(false);
	/** Why the report was not sent, in words. */
	let problem = $state.raw<string>();
	/** What became of the report, once it is sent. */
	let outcome = $state.raw<string>();

	// A form that opens starts empty: what was written about one thing is
	// not sent about another.
	$effect(() => {
		if (!open) return;
		reason = undefined;
		words = '';
		problem = undefined;
		outcome = undefined;
		form = undefined;
		void account?.reportForm(subject).then(
			(known) => (form = known),
			(error) => (problem = error instanceof Error ? error.message : String(error))
		);
	});

	/** How far the pilot's words are over what a report has room for, if they are. */
	const over = $derived(form ? overLength(words, form.textAtMost) : undefined);

	async function send(event: SubmitEvent) {
		event.preventDefault();
		if (!account || !reason || sending) return;
		sending = true;
		problem = undefined;
		try {
			outcome = await account.report(subject, reason, words);
		} catch (error) {
			problem = error instanceof Error ? error.message : String(error);
		} finally {
			sending = false;
		}
	}
</script>

<Sheet {open} {title} {onclose}>
	{#if !account}
		<p class="status" role="status">Opening…</p>
	{:else if outcome}
		<div class="group-card words">
			<p class="outcome" role="status">
				<Icon name="check" />
				{outcome}
			</p>
		</div>
	{:else if !account.signedIn}
		<div class="group-card words">
			<p>
				A report is sent from your account, so that Venti knows who it comes from. Sign in under
				Settings, Account.
			</p>
			<a class="button-plain" href={accountHref()}>Account</a>
		</div>
	{:else if form}
		<form onsubmit={send}>
			<section class="group first" aria-labelledby="{id}-what">
				<h3 class="group-title" id="{id}-what">What is wrong?</h3>
				<div class="group-card" role="radiogroup" aria-labelledby="{id}-what">
					{#each form.reasons as choice (choice.reason)}
						<label class="choice">
							<input
								type="radio"
								name="{id}-reason"
								value={choice.reason}
								aria-describedby="{id}-{choice.reason}"
								bind:group={reason}
								onchange={() => (problem = undefined)}
							/>
							<span class="pair">
								<span>{choice.title}</span>
								<span class="lesser" id="{id}-{choice.reason}">{choice.explanation}</span>
							</span>
							<span class="tick" class:chosen={reason === choice.reason}>
								<Icon name="check" />
							</span>
						</label>
					{/each}
				</div>
			</section>

			<section class="group">
				<label class="group-title" for="{id}-words">
					{reason === 'other' ? 'What is it?' : 'Anything to add?'}
				</label>
				<div class="group-card">
					<textarea
						id="{id}-words"
						rows="4"
						placeholder="In your own words"
						aria-describedby="{id}-notice"
						bind:value={words}
						oninput={() => (problem = undefined)}></textarea>
				</div>
				{#if over}
					<p class="group-note refusal" role="status">
						{text.count(over.has)} of {text.count(over.atMost)} characters
					</p>
				{/if}
				<!-- What happens to a report, said before it is sent. -->
				<p class="group-note" id="{id}-notice">{form.notice}</p>
				<!-- Why the report was not sent. It goes when the pilot changes
				     the reason or the words: it was about what they were. -->
				{#if problem}
					<p class="group-note refusal" role="alert">{problem}</p>
				{/if}
			</section>

			<div class="submit">
				<!-- Nothing can be sent before a reason is chosen. -->
				<button class="button-prominent" type="submit" disabled={!reason || sending}>
					{sending ? 'Sending…' : 'Send report'}
				</button>
			</div>
		</form>
	{:else if problem}
		<p class="status refusal" role="alert">{problem}</p>
	{:else}
		<p class="status" role="status">Opening…</p>
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

	/* The line that parts the rows of a card is not for these two: a
	   sentence and its link are one thing. */
	.words .button-plain {
		margin-left: -0.75rem;
		box-shadow: none;
	}

	.outcome {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		padding-bottom: 0.5rem;
	}

	.outcome :global(svg) {
		flex: none;
		color: var(--link);
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

	label.group-title {
		display: block;
	}

	textarea {
		display: block;
		width: 100%;
		min-height: 6rem;
		padding: 0.75rem 1rem;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		resize: vertical;
	}

	textarea::placeholder {
		color: var(--text-secondary);
	}

	textarea:focus-visible {
		outline-offset: -0.1875rem;
		border-radius: var(--radius-card);
	}

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		justify-content: flex-end;
		margin-top: 1.25rem;
	}

	@media (hover: hover) {
		.choice:hover {
			background: var(--fill);
		}
	}
</style>
