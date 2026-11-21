/**
 * Google's own sign-in button, from Google Identity Services: the one
 * script the app loads from another host. It gives an ID token for this
 * site's client ID, which the core sends to the Venti server
 * (`Core.googleSignInRequest`); the app keeps it no longer than that.
 *
 * The script is asked for only where the server signs pilots in with
 * Google, and the button is drawn only once it has loaded. Where it does
 * not load (offline, a blocker, a browser that refuses it) there is no
 * button, and the account screen opens its handle and password form at once.
 *
 * The content security policy names the script, its button's frame, its
 * style sheet and its requests by their paths under `accounts.google.com`
 * (`src/lib/app/security-policy.ts`).
 */

/**
 * The web client ID Google gave Venti. Not a secret: it is in every page.
 * Another build names its own with `VITE_GOOGLE_CLIENT_ID`.
 */
export const GOOGLE_CLIENT_ID: string =
	import.meta.env.VITE_GOOGLE_CLIENT_ID ||
	'490070141586-8k1a3mrggi5fb87iirrcfhq7eint5ef3.apps.googleusercontent.com';

/** Where Google Identity Services is loaded from. */
export const GOOGLE_SCRIPT = 'https://accounts.google.com/gsi/client';

/** What the button says, by Google's names for its texts. */
export type GoogleButtonText = 'continue_with' | 'signin_with' | 'signup_with' | 'signin';

/** The part of Google Identity Services the app uses. */
interface GoogleId {
	initialize(config: {
		client_id: string;
		callback: (response: { credential?: string }) => void;
		ux_mode?: 'popup' | 'redirect';
		auto_select?: boolean;
		cancel_on_tap_outside?: boolean;
		use_fedcm_for_button?: boolean;
	}): void;
	renderButton(
		parent: HTMLElement,
		options: {
			type?: 'standard' | 'icon';
			theme?: 'outline' | 'filled_blue' | 'filled_black';
			size?: 'large' | 'medium' | 'small';
			text?: GoogleButtonText;
			shape?: 'rectangular' | 'pill' | 'circle' | 'square';
			logo_alignment?: 'left' | 'center';
			width?: number;
			locale?: string;
		}
	): void;
	disableAutoSelect(): void;
}

declare global {
	interface Window {
		google?: { accounts?: { id?: GoogleId } };
	}
}

let loading: Promise<GoogleId | undefined> | undefined;

/**
 * Loads Google Identity Services once. Nothing when it does not load or
 * does not hold what the app needs.
 */
export function loadGoogle(): Promise<GoogleId | undefined> {
	loading ??= new Promise<GoogleId | undefined>((resolve) => {
		if (typeof document === 'undefined') return resolve(undefined);
		const script = document.createElement('script');
		script.src = GOOGLE_SCRIPT;
		script.async = true;
		script.onload = () => resolve(ready());
		script.onerror = () => resolve(undefined);
		document.head.append(script);
	}).then((id) => {
		// One that failed may be asked for again, on the next visit to the screen.
		if (!id) loading = undefined;
		return id;
	});
	return loading;
}

/** Google Identity Services, once its script has run, set up for this site. */
function ready(): GoogleId | undefined {
	const id = window.google?.accounts?.id;
	if (!id || typeof id.initialize !== 'function' || typeof id.renderButton !== 'function') {
		return undefined;
	}
	id.initialize({
		client_id: GOOGLE_CLIENT_ID,
		callback: (response) => {
			if (response.credential) waiting.at(-1)?.(response.credential);
		},
		ux_mode: 'popup',
		auto_select: false,
		cancel_on_tap_outside: true,
		use_fedcm_for_button: true
	});
	return id;
}

/**
 * Who is waiting for an ID token: the newest button drawn. Google calls
 * one callback for the whole page, so a button that opens over another,
 * as in the dialog that deletes an account, takes it over until it closes.
 */
const waiting: ((idToken: string) => void)[] = [];

/** Whether the page is dark, as the design pack's themes follow it. */
function dark(): boolean {
	const theme = document.documentElement.dataset.theme;
	if (theme === 'dark') return true;
	if (theme === 'light') return false;
	return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
}

/**
 * Draws Google's button into `parent`, which hands `onToken` the ID token
 * of the Google account the pilot chose. Says whether a button was drawn;
 * the returned function lets go of it.
 */
export async function drawGoogleButton(
	parent: HTMLElement,
	text: GoogleButtonText,
	onToken: (idToken: string) => void
): Promise<{ drawn: boolean; letGo: () => void }> {
	const id = await loadGoogle();
	if (!id || !parent.isConnected) return { drawn: false, letGo: () => {} };
	waiting.push(onToken);
	const draw = () => {
		parent.replaceChildren();
		id.renderButton(parent, {
			type: 'standard',
			// Google's two themes for a light and a dark page.
			theme: dark() ? 'filled_black' : 'outline',
			size: 'large',
			text,
			shape: 'pill',
			logo_alignment: 'center',
			// Google draws at most 400 px wide.
			width: Math.max(200, Math.min(400, Math.round(parent.parentElement?.clientWidth || 400)))
		});
	};
	draw();
	const scheme = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
	scheme?.addEventListener('change', draw);
	return {
		drawn: true,
		letGo: () => {
			scheme?.removeEventListener('change', draw);
			const at = waiting.lastIndexOf(onToken);
			if (at >= 0) waiting.splice(at, 1);
		}
	};
}
