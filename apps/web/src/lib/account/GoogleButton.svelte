<!--
	"Continue with Google", drawn as Google's "Sign in with Google branding
	guidelines" ask of a button an app draws itself
	(https://developers.google.com/identity/branding-guidelines, read
	2026-11-21). It runs nothing of Google's: what a press does is the
	caller's (`google.ts`).

	This is the one place in the app whose colours and sizes are Google's and
	not the design language's tokens, by the owner's word of 2026-11-21:

	- Light, on the light theme: fill #FFFFFF, a 1 px stroke inside the edge
	  #747775, words #1F1F1F. Dark, on the dark theme: fill #131314, stroke
	  #8E918F, words #E3E3E3.
	- 40 high and a pill; 12 from the edge to the logo, the logo in a square
	  of 20, 10 to the words, 12 from the words to the edge; the words at
	  14/20, medium. The sizes are in rem, so the whole button grows with the
	  reader's text size and keeps its proportions, which the guidelines
	  allow ("you must preserve the aspect ratio").
	- The words are in the app's own typeface, Fira Sans Medium. The
	  guidelines name Google Sans Medium; the app has no file of it and
	  takes no font from another host.
	- The "G" is Google's own file, `google-g.png`, unchanged: never redraw,
	  recolour or crop it (`AGENTS.md` says where it is from). It is 200 by
	  204, so at a height of 20 it is 19.6 wide and stands at the left of its
	  square, as in Google's own buttons.

	A wider place may stretch the button: the logo and the words stay
	together in its middle.
-->
<script lang="ts">
	import googleG from './google-g.png';

	interface Props {
		/** The button's words, the core's: "Continue with Google". */
		label: string;
		disabled?: boolean;
		onclick: () => void;
	}

	let { label, disabled = false, onclick }: Props = $props();
</script>

<button class="google-button" type="button" {disabled} {onclick}>
	<span class="logo"><img src={googleG} alt="" width="200" height="204" /></span>
	{label}
</button>

<style>
	.google-button {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.625rem;
		height: 2.5rem;
		/* The stroke is inside the edge: the 12 are measured from the edge. */
		padding: 0 calc(0.75rem - 1px);
		border: 1px solid #747775;
		border-radius: 1.25rem;
		background: #ffffff;
		color: #1f1f1f;
		font-size: 0.875rem;
		font-weight: 500;
		line-height: 1.25rem;
		white-space: nowrap;
	}

	.logo {
		display: flex;
		flex: none;
		width: 1.25rem;
		height: 1.25rem;
	}

	.logo img {
		width: auto;
		height: 100%;
	}

	.google-button:active:not(:disabled) {
		opacity: 0.7;
	}

	/* Google's dark button, in the system's dark or the one chosen in Settings. */
	@media (prefers-color-scheme: dark) {
		:global(:root:not([data-theme])) .google-button {
			border-color: #8e918f;
			background: #131314;
			color: #e3e3e3;
		}
	}

	:global(:root[data-theme='dark']) .google-button {
		border-color: #8e918f;
		background: #131314;
		color: #e3e3e3;
	}
</style>
