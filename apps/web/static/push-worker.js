/*
 * Venti's service worker for Web Push: it shows what the server sends when
 * an alarm of the pilot's fires, also while no Venti page is open, and
 * opens the alarms' page when the notification is clicked.
 *
 * It is registered only once the pilot switches "Notify this browser" on
 * (`src/lib/alerts/push.ts`). It fetches nothing and keeps nothing: the
 * app's pages load as they do without it. What arrives is the server's
 * JSON (`docs/api.md`, "Web Push"): `title`, the alarm's name; `line`, what
 * happened, in the words the apps show; `alert_id` and `fired_at`.
 */

/** The alarms' page, beside this file: the app may be served under a path. */
const ALERTS = new URL('settings/account/alerts/#fired', self.registration.scope).href;

self.addEventListener('push', (event) => {
	let told = {};
	try {
		told = event.data ? event.data.json() : {};
	} catch {
		// A message that is no JSON still tells the pilot that something fired.
	}
	const title = typeof told.title === 'string' && told.title ? told.title : 'Venti';
	const body = typeof told.line === 'string' ? told.line : '';
	const tag = typeof told.alert_id === 'string' ? told.alert_id : undefined;
	event.waitUntil(self.registration.showNotification(title, { body, tag }));
});

self.addEventListener('notificationclick', (event) => {
	event.notification.close();
	event.waitUntil(
		self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windows) => {
			for (const open of windows) {
				if ('navigate' in open && 'focus' in open) {
					return open.navigate(ALERTS).then((there) => (there ?? open).focus());
				}
			}
			return self.clients.openWindow(ALERTS);
		})
	);
});
