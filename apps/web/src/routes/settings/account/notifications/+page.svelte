<!--
	"Notifications", under the account: what the pilot was told, in the
	order the core gives it: who follows them, who cheered a flight of
	theirs, who wrote under one. Each line is the core's (`notifications`), with its age; it opens
	the flight where it is about one, and the pilot's page otherwise.

	Fetched when the page opens and kept nowhere. Once shown, the marker on
	the pilot's picture moves past what was shown. Nothing is pushed.
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref, pilotHref, publicFlightHref } from '#lib/app/routes.ts';
	import type { Notifications } from '#lib/core/index.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const account = $derived(app.account);
	const social = $derived(app.social);
	const said = $derived(social?.words);
	const signedIn = $derived(Boolean(account?.signedIn));

	let told = $state.raw<Notifications>();
	/** Why the list is missing, in words. */
	let problem = $state.raw<string>();
	let asked = $state(0);

	$effect(() => {
		const core = social?.core;
		void asked;
		if (!core || !signedIn) return;
		let current = true;
		problem = undefined;
		core.notifications().then(
			(answer) => {
				if (!current) return;
				told = answer;
				// Shown: the marker of what is new moves past it, the alarms' as it is.
				if (answer) void app.unread?.seen({ notifications: answer.cursor });
			},
			(error) => {
				// The core's line that the list could not be loaded, then why.
				const why = messageOf(error);
				if (current) problem = [said?.notificationsFailed, why].filter(Boolean).join(' ');
			}
		);
		return () => {
			current = false;
		};
	});
</script>

<svelte:head>
	<title>{said?.notificationsTitle ?? 'Venti'} · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={accountHref()}>
			<Icon name="back" />
			Account
		</a>
	</header>
	<h1>{said?.notificationsTitle ?? ''}</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if account && !signedIn}
		<p class="lead"><a href={accountHref()}>Account</a></p>
	{:else if problem}
		<p class="group-note refusal" role="alert">{problem}</p>
		<div class="again">
			<button class="button-plain" type="button" onclick={() => (asked += 1)}>
				{app.said?.common.tryAgain}
			</button>
		</div>
	{:else if !told}
		<p class="group-note" role="status">Loading…</p>
	{:else if told.lines.length === 0}
		<p class="group-card group-row none">{told.emptyLine}</p>
	{:else}
		<ul class="group-card" aria-label={said?.notificationsTitle}>
			{#each [...told.lines].reverse() as line (`${line.atUnixS}:${line.handle}:${line.flightId ?? ''}:${line.commentId ?? ''}:${line.about}`)}
				<li>
					<a
						class="group-row told"
						href={line.flightId ? publicFlightHref(line.flightId) : pilotHref(line.handle)}
					>
						<span class="words">
							<span>{line.line}</span>
							<span class="lesser numeric">{line.age}</span>
						</span>
						<span class="chevron"><Icon name="forward" /></span>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.lead {
		margin-top: var(--space-2);
		padding: 0 var(--space-4);
	}

	.lead a {
		color: var(--link);
	}

	.told {
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.words {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.lesser {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	.none {
		color: var(--text-secondary);
	}

	.refusal {
		color: var(--danger);
	}

	.again {
		padding: var(--space-2) var(--space-4);
	}
</style>
