# Design

Last revised: 2026-11-18

How Venti looks and behaves, on every platform. The iOS app is the first place these rules are built; Android and the web follow the same structure, colours and wording with their own platform's components.

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
| Fly | Not built | The cockpit and recorder |
| Community | Not built | Feed, groups, events |

A tab appears when there is something real behind it. An empty section explains itself rather than being hidden or disabled.

### Logbook

A system list grouped by year, newest first. The first row holds the totals: flights, airtime, track distance. Each year's header repeats that year's count and airtime. A row shows the track's shape as a thumbnail, the date, the take-off time and glider, and on the right the airtime and track distance.

With no flights the screen says so and offers two buttons: import flights (the main action) and add a sample flight.

Deleting asks first. A flight's file exists only on the device, so it cannot be undone.

### Flight

The map fills the screen. A panel floats over its lower part with, from top to bottom: date, times and glider; four numbers (airtime, track distance, highest point, best climb); the altitude profile; the legend for the track colours.

- The track is coloured by climb rate, with a white edge so it reads over any ground. A hollow dot marks the start and a solid dot the end.
- Dragging along the profile moves a marker along the track, and the four numbers change to that moment: time, time in the air, altitude, climb. The marker **stays** when the finger lifts, so the pilot can look from the chart to the map. A button in the panel's corner returns to the totals.
- Everything else known about the flight is in a sheet opened from the panel: both extremes of altitude and climb, the recorder, the exact meaning of each colour.
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

## How a design change is checked

Nobody on the project can open Xcode in a cloud session, so the interface is checked by tests that use it. `apps/ios/VentiUITests` starts the app with an empty logbook, taps through every screen as a pilot would, asserts what should be there, and keeps a screenshot of each screen in light and dark appearance. The iOS workflow publishes them as the `ios-screenshots` artifact. Look at them after every change to a screen.

## The cockpit, when it is built

Not designed yet. What the apps pilots already fly with teach, to be confirmed in the air:

- XCTrack switches to its thermal page by itself when it detects circling and back afterwards, and offers black themes and a white high-contrast one. Automatic actions exist "to minimize in-flight user interaction".
- SeeYou Navigator lays values over the map and lets any box be moved and resized by pressing and holding. Its reviewer chose the light map "which lets you see the screen better", and missed an automatic re-centre.
- burnair shortens the pilot's own track to the last minute and a half while circling, to clear the map.
- ForeFlight fixes its hazard colours (red within 100 ft, yellow within 1,000 ft) and repeats every alert in sound.

So: very large numbers, few of them, pages that change by themselves with the phase of flight, a high-contrast light theme for sun, sound for anything urgent, and targets a gloved thumb can hit.

## Open

- A dark map style. The map is light in dark mode today.
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
