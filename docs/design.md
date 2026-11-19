# Design

Last revised: 2026-11-19

How Venti looks and behaves, on every platform. The iOS app is the first place these rules are built; Android and the web follow the same structure, colours and wording with their own platform's components. The web app is the second; what is particular to it is under [Web](#web).

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

With no flights the screen says so and offers two buttons: import flights (the main action) and add a sample flight.

Deleting asks first. A flight's file exists only on the device, so it cannot be undone.

### Flight

The map fills the screen. A panel floats over its lower part with, from top to bottom: date, times and glider; four numbers (airtime, track distance, highest point, best climb); the altitude profile; the legend for the track colours.

- The track is coloured by climb rate, with a white edge so it reads over any ground. A hollow dot marks the start and a solid dot the end. Under it, a thin dashed line shows the scored route through its turnpoints.
- Dragging along the profile moves a marker along the track, and the four numbers change to that moment: time, time in the air, altitude, climb. The marker **stays** when the finger lifts, so the pilot can look from the chart to the map. A button in the panel's corner returns to the totals. The play button beside it runs the marker along the whole flight in about half a minute.
- Everything else known about the flight is in a sheet opened from the panel: how the flight scores as a cross-country flight (free flight, free triangle or FAI triangle, with distance and points), the time spent in thermals and on glides, every thermal (when, height gained, average climb, the wind it showed) and every glide (distance, glide ratio, speed), both extremes of altitude and climb, the recorder, the exact meaning of each colour. Choosing a thermal or glide closes the sheet and marks that moment on the map and the profile.
- The map is told how much of it the panel and bars cover, and fits the track into the rest.

### Map

The map fills the screen under the tab bar. One round glass button, bottom right, centres the map on the pilot and follows until the map is dragged. Logbook flights are drawn as accent-coloured lines, with thermal circles smoothed into the line the glider drifted along, so many flights stay readable together.

Wherever a map reaches the top of the screen, a soft band of the background colour sits behind the clock and status icons to keep them readable.

## Colour

| Role | Light | Dark | Notes |
|---|---|---|---|
| Accent | `#6250D6` | `#9085E9` | The main button, selected controls, a flight's line in lists, charts and the overview map. Violet, because it is none of the colours that carry meaning on a track or will carry meaning for flyability. |
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

## Web

The web app is the same product in a browser: the same screens, the same words. It has the Map and the Logbook; the cockpit and its settings are for the phone. A browser has no system components to borrow, so the app draws its own, as plainly as it can, and keeps to what a browser does have: its dialogs, its popovers, its focus ring, the reader's text size, the system's light or dark appearance.

Two layouts, chosen by the window's width alone (56 rem, 896 px at the usual text size, so a reader with large text gets the narrow layout sooner):

- **Narrow** is the iPhone app. The navigation is a bar of glass floating at the bottom, and steps aside on the flight screen. The flight panel floats over the lower part of the map; the details open as a sheet from the bottom edge.
- **Wide** uses the room, it does not stretch the phone:
  - The navigation is a rail down the left side, part of the page and not floating on it.
  - **Flight:** the panel stands beside the map as a column, and everything the sheet holds on a phone is in that column under the profile, on screen without a tap. The map has the rest of the window, with buttons for zoom, because a mouse cannot pinch. Choosing a thermal or glide scrolls the column back to the profile, where the mark is.
  - **Logbook:** one column of readable width, with the import button spelled out. The statistics are a narrower column of their own. Each row has room for two more numbers the core already gives, highest point and best climb, under column headings, and a delete button that appears with the mouse over the row.
  - **Map:** the map fills the window beside the rail, with zoom buttons above the position button.

What the web adds:

- **Dropping files.** Flight files dragged onto any screen are imported; while they hover, the page says so.
- **Terrain in 3D.** A button on the flight map tilts it into the terrain and lets it be turned; the same button lays it flat and north up again.
- **Downloading** a flight's file, from the flight screen's menu, where the iPhone has "Share flight file".

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
| Accessibility settings | "Reduce motion" stops the animations; "reduce transparency" and "increase contrast" turn glass into a solid surface. |

Keyboard and screen reader: everything can be reached and used from the keyboard. The altitude profile is a slider over the flight: the arrow keys move the mark, Page Up and Page Down move it further, Escape lets go of it, and it reads out time, altitude and climb. The numbers under the cursor are announced as they change. Maps take the keyboard as MapLibre provides.

Times are shown in the time zone of the browser, as the iPhone shows them in the phone's. The words are English; dates and numbers follow the reader's region, so a pilot in Germany gets a 24-hour clock.

To verify, none of it has been seen outside Chromium on a test machine: Safari and Firefox, a real phone (touch on the profile, the bar's distance from the home indicator, glass over a moving map), a screen reader, and the rounded number face on Apple devices.

## How a design change is checked

Nobody on the project can open Xcode in a cloud session, so the interface is checked by tests that use it. `apps/ios/VentiUITests` starts the app with an empty logbook, taps through every screen as a pilot would, asserts what should be there, and keeps a screenshot of each screen in light and dark appearance. The iOS workflow publishes them as the `ios-screenshots` artifact. Look at them after every change to a screen.

The web app is checked the same way by `apps/web/e2e`, in a real browser, at a phone's width and a desktop's, in light and dark. A cloud session can run these itself (`npm run test:e2e` in `apps/web`) and look at `apps/web/screenshots`; the Web workflow publishes them as the `web-screenshots` artifact.

## The cockpit

The first version is built and has been checked on a simulator with a replayed flight. It has not flown.

- **A full-screen cover, not a tab.** Starting a flight covers the whole app; nothing can be navigated to by accident. One button ends it, and asks first.
- **Fixed places, fixed sizes.** Altitude and climb sit side by side, always, in the largest type in the app. Unlike everywhere else, these sizes do not follow the text-size setting: an instrument must be where the eye expects it.
- **It changes by itself.** While the glider circles in lift the lower row shows height gained, the thermal's average climb and the wind, and the map zooms in; on the glide it shows ground speed, glide ratio and distance to the take-off, and the map zooms out. The pilot touches nothing.
- **The map follows.** North up, the glider in the middle, the last three minutes of track behind it in the climb colours. It cannot be dragged away; it can be zoomed.
- **The climb bar** beside the climb number grows up or down from the middle, in the climb colours. The number itself stays in the text colour.
- **What is missing is said in red at the top:** no position, or a vario that has stopped talking.
- **Sound.** Beeps that rise in pitch and speed with the climb, a continuous low tone in heavy sink, silence on the ground. One button mutes it.
- **Trying it on the ground.** The Fly tab can replay the sample flight through the cockpit, in real time or twenty times faster.

To verify in the air, all of it: readability in sun, whether the numbers are the right ones, the sound, glove-sized targets.

What the apps pilots already fly with teach, and what the next versions should take from them:

- XCTrack switches to its thermal page by itself when it detects circling and back afterwards, and offers black themes and a white high-contrast one. Automatic actions exist "to minimize in-flight user interaction".
- SeeYou Navigator lays values over the map and lets any box be moved and resized by pressing and holding. Its reviewer chose the light map "which lets you see the screen better", and missed an automatic re-centre.
- burnair shortens the pilot's own track to the last minute and a half while circling, to clear the map.
- ForeFlight fixes its hazard colours (red within 100 ft, yellow within 1,000 ft) and repeats every alert in sound.

Still to design: a high-contrast theme for sun, layouts the pilot can change, and the pages for navigation and airspace.

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
