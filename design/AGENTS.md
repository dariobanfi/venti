# Venti design pack: start here

You are building UI for Venti, a paragliding app. Follow this pack for anything a user sees.

## Before you write UI

1. Read `DESIGN.md`.
2. Look at `reference/screens/*.png`. Markup for them is in `reference/index.html`.
3. Import the tokens for your platform:

| Platform | File |
|---|---|
| Web | `tokens/tokens.css` and `components/components.css` |
| Tailwind | `tokens/tailwind.preset.js` and `tokens/tokens.css` |
| React Native, Flutter, map styles | `tokens/tokens.ts` or `tokens/tokens.json` |
| SwiftUI | `tokens/VentiTheme.swift` |
| Jetpack Compose | `tokens/VentiTheme.kt` |

   The Swift and Kotlin files cover the core tokens and have not been compiled yet. Map and airspace colours are in `tokens.json`.

4. Bundle the fonts in `fonts/` (keep `OFL.txt` with them).

## Hard rules

- No hex values, font sizes or radii in components. Use tokens. If one is missing, ask.
- Use the files in `assets/`. Do not redraw or recolour the logo, icon or art.
- Every screen works in `light` and `dark`. In-flight screens use `flight` only.
- One accent (coral) button per screen at most.
- Status is icon + word + colour. Never colour alone.
- Every coloured data layer has a legend. Every forecast has an update time.
- Changing numbers use tabular figures and show their unit.
- Touch targets are 48 px or more, 64 px in flight.
- No art behind data. No art at all in flight mode.
- Never describe conditions as "safe". Never show stale data as live.

## Before you finish

- [ ] Screenshot the screen in each theme and compare with `reference/screens/`.
- [ ] Check loading, empty, error and offline states.
- [ ] Check at 360 px wide and with text at 130%.
- [ ] Search your diff for `#` colours and hard-coded sizes.
