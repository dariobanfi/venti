# Design

Last revised: 2026-11-19

How Venti looks and behaves, on every platform. The iOS app is the first place these rules are built; Android and the web follow the same structure, colours and wording with their own platform's components. The web app is the second and the Android app the third; what is particular to each is under [Web](#web) and [Android](#android).

A change to a screen that contradicts this document updates it in the same commit.

## Principles

1. **The map and the flight are the content. Controls float above them.** Translucent "glass" surfaces are for navigation and controls only: bars, the flight panel, map buttons. Lists, forms and empty states use plain system backgrounds. This is Apple's own rule for its current design ("Don't use Liquid Glass in the content layer") and it is what keeps a map readable.
2. **System components first.** Tab bar, navigation bar, lists, sheets, alerts, menus and symbols are the platform's own, unstyled. They follow light and dark mode, text size, contrast and transparency settings without code of ours, and they change with the system when it changes. We draw our own component only where the platform has none: the track thumbnail, the metric, the flight panel, the climb colours.
3. **One line per flight, and it is boringly obvious.** A logbook row answers "which flight, when, how long, how far" without being opened. Flighty's designers describe their flight rows the same way: airport boards have had fifty years to work out what matters.
4. **Numbers are set, not just printed.** A value is large, in the rounded system face with fixed-width digits; its unit is small and grey beside it; what it measures is a caption below. A number that changes under the finger must not jitter sideways.
5. **Nothing important hides behind a gesture.** The headline numbers of a flight are on screen before anything is touched. Dragging along the chart adds detail; it reveals nothing essential.
6. **Colour carries meaning or it is not used.** There is one accent, one scale for climb and sink, and (later) one set of status colours for "flyable, marginal, not flyable". Text is never set in a data colour.
7. **It works with no connection and in sunlight.** Fonts are inside the app. Every screen has a state for "nothing here yet" that says what to do next. The cockpit gets its own stricter rules when it is built (below).

## Structure

| Tab | Today | Later |
|---|---|---|
| **Map** | Terrain map with every logbook flight drawn on it; the pilot's position on request | Live wind, sites, forecasts, airspace: the Today and Plan modes |
| **Logbook** | Flights by year with totals; import; one flight on the map with its profile | Analysis, comparison, 3D replay: the Review mode |
| **Fly** | The cockpit: recorder, vario with sound, Bluetooth vario, map with the track behind the glider, automatic thermal view. Not yet flown. | Airspace, navigation, live tracking, and the rest of the XCTrack checklist |
| Community | Not built | Feed, groups, events |
| **Assistant** | Questions about the pilot's own flying, answered from the logbook by a model on the pilot's own account. Under each answer it says what was looked up. | Weather, sites, airspace and transit once the server has them; voice; alerts in flight |
| **Settings** | The pilot's name and glider (written into recorded flights), the vario sound, and the credits the map data's licences ask for | Units, account |

A tab appears when there is something real behind it. An empty section explains itself rather than being hidden or disabled.

### Logbook

A system list grouped by year, newest first. The first row holds the totals: flights, airtime, cross-country distance. It leads to the statistics: a bar chart of the last twelve months (airtime, distance or number of flights), the gliders flown, and the pilot's bests, each leading to its flight. Each year's header repeats that year's count and airtime. A row shows the track's shape as a thumbnail, the date, the take-off time and glider, and on the right the airtime and the scored distance, with a small triangle if the flight scored as one.

With no flights the screen says so and offers two buttons: import flights (the main action) and add a sample flight. Importing takes single files or a whole folder, with the folders inside it: a pilot's history in one go. While many files are read, a bar over the bottom of the list counts them ("Importing 37 of 312") and the list fills; at the end one sentence says how many flights were added and how many were there already. Files in a folder that are not flight files are passed over without a word.

Deleting asks first. A flight's file exists only on the device, so it cannot be undone.

A flight that began at a place the pilot has a waypoint for says so in its line, after the time: "10:30 · Wallberg". A take-off within a kilometre counts, or any waypoint within a few hundred metres. Names from a proper list of flying sites will take over once there is one.

### Flight

The map fills the screen. A panel floats over its lower part with, from top to bottom: date, times and glider; four numbers (airtime, track distance, highest point, best climb); the altitude profile; the legend for the track colours.

- The track is coloured by climb rate, with a white edge so it reads over any ground. A hollow dot marks the start and a solid dot the end. Under it, a thin dashed line shows the scored route through its turnpoints.
- Dragging along the profile moves a marker along the track, and the four numbers change to that moment: time, time in the air, altitude, climb. The marker **stays** when the finger lifts, so the pilot can look from the chart to the map. A button in the panel's corner returns to the totals. The play button beside it runs the marker along the whole flight in about half a minute.
- Everything else known about the flight is in a sheet opened from the panel: how the flight scores as a cross-country flight (free flight, free triangle or FAI triangle, with distance and points), the time spent in thermals and on glides, every thermal (when, height gained, average climb, the wind it showed) and every glide (distance, glide ratio, speed), both extremes of altitude and climb, the recorder, the exact meaning of each colour. Choosing a thermal or glide closes the sheet and marks that moment on the map and the profile.
- The map is told how much of it the panel and bars cover, and fits the track into the rest.

### Map

The map fills the screen under the tab bar. One round glass button, bottom right, centres the map on the pilot and follows until the map is dragged. Logbook flights are drawn as accent-coloured lines, with thermal circles smoothed into the line the glider drifted along, so many flights stay readable together.

Wherever a map reaches the top of the screen, a soft band of the background colour sits behind the clock and status icons to keep them readable.

**Wind.** When a Venti server is set, every wind station in view is an arrow pointing where the wind blows, darker and a little larger the stronger the wind, with mean wind and gusts in km/h below it ("12/25"). Where stations crowd, the windiest keep their place and the others give way until the pilot zooms in. A station whose reading is more than an hour old is a grey dot without numbers: an old number on a map reads as the wind now. Calm is a pale dot. A tap on an arrow opens the station: the reading with its age, the last 24 hours of wind and gusts as a chart with the direction in a small chart of its own below it, and whose data it is. A second round button above the position button lists the same stations by name, for comparing numbers and for pilots who use VoiceOver, which cannot reach symbols drawn on a map. The providers are credited in a small line at the bottom of the map, and in full on each station's page.

### The page of a shared flight

A flight that its pilot shares has a page on the web, made by the server, which works without the web app and without scripts. It is the first thing someone outside the app sees of Venti, usually as a preview in a chat.

- The page follows the flight screen in what it puts first: a headline that says what the flight was ("42.3 km closed FAI triangle"), who flew it and when; then the track's shape; then four numbers set large (airtime, distance, points, highest point); then the altitude profile; then everything else as a plain list; then one button, to download the flight file.
- The track is a line in the accent colour on a faintly tinted panel, north up, with a hollow dot at the start and a solid dot at the end, as in the app. There is no map under it, because there is no tile server yet.
- It follows light and dark from the visitor's system, uses the system's own typeface, and sets numbers in the rounded face with fixed-width digits where the system has one.
- The drawings say in words what they show, for those who do not see them, and the altitude profile's lowest and highest values stand in its caption rather than as tiny labels inside it.
- The preview picture (1200 by 630) shows the same track on the same panel and three numbers beside it: distance with what it was scored as, airtime, points. It is always light, because a picture cannot follow the reader's appearance. Its numbers are set in Nunito, the open typeface closest to the rounded system face. A pilot's name is not on it: the link's title carries it.
- Points are called "scored by Venti" on the picture and explained in the page's footer: they follow the World XContest's rules as we read them and are not the league's result.

To verify: how the preview looks in the chat apps pilots use (WhatsApp, Signal, Telegram, iMessage). It was built to the Open Graph and Twitter-card conventions and has been looked at in a browser only. The names of the Open Graph lines were checked against <https://ogp.me/> on 2026-11-19. The names of the `twitter:` lines were not checked against X's own documentation, whose page for them could not be reached that day; they are the ones in common use.

### Account and sharing

The account lives under Settings, beside the server's address, because it is optional and belongs to the server. Signing in is one form with a switch between "Sign in" and "New account"; the form says what an account does before it asks for anything, and where a password is chosen it says that a forgotten one cannot be recovered. Signed in, the screen shows how the logbook and the server stand (flights there, flights still to send, in step since when), one button to sync now, and sign out and delete.

A flight is shared from its own screen: More, "Share with a link". The sheet lists the three answers to "who can see this flight" with what each means, the chosen one ticked, and below them the link once the flight is not private. Nothing is ever shared by default.

## Colour

| Role | Light | Dark | Notes |
|---|---|---|---|
| Accent | `#6250D6` | `#9085E9` | The main button, selected controls, a flight's line in lists, charts, the overview map and the page of a shared flight. Violet, because it is none of the colours that carry meaning on a track or will carry meaning for flyability. |
| Everything else | system | system | Backgrounds, text, separators and fills are the platform's semantic colours. |

The map style is light in both appearances for now, so colours drawn on the map have one value.

### Climb and sink

A track is coloured in five bands of climb rate, averaged over ten seconds: two blues for sink, grey for level flight, orange and red for climb. Warm means up.

| Band | Rate (m/s) | Colour |
|---|---|---|
| Strong sink | below −2.0 | `#184F95` |
| Sink | −2.0 to −0.5 | `#3987E5` |
| Level | −0.5 to +0.5 | `#85847F` |
| Climb | +0.5 to +2.0 | `#EB6834` |
| Strong climb | +2.0 and above | `#B8242B` |

A glider's own sink rate in still air is a little over 1 m/s, so an ordinary glide is "sink" blue and thermals stand out against it in orange and red.

The five colours were checked together on 2026-11-18 with a palette validator: every pair stays apart for red-green colour blindness (worst pair 9.7 on the OKLab scale, target 8), for normal vision (16.8, floor 15), and each has at least 3:1 contrast against a light surface. Blue against orange is the pairing that survives colour blindness; red against green does not, and green is reserved for "flyable".

To verify: no flying app we read documents which colours it uses for climb and sink, so "warm means up" is our choice, taken from weather maps and gliding instruments. Check it with pilots.

### Wind

Wind strength is one blue in four steps of the mean wind, light to dark.

| Band | Mean wind (km/h) | Colour |
|---|---|---|
| Light | below 10 | `#86B6EF` |
| Moderate | 10 to 20 | `#3987E5` |
| Fresh | 20 to 30 | `#1C5CAB` |
| Strong | 30 and above | `#0D366B` |

The scale shows a magnitude, not a verdict. Most wind maps run from green to red, which says "fine" and "too much"; but whether 25 km/h is too much depends on the pilot, the wing, the place and the gusts, and the app does not decide that from one number. Green stays reserved for "flyable", which will be worked out from a site's allowed directions and limits, not from a station alone. The band edges are round numbers, not limits. The four steps were checked on 2026-11-19 with the palette validator as an ordered ramp: one hue, evenly darker, the lightest still 2:1 against a light surface. Every arrow has a white edge, so it also stands on dark forest and on water.

In charts the mean wind is a line in `#256ABF` (light) or `#6DA7EC` (dark), and the span up to the gusts is the same blue held back (`#B7D3F6` light, `#1C5CAB` dark).

To verify with pilots: whether the single hue is read as quickly as the green-to-red they know from other apps.

## Icon

A white paraglider on the accent violet; the pilot is a dot in the colour a climb has on a track. It is drawn by `tools/app-icon/make_icon.py`, so it can be changed as text like everything else.

## Type

System text styles only, so the pilot's text-size setting applies everywhere. Titles and body text use the default face. Numbers use the rounded variant with fixed-width digits. No light weights, nothing below the caption size.

## iOS

- Minimum iOS 26. The floating panel, the glass button style and the new tab bar exist only from that version, and supporting older ones would mean writing every glass surface twice and testing a path nobody on the project can see.
- iPhone, portrait. The flight screen is laid out for it; landscape and iPad are open (below).
- Glass is used in exactly two places we draw ourselves: the flight panel and the map's position button. Bars and sheets get theirs from the system.
- Touch targets are at least 44 by 44 points, including the two icon buttons in the flight panel.
- Every custom element has an accessibility label; the chart and legend describe themselves in words.

## Android

The Android app is the same product with Android's own components: Material 3, unstyled where Material has what is needed. It has the iPhone app's five sections. The words are the iPhone's, with "phone" for "iPhone".

- **Colour.** Material 3 with a fixed scheme: the accent as its primary colour, and neutral surfaces. Not "dynamic colour" from the pilot's wallpaper: a wallpaper's orange or blue would sit beside the climb colours as if it meant something. No third hue either.
- **No glass.** Android has no system material like Liquid Glass. The three surfaces that float over a map (the flight panel, the round buttons, the cockpit's badges) are opaque Material surfaces with a shadow. The rule behind the glass holds: they are controls above the content, and nothing else floats.
- **Navigation.** A Material navigation bar with the five sections, which is as many as one takes; it steps aside on a flight, the statistics and the credits, which have the whole screen and a back arrow. The system's back button goes one step back, and in the cockpit asks whether to finish.
- **Lists.** Rows stand on rounded surfaces in groups, a heading in the accent colour above and a line of explanation below, in the logbook, the details, the statistics, the settings and the credits alike.
- **Deleting a flight.** Press and hold its row, or use the flight screen's menu. Both ask first.
- **The flight's details** are a bottom sheet over the map, as on the iPhone. The vario list is one too.
- **Choosing a place to fly to** is a bottom sheet over the cockpit, opened by a tap on the line that shows the way: the take-off first, then the pilot's waypoints, nearest first, with a field to find one by name. In the pilot's list of waypoints, pressing and holding one offers to delete it, as in the logbook. The trim speed and the glide ratio are set with a button either side of the number.
- **Airspace warnings** are a banner over the lower edge of the cockpit's map, in the iPhone's colours in both appearances: a warning is not themed. A tap opens a bottom sheet with every warning and what the pilot can answer. An airspace file is deleted by pressing and holding its row.
- **Wind stations** open in a bottom sheet as well. A station tapped on the map opens at half height, with the map still in view above it; the list of stations opens at full height, with a field to find one by name, and leads to a station and back. TalkBack reads a row as the station's name and its wind in words. The line that credits the providers sits beside the map's own credits button.
- **Type.** The system's typeface at Material's sizes, which follow the pilot's text-size setting. Numbers use digits of equal width. Android has no rounded system face, so numbers are not rounded here. A loss is set with a true minus sign, as wide as the plus of a gain.
- **Charts.** Android has no chart of its own. The altitude profile, the months' bars and a station's wind and direction over a day are drawn by the app: the same axes as on the iPhone, round values on the trailing edge and times of day along the bottom. A finger on a station's charts reads out that moment in the line above them.
- **The assistant's answers** show the bold, italics, code and links a model writes in Markdown, read by a small reader of the app's own; Android has none.
- **Maps** stay north up and flat: they cannot be turned or tilted. To verify with pilots whether turning is missed.
- **The cockpit's sizes are fixed** in the screen's own units, and do not follow the text-size setting. The map cannot be dragged away from the glider, and pinching zooms about the middle of the map, where the glider is, wherever the fingers are.
- **Icons** are Google's Material Symbols, outlined; the Fly tab's is a paraglider. The app icon is the same drawing as on the iPhone, as an adaptive icon with a one-colour version for themed icons.
- Phone, portrait. Touch targets are at least 48 by 48 dp. Every custom element has a description for TalkBack; the chart and the legend describe themselves in words.

To verify, all of it: the app has been seen only in screenshots from an emulator. Sunlight, a hand in a glove, TalkBack and a large text size have not been tried.

## Web

The web app is the same product in a browser: the same screens, the same words. It has the Map, the Logbook and, under Settings, the pilot's account; the cockpit and its settings are for the phone. A browser has no system components to borrow, so the app draws its own, as plainly as it can, and keeps to what a browser does have: its dialogs, its popovers, its focus ring, the reader's text size, the system's light or dark appearance.

Two layouts, chosen by the window's width alone (56 rem, 896 px at the usual text size, so a reader with large text gets the narrow layout sooner):

- **Narrow** is the iPhone app. The navigation is a bar of glass floating at the bottom, and steps aside on the flight screen. The flight panel floats over the lower part of the map; the details open as a sheet from the bottom edge.
- **Wide** uses the room, it does not stretch the phone:
  - The navigation is a rail down the left side, part of the page and not floating on it.
  - **Flight:** the panel stands beside the map as a column, and everything the sheet holds on a phone is in that column under the profile, on screen without a tap. The map has the rest of the window, with buttons for zoom, because a mouse cannot pinch. Choosing a thermal or glide scrolls the column back to the profile, where the mark is.
  - **Logbook:** one column of readable width, with the import button spelled out. The statistics are a narrower column of their own. Each row has room for two more numbers the core already gives, highest point and best climb, under column headings, and a delete button that appears with the mouse over the row.
  - **Map:** the map fills the window beside the rail, with zoom buttons above the wind and position buttons. A chosen wind station is a card at the map's side, not a sheet over it: the map stays in reach, and choosing another arrow fills the same card.

What the web adds:

- **Dropping files.** Flight files dragged onto any screen are imported; while they hover, the page says so.
- **Terrain in 3D.** A button on the flight map tilts it into the terrain and lets it be turned; the same button lays it flat and north up again.
- **Downloading** a flight's file, from the flight screen's menu, where the iPhone has "Share flight file".

Where the web's wind differs from the iPhone's:

- **No server address.** The wind is that of the server the site is served from. A site without one has a map without wind and without the wind button, and says nothing about it.
- **The credit** of the wind's providers is a line of its own directly above the map's credits. It wraps onto a second line rather than being cut short, because it is owed to each of them.
- **A station's chart** is read by a mouse passing over it, by a finger dragged along it, and by the arrow keys one reading at a time.
- **The arrow in the list** has the white edge it has on the map. Without it the darkest blue is lost on a dark surface.

Where the web's account and sharing differ from the iPhone's:

- **No server address.** The account is on the server the site is served from, whose name stands over the form. A site without a server says so instead of offering a form.
- **Refusals stand where they belong**, not in an alert: under the fields of the form, in the dialog that asks for the password, among the rows about the logbook. The words are the core's.
- **What the last sync did** is a row of its own: "3 sent, 1 fetched".
- **The link** of a shared flight opens its page in a new tab, and a button copies it; a browser has no share sheet to hand it to.
- **Deleting a flight** says, when signed in, that the server's copy and those on the pilot's other devices go with it.
- **Passwords** are fields a password manager recognises: the handle as the user name, the password as a current or a new one.

What stands in for the system:

| On the iPhone | In the browser |
|---|---|
| System colours | The same roles as values in `apps/web/src/app.css`. Text and its background keep a contrast of at least 4.5 to 1 in both appearances (calculated, 2026-11-19). |
| Liquid Glass | A translucent, blurred surface, in the same three places and no others: navigation bar, flight panel, map buttons. Dark glass is denser than light, because the map under it stays light. Text on glass has stronger colours of its own, which hold 4.5 to 1 over the lightest and darkest ground the map draws. |
| The selected tab in the accent colour | An accent icon on a filled shape; the tab's name stays in the text colour, because accent text on that shape falls short of 4.5 to 1. |
| System text styles | The system's own typeface at the sizes of those styles, in rem, so the reader's text size scales everything. Numbers use the rounded face where the system has one (Apple's), and digits of equal width everywhere. |
| SF Symbols | A small set of outline icons drawn for the app (`apps/web/src/lib/design/Icon.svelte`). |
| Alerts, sheets, menus | The browser's dialog and popover, which bring focus handling, Escape and the dimmed background with them. |
| Swipe to delete | The flight screen's menu, and on a wide screen a button on the row. |
| The grouped list of a settings screen | Rows on a rounded surface, with what they are about above it and a line of explanation below. A text field is a row with its name in front. |
| The segmented control | A row of radio buttons drawn as segments, the chosen one raised. |
| Accessibility settings | "Reduce motion" stops the animations; "reduce transparency" and "increase contrast" turn glass into a solid surface. |

Keyboard and screen reader: everything can be reached and used from the keyboard. The altitude profile is a slider over the flight: the arrow keys move the mark, Page Up and Page Down move it further, Escape lets go of it, and it reads out time, altitude and climb. The numbers under the cursor are announced as they change. Maps take the keyboard as MapLibre provides.

Times are shown in the time zone of the browser, as the iPhone shows them in the phone's. The words are English; dates and numbers follow the reader's region, so a pilot in Germany gets a 24-hour clock.

To verify, none of it has been seen outside Chromium on a test machine: Safari and Firefox, a real phone (touch on the profile and on a station's chart, tapping an arrow with a finger, the bar's distance from the home indicator, glass over a moving map), a screen reader, and the rounded number face on Apple devices.

## How a design change is checked

Nobody on the project can open Xcode in a cloud session, so the interface is checked by tests that use it. `apps/ios/VentiUITests` starts the app with an empty logbook, taps through every screen as a pilot would, asserts what should be there, and keeps a screenshot of each screen in light and dark appearance. The iOS workflow publishes them as the `ios-screenshots` artifact. Look at them after every change to a screen.

The Android app is checked the same way by `apps/android/app/src/androidTest`, on an emulator that only GitHub can run; the Android workflow publishes the screenshots as the `android-screenshots` artifact.

The web app is checked the same way by `apps/web/e2e`, in a real browser, at a phone's width and a desktop's, in light and dark. A cloud session can run these itself (`npm run test:e2e` in `apps/web`) and look at `apps/web/screenshots`; the Web workflow publishes them as the `web-screenshots` artifact.

## The cockpit

The first version is built and has been checked on a simulator with a replayed flight. It has not flown.

- **A full-screen cover, not a tab.** Starting a flight covers the whole app; nothing can be navigated to by accident. One button ends it, and asks first.
- **Fixed places, fixed sizes.** Altitude and climb sit side by side, always, in the largest type in the app. Unlike everywhere else, these sizes do not follow the text-size setting: an instrument must be where the eye expects it.
- **It changes by itself.** While the glider circles in lift the lower row shows height gained, the thermal's average climb and the wind, and the map zooms in; on the glide it shows ground speed, the glide ratio the glider is making and, beside it, the glide ratio it needs to reach the place it is flying to, and the map zooms out. The pilot touches nothing.
- **Where to.** One line between the map and the numbers says where the cockpit is navigating: an arrow that points to the place as seen from the glider (up is straight ahead), its name and distance, and the height at which the glider is expected to arrive over it. Until the pilot chooses a place, that place is the take-off. A tap on the line lists the pilot's waypoints nearest first; that list is the one thing in the cockpit that needs a second touch. On the map a thin dashed line runs from the glider to the place. An arrival below the place is the second thing shown in red, and a wind too strong to get there is said in words, not as a number. When no wind has been measured yet, the line says that the prediction is for still air.
- **The map follows.** North up, the glider in the middle, the last three minutes of track behind it in the climb colours. It cannot be dragged away; it can be zoomed.
- **The climb bar** beside the climb number grows up or down from the middle, in the climb colours. The number itself stays in the text colour.
- **The line at the top** says whether the flight is being recorded, how long it has lasted and, once there is a kilometre of it, the cross-country distance so far by the logbook's scoring, with a triangle when the flight scores as one.
- **Daylight.** Once less than two hours are left, the top left of the map says "Sunset in 1:12"; after sunset "Night in 0:20", in red, until the end of civil twilight. The times are for a level horizon: in a valley the sun is gone earlier.
- **What is missing is said in red at the top:** no position, or a vario that has stopped talking.
- **Airspace.** The most urgent warning lies over the bottom of the map as a band: what and where in one line ("CTR Innsbruck 300 m NE", "CTR Innsbruck in 30 s", "Inside CTR Innsbruck"), its class and limits below, and a line when the height is in doubt. Plain while the airspace is near, amber when very near, red when the glider is about to be inside or is. A new or more urgent warning is spoken, because the pilot's eyes are outside. A tap lists all warnings with two answers: "Seen: quiet for 10 minutes", which speaks again if it gets worse, and "Does not apply today". On the maps airspace to stay out of has a magenta edge and a faint wash, as aviation charts draw boundaries; airspace that deserves attention is amber; the rest a thin grey line without a name. The wash fades in between zoom 8 and 10: on a map of a whole region, where airspace lies over airspace, only the edges are drawn, so that the wind and the valleys are not painted over. Before take-off the Fly tab says in a line what the cockpit will warn from ("openAIP, 344 airspaces · 1 file", or "None"), and a tap opens the list: what is kept from the pilot's server, by provider, with its date, its credit and its notice, and the pilot's own files. The cockpit must never be silent for a reason the pilot could not have seen on the ground.
- **Sound.** Beeps that rise in pitch and speed with the climb, a continuous low tone in heavy sink, silence on the ground. One button mutes it.
- **Trying it on the ground.** The Fly tab can replay the sample flight through the cockpit, in real time or twenty times faster.

To verify in the air, all of it: readability in sun, whether the numbers are the right ones, the sound, glove-sized targets.

What the apps pilots already fly with teach, and what the next versions should take from them:

- XCTrack switches to its thermal page by itself when it detects circling and back afterwards, and offers black themes and a white high-contrast one. Automatic actions exist "to minimize in-flight user interaction".
- SeeYou Navigator lays values over the map and lets any box be moved and resized by pressing and holding. Its reviewer chose the light map "which lets you see the screen better", and missed an automatic re-centre.
- burnair shortens the pilot's own track to the last minute and a half while circling, to clear the map.
- ForeFlight fixes its hazard colours (red within 100 ft, yellow within 1,000 ft) and repeats every alert in sound.

Still to design: a high-contrast theme for sun, layouts the pilot can change, routes of more than one place, and the airspace side view.

## Open

- A dark map style. The map is light in dark mode today.
- On the web: a layout between the two, for a tablet held upright, where the phone's panel is wider than it needs to be; and the track at flown height in the 3D view.
- Landscape and iPad layouts for the flight screen.
- German. All text is English and written in the code; it moves to a string catalogue with the first translation.
- Units other than metric.
- Names for flights. A row is titled by its date until the sites data source exists and a flight can be named after its take-off.

## Sources

Read on 2026-11-18. Apple's pages were read in their published form that day.

- Apple, Human Interface Guidelines: [Materials](https://developer.apple.com/design/human-interface-guidelines/materials), [Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars), [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), [Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets), [Maps](https://developer.apple.com/design/human-interface-guidelines/maps), [Charts](https://developer.apple.com/design/human-interface-guidelines/charts), [Typography](https://developer.apple.com/design/human-interface-guidelines/typography), [Color](https://developer.apple.com/design/human-interface-guidelines/color), [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility).
- Apple, [Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass).
- Apple Design Awards: [Flighty](https://developer.apple.com/news/?id=970ncww4) (2023), [Tide Guide](https://developer.apple.com/news/?id=4r9b23wx) (2026), and the [awards pages](https://developer.apple.com/design/awards/) for Slopes, Gentler Streak, Crouton and Any Distance.
- Slopes, [Liquid Glass comes to skiing](https://blog.getslopes.com/liquid-glass-comes-to-skiing-and-snowboarding/): glass only on the ride card and controls, so the map shows through.
- Nielsen Norman Group, [Liquid Glass](https://www.nngroup.com/articles/liquid-glass/): the counter-view, that the new design costs legibility. It is why our text sits on glass only in the flight panel, and why the panel's numbers are large.
- [XCTrack manual](https://xctrack.org/Manual.html); Cross Country magazine's [SeeYou Navigator review](https://xcmag.com/?p=70303); the App Store pages of [burnair Go](https://apps.apple.com/us/app/id1666354109) and [Gaggle](https://apps.apple.com/us/app/id1556694314); [ForeFlight hazard awareness](https://foreflight.com/hazard).

To verify: that iOS 27 keeps this design language with adjusted translucency and a user setting for it. That comes from press coverage ([MacRumors](https://www.macrumors.com/2026/06/10/how-liquid-glass-is-changing-in-ios-27/), [MacStories](https://www.macstories.net/stories/ios-and-ipados-27-review/2/)), not from Apple's guidelines. If it holds, our glass surfaces must stay legible at both ends of that setting.
