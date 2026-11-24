<!--
	Following a pilot, on that pilot's page: how many follow them and whom
	they follow, in the core's words, and for a signed-in pilot on another's
	page one button, "Follow" or "Following". Signed out, the counts alone.

	The button's word, the counts and every refusal are the core's
	(`followList`, `followAnswer`).
-->
<script lang="ts">
	import { messageOf } from '#lib/core/index.ts';
	import { getApp } from '#lib/app/app.svelte.ts';
	import FaceStack from '#lib/community/FaceStack.svelte';
	import type { FollowList } from '#lib/core/index.ts';

	interface Props {
		/** The pilot whose page this is. */
		handle: string;
	}

	let { handle }: Props = $props();

	const app = getApp();
	const social = $derived(app.social);
	const own = $derived(app.account?.account?.handle);
	const signedIn = $derived(Boolean(app.account?.signedIn));

	let follows = $state.raw<FollowList>();
	let working = $state(false);
	/** Why following did not change, in the core's words. */
	let problem = $state.raw<string>();

	// Asked again when the pilot signs in or out: the answer then says
	// whether they follow.
	$effect(() => {
		const core = social?.core;
		void signedIn;
		if (!core || !handle) return;
		let current = true;
		follows = undefined;
		problem = undefined;
		core.follows(handle).then(
			(answer) => {
				if (current) follows = answer;
			},
			// Without the counts the page is whole all the same.
			() => {}
		);
		return () => {
			current = false;
		};
	});

	/** The button is for a signed-in pilot on another pilot's page. */
	const offered = $derived(
		signedIn && own !== handle && follows?.button !== undefined && follows.youFollow !== undefined
	);

	async function toggle() {
		if (!social || !follows || working) return;
		working = true;
		problem = undefined;
		try {
			const state = await social.core.follow(handle, !follows.youFollow);
			follows = {
				...follows,
				youFollow: state.following,
				button: state.button,
				followers: state.followers,
				followersLine: state.followersLine
			};
			// The feed is of other pilots now.
			social.againFeed();
		} catch (error) {
			problem = messageOf(error);
		} finally {
			working = false;
		}
	}
</script>

{#if follows}
	<div class="follow">
		<p class="counts numeric">
			<FaceStack pilots={follows.pilots} />
			<span>{follows.followersLine}</span>
			<span aria-hidden="true">·</span>
			<span>{follows.followingLine}</span>
		</p>
		{#if offered}
			<button
				class={follows.youFollow ? 'button-plain' : 'button-prominent'}
				type="button"
				aria-pressed={follows.youFollow}
				disabled={working}
				onclick={toggle}
			>
				{follows.button}
			</button>
		{/if}
		{#if problem}
			<p class="problem" role="alert">{problem}</p>
		{/if}
	</div>
{/if}

<style>
	.follow {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-4);
		margin-top: var(--space-2);
	}

	.counts {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		font-size: var(--subheadline);
		color: var(--text-secondary);
	}

	.problem {
		flex-basis: 100%;
		font-size: var(--footnote);
		color: var(--danger);
	}
</style>
