# For the owner: what waits for you

A short list to pick up when there is time. Each line says what to do and where the detail is; the full register of open questions is in [plan.md](plan.md#open-questions-for-the-owner) (numbers in brackets). Sessions keep this list current: a line goes when it is done, and a new one is added the day it comes up. Last gone through: 2026-11-22.

## Only you can do these

- [ ] **Push notifications** (your choice of 2026-11-23: Apple's and Google's): make an APNs key (.p8) in your Apple developer account for the app's bundle identifier, and a Firebase project with Cloud Messaging for `fly.venti.app` (its `google-services.json` and a service account for the server). Put `google-services.json` at `apps/android/app/google-services.json` (ignored by git; the Android build reads it, and without it the app has no push), the rest in `deploy/data/push/` (ignored by git) and point `VENTI_APNS_KEY_FILE`, `VENTI_APNS_KEY_ID`, `VENTI_APNS_TEAM_ID` and `VENTI_FCM_SERVICE_ACCOUNT_FILE` in `deploy/.env` at them ([running-a-server.md](running-a-server.md#push-to-the-phone-apps)). Android's half is done (2026-11-23: Firebase project `venti-ea3e9` under your personal Google account, the app's file in place, the server's key in `deploy/data/push/`, the local stack sending). Apple's half waits until you join the paid Apple Developer Program: then the APNs key, Push Notifications on the App ID, and `VENTI_ENTITLEMENTS = Config/VentiPush.entitlements` in `apps/ios/Config/Local.xcconfig`. Until then iPhones get no push.


- [ ] **Météo-France:** make a free account at its API portal and a key, so its stations can be collected (16). The adapter is not written until then.
- [ ] **Trentino:** ask the Province of Trento to bring its server's encryption up to date, so its 46 wind stations can come back (24). A draft can be written for you; nothing has been sent.
- [ ] **SLF:** register with its data service, which its terms recommend (21).
- [ ] **The other letters:** Holfuy, the DHV, the four avalanche services, Austro Control, Land Tirol (10). Drafts on request; nothing has been sent to anyone.
- [ ] **A server and HTTPS:** nothing is deployed. The assistant's sign-in (`/connect/`) cannot be tried with a real Claude or ChatGPT until Venti answers over HTTPS ([running-a-server.md](running-a-server.md)).
- [ ] **A real Google sign-in** on each app, and on the page where a pilot agrees to an assistant (8).
- [ ] **Google sign-in on Android** needs two things only you can do: an emulator image with Google Play (the one in use, `pixel6-api36`, is plain Android without Google's services, so it cannot add a Google account at all), and Android's OAuth client registered in your Google Cloud project for `fly.venti.app` with the debug build's signing certificate, its client ID passed to the build as `venti.googleClientId`. The iPhone signs in through a web page and needs neither.
- [ ] **A real phone:** nothing has run on one. The iPhone's widget needs a second bundle identifier and an app group on your developer account.
- [ ] **GitHub:** delete the stale branch `phase-0/foundations`; sessions cannot.
- [ ] **The DHV's permission in writing:** its text or a copy (who gave it, when, and whether it covers keeping the sites, handing them out through the API and the connector, and the credit to show), so that the adapter's manifest can record it ([plan.md](plan.md#owner-decisions), 22). The DHV's own terms (§ 5) forbid publishing the data unless it allows it in writing (§ 6). Also needed from the DHV: where a server fetches the site files now (the download addresses answer 404 or ask for a sign-in) and which format to take, ideally a sample.
- [ ] **XContest:** the letter about its upload key may also ask for its live positions (XContest Live); a draft can be written for you, nothing has been sent.

## Decide, yes or no (what is built stands until you say otherwise)


- [ ] **Map regions to download:** nine are built from the Alps extract (Inn valley around Innsbruck 46 MB, Chiemgau 37 MB, Bernese Oberland 35 MB, Bavarian Alps and Tyrol 314 MB, Swiss Alps 506 MB, the Alps in three thirds of 1.0 to 1.3 GB, and the whole Alps 3.1 GB), listed in `map/regions/` ([running-a-server.md](running-a-server.md#map-regions)). Keep the whole Alps beside its thirds, or not? Other valleys you fly are one small file each. Your stack lists them once their folders are in `deploy/data/maps` and the server is restarted (2026-11-23).

- [ ] **One place shown once:** where two sources describe the same take-off within 150 m, the map shows one, taken in the order Venti's own > the FFVL > OpenStreetMap > pilots' suggestions (2026-11-23). In the French Alps about 590 of the FFVL's 1,004 places meet an OpenStreetMap one.

- [ ] **A flight's times by the place it was flown at** (13). Seen on the owner's own logbook on 2026-11-22: flights flown in California show a take-off at 23:39 and at 2:56 at night, because the times are in the phone's own zone. The sessions' advice is yes; it needs a way to know a place's time zone without a network.
- [ ] **Foehn:** do "possible" from 3 hPa and "likely" from 6 hPa match what you know of the Inn valley, also for south foehn? Is Klagenfurt–Salzburg a pair pilots use? Until you say, that pair never speaks on Today (19).
- [ ] **The Alpine box's western edge** is 5.0° E, 56 km west of Grenoble; beyond it the cockpit says it is not checking airspace. Move it west? (23)
- [ ] **How a stranger's adapter reaches the repository:** the guide says a pull request; your rule of none covers your own work (17).
- [ ] **Slovenia** is asked every four minutes because its file fills in over each ten; the file itself suggests ten (25).
- [ ] **Lombardy's stations** arrive two to six hours late and are switched off. Show them as old readings instead? (18)
- [ ] **Pioupiou's condition:** while it is used, no station source may join that is not open for commercial reuse, which rules out Holfuy by permission (22).
- [ ] **Short names** "Pioupiou" (it calls itself OpenWindMap) and "South Tyrol": the names pilots know? (26)
- [ ] **The credits behind the map's "i"** run to eight lines on a small phone with large text. Shorten them only if that still satisfies each licence, which is not verified (27).
- [ ] **A suspended account** can still agree to an assistant on the server's own password form, not on the web's page (20).
- [ ] **Your local stack:** the repository's rule says to rebuild it without a database dump first; sessions have been taking one each time. Which do you want?
- [ ] **"Send to a league"** stays the main button on your own flight, because the design's flight screen has it there. Move it into the "…" menu?
- [ ] **On the iPhone a station's network** stands as a grey subtitle under the title, in the system's font; on the web it heads the "Source" card. One manner for both?
- [ ] **Icons the design pack lacks:** Android's two QR icons were drawn from memory of an open icon set, and the iPhone's radar switch uses a system symbol. The web's Community section uses Lucide's "users" for people (2026-11-23). Supply icons for a QR code, scanning one, the radar and people, or say that stand-ins are fine.
- [ ] **A real phone's camera** has never read a task's QR code on Android: the tests have none. Try it once with a code from XCTrack or from the iPhone.


## Look at, and say what is wrong

- [ ] **The first news post** (`apps/web/src/content/news/2026-11-22-welcome-to-venti.md`, marked Draft) and its picture, and the page `/about/`: rewrite or approve them, then remove `draft: true` (2026-11-23).

- [ ] Your own 101 flights (2022 to 2026), brought into both phones on 2026-11-22 from a copy in `~/code/venti-flights`: the logbook, a flight's page, and "Replay in the cockpit". Since that evening they are also in your account on the local server, all public by your word.
- [ ] **Sign in with Google** yourself, on the website and on each phone: sessions type no passwords. The local Venti takes new accounts by invite; ask a session for a code.
- [ ] The map with airspace off until switched on, and the third round button.
- [ ] Wind across Switzerland and the French Alps, and a station's row naming its network.
- [ ] The foehn line on Today and the view behind it.
- [ ] The aerial view in France (from zoom 13) and Baden-Württemberg.
- [ ] The home-screen widget on both phones.
- [ ] "Follow on the map" on a pilot in the air.
- [ ] The parity work on both phones (2026-11-22): a task as a QR code, full brightness in the cockpit (on by default, as the design says) and the airspace strip's tap on Android; the watched folder, the last-sync line and an alarm's tap on the iPhone.

## Older questions, still open

In [plan.md](plan.md#open-questions-for-the-owner): colours for the kinds of day (1), Venti's own limits for rating a day (2), the terms for contributors (3) and for reusing a public flight (4), the ranking (5), an OpenStreetMap name beside a pilot's flight (6), push to a phone (7), rules for what pilots share (9), counts from the Open Glider Network under its licence (12), a flight's times by the place's clock (13), the server's backups and contact line (14), whether a handle is permanent (15). And under [Needed from the owner](plan.md#needed-from-the-owner): live tracking, public flights, reports, groups and plans, the thermal forecast.
