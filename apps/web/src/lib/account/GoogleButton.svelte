<!--
	Google's own button, drawn by Google Identity Services into this element
	(`google.ts`): Google's branding asks for its button, so its words and
	look are Google's. It hands the ID token of the account the pilot chose
	to `ontoken`. Until Google's script has loaded there is nothing here,
	and where it does not load there is no button: `drawn` says which.
-->
<script lang="ts">
	import { drawGoogleButton, type GoogleButtonText } from './google';

	interface Props {
		/** What the button says, by Google's names for its texts. */
		text: GoogleButtonText;
		ontoken: (idToken: string) => void;
		/** Whether Google's button is on the screen: nothing until that is known. */
		drawn?: boolean;
	}

	let { text, ontoken, drawn = $bindable() }: Props = $props();

	let parent = $state<HTMLDivElement>();

	$effect(() => {
		const into = parent;
		if (!into) return;
		let letGo: (() => void) | undefined;
		let gone = false;
		void drawGoogleButton(into, text, (token) => ontoken(token)).then((button) => {
			if (gone) return button.letGo();
			letGo = button.letGo;
			drawn = button.drawn;
		});
		return () => {
			gone = true;
			letGo?.();
		};
	});
</script>

<div class="google" class:drawn bind:this={parent}></div>

<style>
	.google {
		display: flex;
		justify-content: center;
	}

	.google:not(.drawn) {
		display: none;
	}
</style>
