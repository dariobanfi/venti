<!--
	"Alerts", under the account: the pilot's alarms, each in the core's line
	with its switch, a tap to change it; and "What fired" under them, the
	core's lines, the newest first. Both are what the browser fetched last
	(`Core.alertsHeld`, `alertsFiredHeld`), so they show without a network;
	the page fetches them anew when it opens.

	A new alarm is made from a wind station's page or a take-off's ("Alert
	me"). The headings and the empty lines are the app's: the core has none.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import AlertForm from '#lib/alerts/AlertForm.svelte';
	import { getApp } from '#lib/app/app.svelte.ts';
	import { accountHref } from '#lib/app/routes.ts';
	import type { AlertDraft, AlertView } from '#lib/core/index.ts';
	import { format } from '#lib/design/format.ts';
	import Icon from '#lib/design/Icon.svelte';

	const app = getApp();
	const account = $derived(app.account);
	const alerts = $derived(app.alerts);
	const text = format();

	// Read from the browser at once, then fetched anew; what fired counts as seen.
	$effect(() => {
		const model = alerts;
		const signedIn = account?.signedIn;
		if (!model || !signedIn) return;
		untrack(() => {
			model.seen();
			void model.reread().then(() => model.sync());
		});
	});

	let editing = $state.raw<{ id: string; draft: AlertDraft }>();
	let open = $state(false);
	/** Why an alarm was not switched, in words. */
	let problem = $state.raw<string>();
	let switching = $state<string>();

	function edit(alert: AlertView) {
		if (!alert.draft) return;
		editing = { id: alert.id, draft: alert.draft };
		open = true;
	}

	async function toggle(alert: AlertView, on: boolean) {
		if (!alerts) return;
		switching = alert.id;
		problem = undefined;
		try {
			await alerts.switch(alert, on);
		} catch (error) {
			problem = (error as Error).message;
		} finally {
			switching = undefined;
		}
	}
</script>

<svelte:head>
	<title>Alerts · Venti</title>
</svelte:head>

<div class="settings-page">
	<header>
		<a class="back-link" href={accountHref()}>
			<Icon name="back" />
			Account
		</a>
	</header>
	<h1>Alerts</h1>

	{#if app.failure}
		<p class="group-note">Venti could not open its data. {app.failure}</p>
	{:else if account && !account.signedIn}
		<p class="lead">
			Alarms live on your account. <a href={accountHref()}>Sign in</a> to set one.
		</p>
	{:else if alerts}
		{#if problem || alerts.problem}
			<p class="group-note refusal" role="alert">{problem ?? alerts.problem}</p>
		{/if}

		<section class="group" aria-labelledby="alerts-list">
			<h2 class="group-title" id="alerts-list">Your alarms</h2>
			{#if !alerts.read}
				<p class="group-note" role="status">Loading…</p>
			{:else if alerts.alerts.length === 0}
				<p class="group-card group-row none">
					No alarms yet. Set one with “Alert me” on a wind station’s page or a take-off’s.
				</p>
			{:else}
				<ul class="group-card" aria-labelledby="alerts-list">
					{#each alerts.alerts as alert (alert.id)}
						<li class="alert">
							<button
								class="about"
								type="button"
								disabled={!alert.draft}
								onclick={() => edit(alert)}
							>
								<span class="name">{alert.name}</span>
								<span class="lesser">{alert.line}</span>
							</button>
							<label class="toggle switch">
								<span class="visually-hidden">{alert.name} on</span>
								<input
									type="checkbox"
									role="switch"
									checked={alert.enabled}
									disabled={!alert.draft || switching === alert.id}
									onchange={(event) => toggle(alert, event.currentTarget.checked)}
								/>
							</label>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section class="group" aria-labelledby="fired">
			<h2 class="group-title" id="fired">What fired</h2>
			{#if alerts.read && alerts.fired.length === 0}
				<p class="group-card group-row none">Nothing has fired in the last 30 days.</p>
			{:else if alerts.fired.length > 0}
				<ul class="group-card" aria-labelledby="fired">
					{#each alerts.fired as fired (`${fired.alertId}:${fired.firedAtUnixS}`)}
						<li class="fired">
							<span>{fired.line}</span>
							<span class="lesser numeric">
								{text.dayInYear(fired.firedAtUnixS)}, {text.time(fired.firedAtUnixS)}
							</span>
						</li>
					{/each}
				</ul>
			{/if}
			<p class="group-note">
				Venti checks your alarms as the data arrives. This site fetches what fired while it is open,
				and your browser tells you if you let it.
			</p>
		</section>
	{/if}
</div>

{#if alerts && editing}
	<AlertForm {open} {alerts} id={editing.id} start={editing.draft} onclose={() => (open = false)} />
{/if}

<style>
	.lead {
		margin-top: var(--space-2);
		padding: 0 var(--space-4);
		color: var(--text-secondary);
	}

	.lead a {
		color: var(--link);
	}

	.alert {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding-right: var(--space-4);
	}

	.about {
		display: flex;
		flex: 1;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-1);
		min-width: 0;
		min-height: var(--target);
		padding: var(--space-3) var(--space-4);
		color: var(--text);
		text-align: left;
		overflow-wrap: anywhere;
	}

	.name {
		font-weight: 600;
	}

	.switch {
		display: flex;
		flex: none;
	}

	.fired {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-4);
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

	@media (hover: hover) {
		.about:not(:disabled):hover {
			background: var(--fill);
		}
	}
</style>
