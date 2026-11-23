# Venti design language

Venti is a paragliding app: weather, maps and cross-country flights in one place.
The look is called **Valley**: a thermal spiral climbing out of an Alpine valley at dawn.

Open `reference/index.html` to see everything below rendered. Screenshots are in `reference/screens/`.

## 1. Principles

1. **Readable on a mountain.** Pilots use this in sun, in gloves, and in turbulence. Size and contrast come before style.
2. **The number is the hero.** Lead with the reading (wind, climb, altitude), then its unit, then its name.
3. **Art frames, data decides.** Valley art is for arrival moments. It never sits behind data.
4. **Cool is down, warm is up.** Teal means sink, coral and gold mean lift. Keep that everywhere.
5. **Inform, never clear.** The app describes conditions. The pilot decides.

## 2. Themes

| Theme | When | Feel |
|---|---|---|
| `light` | Default, phone in light mode | Cream paper, indigo ink |
| `dark` | Phone in dark mode | The indigo slope from the art |
| `flight` | From flight start to landing, whatever the phone setting | Pure white, near-black ink, large targets |

Web: set `data-theme` on any element. Native: `VentiTheme.light / .dark / .flight`, `VentiLight / VentiDark / VentiFlight`.

## 3. Colour

Use **theme tokens** (`--color-*`) in product UI. Use **brand colours** (`--brand-*`) only in art and marketing. Never write a hex value in a component.

| Token | Use |
|---|---|
| `bg`, `surface`, `surface-raised` | Page, cards and sheets, inset tracks |
| `text`, `text-secondary`, `text-muted` | Readings and titles, names, captions |
| `primary` / `on-primary` | Main button, selected chip or tab |
| `accent` / `on-accent` | One per screen: the action that starts or records a flight |
| `accent-text` | Coral as text or icon. Plain `accent` is too light for text |
| `border`, `border-strong` | Dividers, inputs and outline buttons |
| `good`, `caution`, `danger` (+ `-soft`) | Status only |
| `danger-fill` / `on-danger-fill` | Alerts and destructive buttons |

- Coral is the brand, crimson is danger. Do not use coral for errors or crimson for decoration.
- Status is always **icon + word + colour**: Flyable, Marginal, Not flyable.
- All text pairs pass WCAG AA (4.5:1). Flight theme passes 7:1, except the accent button. See `tokens/contrast-report.md`.

### Data scales

Pick the step with `scaleColor()` in `tokens/tokens.ts` or the breaks below. Always show a legend or the number next to the colour.

| Scale | Type | Breaks | Use |
|---|---|---|---|
| `vario-1…7` | Teal, grey, coral | −3, −1.5, −0.5, +0.5, +1.5, +3 m/s | Flown tracks, altitude chart, vario bar |
| `wind-1…7` | Purple | 5, 10, 15, 20, 25, 35 km/h | Wind layers and stations |
| `thermal-1…6` | Coral | 0.5, 1, 1.5, 2, 3 m/s | Forecast climb, hotspots |
| `series-1…5` | Five hues, fixed order | none | Comparing pilots or flights |

- Wind colour shows speed only. Whether it is too much for a site is a **status**, from that site's limits.
- Series order is fixed and tested for colour blindness between neighbours. On a map, where any two tracks can cross, show at most 3 and label them. Never invent a sixth colour.
- Text is never coloured by a scale. Put a coloured mark next to normal text.
- Charts: one y-axis, 2–2.5 px lines, quiet grid in `border`, tap or hover shows the value.

### Map

Colours are in `--map-*` and `venti.map`. The base map stays pale so overlays stand out.

- Order, bottom to top: land, hillshade, contours, water, roads, weather layer, airspace, tracks, sites, labels, own position.
- Tracks get a dark casing (`track-casing`), about 1.5 px each side.
- Labels get a halo (`label-halo`).
- Airspace: 2 px outline, 12–16% fill, and a text label with class and limits. Prohibited and restricted add a 45° hatch. Mandatory zones (TMZ, RMZ) use a dashed outline.
- Own position is a cream arrow with a dark ring, the same pairing as the dot in the logo.

## 4. Type

One family: **Fira Sans** (open licence, files in `fonts/`).

| Style | Size / line | Weight | Use |
|---|---|---|---|
| `display` | 56 / 56 | Black Italic | Marketing headlines |
| `hero` | 36 / 38 | Black Italic | At most one per screen: the verdict or the distance |
| `title` | 26 / 32 | SemiBold | Screen titles |
| `heading` | 19 / 25 | SemiBold | Card and sheet titles |
| `body` | 16 / 24 | Regular | Text |
| `label` | 14 / 20 | Medium | Controls, reading names |
| `caption` | 12 / 16 | Regular | Sources, times. Not in flight theme |
| `instrument-xl` | 84 / 84 | Bold | The one main flight reading |
| `instrument-l` | 44 / 46 | Bold | Other flight readings |
| `instrument-m` | 24 / 28 | SemiBold | Readings in cards |

- Numbers that change use tabular figures so they don't jump.
- Vario always has a sign: `+2.6`, `−1.4` (true minus, U+2212).
- Units are smaller, in `text-secondary`, after the number.
- Sentence case everywhere. No all-caps labels.
- Support the phone's text size setting up to 130%.

## 5. Space, shape, depth, motion

- Spacing: 4, 8, 12, 16, 24, 32, 48, 72. Screen padding 16.
- Radius shows role: `s` 8 status, `m` 14 cards and inputs, `l` 24 sheets, `pill` buttons and chips.
- Cards are flat with a 1 px border. Only things floating over the map get a shadow.
- Touch targets: 48 px, 64 px in flight. Chips draw at 36 px, so pad their tap area to 48.
- Motion: 120 / 200 / 320 ms, `cubic-bezier(.2,.8,.2,1)`. Move things only in answer to a tap. The spiral loader is the one looping animation. Respect reduced motion.

## 6. Components

`components/components.css` is the reference. Match it on native.

| Component | Class | Rule |
|---|---|---|
| Button | `.v-btn` + `--accent`, `--quiet`, `--danger`, `--round` | Label is a verb: "Start flight" |
| Chip | `.v-chip` | Toggles map layers |
| Segmented control | `.v-seg` | 2–4 choices |
| Status | `.v-status` | Icon and word required |
| Alert | `.v-alert` | What, how far, which way |
| Card, sheet | `.v-card`, `.v-sheet` | No card inside a card |
| Reading | `.v-reading` + `--l`, `--xl` | Value, unit, name |
| Hour strip | `.v-hours` | Hour, wind arrow, speed, climb bar |
| Scale legend | `.v-scale` | Next to every coloured layer |
| Tab bar | `.v-tabs` | Today, Map, Fly, Flights, Community |

Wind arrows point where the wind **goes**. Text names where it comes **from** ("Wind from W").

## 7. Flight mode

- No art, gradients, shadows or captions. No text under 14 px.
- One `instrument-xl` reading, at most four `instrument-l`.
- At most two buttons. Ending a flight is press and hold.
- Nothing needs a swipe, a long list or typing.
- Alerts take the top of the screen and come with sound and vibration.
- Stale or lost data is shown as such: replace the number with `– –` and say "No GPS" or "Wind 25 min old". Never keep showing an old value as live.

## 8. Logo and art

| Need | File |
|---|---|
| Logo on cream or white | `assets/logo/lockup-on-light.svg` |
| Logo on indigo | `assets/logo/lockup-on-dark.svg` |
| Spiral on sky or cream | `assets/logo/mark.svg` (under 64 px: `mark-small.svg`) |
| Spiral on indigo, coral or photos | `assets/logo/mark-mono-cream.svg` |
| Loading | `assets/logo/loader.svg` |
| iOS | `assets/app-icon/ios/AppIcon.appiconset`, layers in `assets/app-icon/layers/` |
| Android | copy `assets/app-icon/android/res/` into the project |
| Web | `assets/app-icon/web/`, tags in `head.html` |
| Website hero | `assets/illustration/hero-wide.svg`, text goes on the indigo slope |
| Splash, onboarding, mobile hero | `assets/illustration/hero-portrait.svg`, text goes on the indigo base |
| Empty states, backdrops | `valley-wide-empty.svg`, `valley-portrait-empty.svg` |
| Header on Today | `valley-strip.svg` |
| Indigo sections | `assets/pattern/contours-on-indigo.svg`, tiles |
| Link previews, store | `assets/social/` |

- Clear space around the logo: the width of the dot and its ring, twice.
- Don't recolour, rotate, flip, outline or add shadows to the spiral.
- Don't put the full-colour spiral on indigo: its tail disappears.
- Don't retype the wordmark. Use the file.

**Drawing new art:** flat shapes with angular edges. Purple slopes on the left (shade), peach and coral on the right (sun), lighter with distance. Sky runs teal to cream. Thin contour lines on the nearest slopes only. No photos, 3D, glow or glass.

**Icons:** `assets/icons/` has the flying set (24 px, 2 px stroke, round ends, `currentColor`). For general icons such as search or settings, use Lucide at 2 px stroke.

## 9. Voice

Short and calm, like a local pilot at launch.

| Write | Not |
|---|---|
| Good from noon | Epic conditions await! |
| Gusts to 26 km/h after 15:00 | Wind may be suboptimal |
| Not flyable | Dangerous!! |
| CTR D in 1.2 km | Warning: airspace proximity detected |
| No flights yet. Start one, or import an IGC file. | Nothing here :( |

- Never say "safe" or "go". Forecasts show their update time and source.
- Metric by default, 24-hour time. Airspace limits stay in the unit they are published in.

## 10. Still to confirm with the product owner

- Airspace colours and scale breaks are proposals. Test them with pilots.
- Flight theme contrast is computed, not yet tested on a phone in direct sun.
- Tagline ("Weather, maps and cross-country in one app") and tab names are placeholders.
