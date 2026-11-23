<!--
	"Send to a league": what a pilot who still posts to XContest or the
	DHV-XC should know before one of the pilot's flights goes there, and the
	way there. The words are the core's (`league_handover_from_file`, from
	the file fetched from the account), as on the iPhone's
	`LeagueSheet.swift`.

	A flight goes to a league from the pilot's own device and never through
	Venti. XContest takes it on its own page, from the pilot. The DHV-XC
	takes it from an app, which the web app does not do: a page could only
	call the DHV-XC if the DHV-XC let other sites call it, which is not
	known, and the upload password would have to be typed into a page.
-->
<script lang="ts">
	import type { LeagueHandover } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';
	import Sheet from '#lib/design/Sheet.svelte';

	interface Props {
		open: boolean;
		/** What the core says about the flight, once it has. */
		handover: LeagueHandover | undefined;
		/** Why it could not say, in words for the pilot. */
		problem: string | undefined;
		/** Saves the flight's file where the browser keeps downloads. */
		ondownload: () => void;
		onclose: () => void;
	}

	let { open, handover, problem, ondownload, onclose }: Props = $props();
</script>

<Sheet {open} title="Send to a league" {onclose}>
	<div class="league">
		{#if handover}
			<section class="group" aria-labelledby="league-before">
				<h2 class="group-title" id="league-before">Before it goes</h2>
				<ul class="group-card">
					<li class="group-row fact" class:caution={handover.isLate}>
						<span class="sign">
							<Icon name={handover.isLate ? 'warning' : 'calendar'} />
						</span>
						<span>
							{#if handover.isLate}<strong>Too late for the contests.</strong>{/if}
							{handover.deadline}
						</span>
					</li>
					<li class="group-row fact" class:caution={!handover.isSigned}>
						<span class="sign">
							<Icon name={handover.isSigned ? 'verified' : 'nosign'} />
						</span>
						<span>
							{#if !handover.isSigned}<strong>Not signed.</strong>{/if}
							{handover.signature}
						</span>
					</li>
				</ul>
			</section>

			<section class="group" aria-labelledby="league-xcontest">
				<h2 class="group-title" id="league-xcontest">XContest</h2>
				<div class="group-card">
					<button class="group-row action" type="button" onclick={ondownload}>
						<span>Save the flight file</span>
						<Icon name="download" />
					</button>
					<a
						class="group-row action"
						href={handover.xcontestClaimUrl}
						target="_blank"
						rel="noopener noreferrer"
					>
						<span>Open XContest’s claim page</span>
						<Icon name="link" />
					</a>
				</div>
				<p class="group-note">
					XContest takes a flight on its own page, from you: save the file, sign in there, choose
					“Claim flight” and upload it. XContest gives other apps no way to do that for you yet.
					<a href={handover.xcontestRulesUrl} target="_blank" rel="noopener noreferrer">
						XContest’s rules
					</a>
				</p>
			</section>

			<section class="group" aria-labelledby="league-dhvxc">
				<h2 class="group-title" id="league-dhvxc">DHV-XC</h2>
				<p class="group-note first">
					The web app does not send a flight to the DHV-XC: a flight goes to a league from your own
					device and never through Venti, and whether the DHV-XC lets a web page send to it is not
					known. Send it from Venti on your phone, or upload the file on the DHV-XC’s own pages.
					<a href={handover.dhvxcRulesUrl} target="_blank" rel="noopener noreferrer">
						The DHV-XC’s rules
					</a>
				</p>
			</section>
		{:else if problem}
			<p class="said" role="status">{problem}</p>
		{:else}
			<p class="said" role="status">Reading the flight…</p>
		{/if}
	</div>
</Sheet>

<style>
	.league {
		padding: 0 var(--gutter) 1.5rem;
	}

	.league > .group:first-child {
		margin-top: 0.5rem;
	}

	.fact {
		justify-content: flex-start;
		align-items: flex-start;
		gap: 0.75rem;
		padding-top: 0.75rem;
		padding-bottom: 0.75rem;
		font-size: var(--subheadline);
	}

	.sign {
		flex: none;
		color: var(--accent);
	}

	/* Status is a sign, a word and a colour: never the colour alone. */
	.caution .sign,
	.caution strong {
		color: var(--color-caution);
	}

	.action {
		color: var(--link);
	}

	.group-note a {
		color: var(--link);
		text-decoration: underline;
	}

	.first {
		padding-top: 0;
	}

	.said {
		padding: 1.5rem 0;
		color: var(--text-secondary);
	}
</style>
