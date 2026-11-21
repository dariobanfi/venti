<!--
	The form that gives a program access to the pilot's account: its name,
	what it may do, an end date if wanted, and the password, which the server
	asks for before it makes a token.

	Every word of it is the core's (`TokenForm`), and so is every refusal:
	the name is checked by the core when the pilot leaves its field, and the
	whole form when it is sent, before the server is asked. What comes back
	holds the token's secret; it is handed to the page that shows it once
	and kept nowhere here. The password is let go of as soon as it was sent.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { TokenCreated, TokenForm } from '#lib/core/index.ts';
	import Sheet from '#lib/design/Sheet.svelte';
	import type { AccountModel } from './model.svelte';
	import { endOfDayUnixS, firstEndDay } from './programs';

	interface Props {
		open: boolean;
		account: AccountModel;
		form: TokenForm;
		/** Access was given: the token with its secret, this once. */
		ongiven: (created: TokenCreated) => void;
		onclose: () => void;
	}

	let { open, account, form, ongiven, onclose }: Props = $props();

	const id = $props.id();

	let name = $state('');
	/** The names of the scopes switched on. */
	let chosen = $state<string[]>([]);
	let ends = $state('');
	let password = $state('');
	let sending = $state(false);
	/** Why the name cannot be one, in the core's words. */
	let nameRefused = $state.raw<string>();
	/** Why access was not given, in the core's words. */
	let refusal = $state.raw<string>();
	/** The earliest end the field offers. */
	let firstDay = $state('');

	// Only opening empties the form.
	$effect(() => {
		if (!open) return;
		untrack(() => {
			name = '';
			chosen = [];
			ends = '';
			password = '';
			sending = false;
			nameRefused = undefined;
			refusal = undefined;
			firstDay = firstEndDay(Date.now());
		});
	});

	// Closed, by any way out: the password is not held on to.
	$effect(() => {
		if (!open) password = '';
	});

	async function checkName() {
		const entered = name;
		if (entered === '') return;
		try {
			await account.checkProgramName(entered);
			if (entered === name) nameRefused = undefined;
		} catch (error) {
			if (entered === name) nameRefused = (error as Error).message;
		}
	}

	async function give(event: SubmitEvent) {
		event.preventDefault();
		if (sending) return;
		sending = true;
		refusal = undefined;
		const entered = password;
		try {
			const created = await account.giveAccess({
				name,
				scopes: [...chosen],
				endsUnixS: endOfDayUnixS(ends),
				password: entered
			});
			password = '';
			ongiven(created);
		} catch (error) {
			refusal = (error as Error).message;
		} finally {
			sending = false;
		}
	}

	function changed() {
		refusal = undefined;
		nameRefused = undefined;
	}
</script>

<Sheet {open} title={form.submit} {onclose}>
	<form onsubmit={give} oninput={changed} novalidate>
		<div class="group first">
			<div class="group-card">
				<label class="field">
					<span>{form.nameLabel}</span>
					<input
						name="program"
						autocomplete="off"
						placeholder={form.nameHint}
						maxlength={form.nameMaxLength}
						aria-describedby={nameRefused ? `${id}-name` : undefined}
						aria-invalid={nameRefused ? 'true' : undefined}
						bind:value={name}
						onchange={checkName}
					/>
				</label>
			</div>
			{#if nameRefused}
				<p class="group-note refusal" id="{id}-name" role="alert">{nameRefused}</p>
			{/if}
		</div>

		<section class="group" aria-labelledby="{id}-scopes">
			<h3 class="group-title" id="{id}-scopes">{form.scopesTitle}</h3>
			<div class="group-card">
				{#each form.scopes as scope (scope.scope)}
					<label class="group-row toggle">
						<span class="scope">
							<span>{scope.title}</span>
							<span class="line">{scope.line}</span>
						</span>
						<input type="checkbox" role="switch" value={scope.scope} bind:group={chosen} />
					</label>
				{/each}
			</div>
		</section>

		<div class="group">
			<div class="group-card">
				<label class="field">
					<span>{form.endsLabel}</span>
					<input type="date" min={firstDay} aria-describedby="{id}-ends" bind:value={ends} />
				</label>
			</div>
			<p class="group-note" id="{id}-ends">{form.endsHint}</p>
		</div>

		<div class="group">
			<div class="group-card">
				<!-- For the browser's password manager, which fills a password in for a name. -->
				<input
					class="visually-hidden"
					type="text"
					name="username"
					autocomplete="username"
					value={account.account?.handle ?? ''}
					tabindex="-1"
					aria-hidden="true"
					readonly
				/>
				<label class="field">
					<span>{form.passwordLabel}</span>
					<input
						type="password"
						name="password"
						autocomplete="current-password"
						aria-describedby="{id}-password"
						bind:value={password}
					/>
				</label>
			</div>
			<p class="group-note" id="{id}-password">{form.passwordReason}</p>
			{#if refusal}
				<p class="group-note refusal" role="alert">{refusal}</p>
			{/if}
		</div>

		<div class="submit">
			<button class="button-prominent" type="submit" disabled={sending}>{form.submit}</button>
		</div>
	</form>
</Sheet>

<style>
	.first {
		margin-top: var(--space-1);
	}

	/* What a program may do, and under it what that opens. */
	.scope {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		padding: var(--space-1) 0;
	}

	.line {
		font-size: var(--footnote);
		color: var(--text-secondary);
	}

	/* The browser draws a date with a box of its own. */
	input[type='date'] {
		font-family: var(--font-rounded);
		font-variant-numeric: tabular-nums;
	}

	.refusal {
		color: var(--danger);
	}

	.submit {
		display: flex;
		justify-content: flex-end;
		margin-top: var(--space-5);
	}
</style>
