# Contrast report

WCAG 2 contrast ratios, computed from the tokens. Text needs 4.5 (7 in flight mode, except on-accent); focus rings and input borders need 3.

| Foreground | Background | Light | Dark | Flight |
|---|---|---|---|---|
| text | bg | 15.8 | 14.0 | 19.2 |
| text | surface | 16.4 | 11.9 | 19.2 |
| text | surface-raised | 14.3 | 9.8 | 16.2 |
| text-secondary | bg | 7.9 | 9.2 | 16.7 |
| text-secondary | surface | 8.2 | 7.8 | 16.7 |
| text-secondary | surface-raised | 7.1 | 6.5 | 14.1 |
| text-muted | bg | 5.4 | 6.4 | 12.2 |
| text-muted | surface | 5.7 | 5.5 | 12.2 |
| text-muted | surface-raised | 4.9 | 4.5 | 10.3 |
| accent-text | bg | 5.6 | 7.5 | 9.1 |
| accent-text | surface | 5.8 | 6.4 | 9.1 |
| accent-text | surface-raised | 5.1 | 5.3 | 7.7 |
| link | bg | 6.1 | 9.1 | 8.9 |
| link | surface | 6.4 | 7.7 | 8.9 |
| link | surface-raised | 5.5 | 6.4 | 7.5 |
| good | bg | 5.7 | 8.8 | 9.7 |
| good | surface | 5.9 | 7.5 | 9.7 |
| good | surface-raised | 5.1 | 6.2 | 8.2 |
| caution | bg | 5.8 | 10.7 | 9.9 |
| caution | surface | 6.1 | 9.1 | 9.9 |
| caution | surface-raised | 5.3 | 7.5 | 8.4 |
| danger | bg | 6.4 | 7.2 | 10.0 |
| danger | surface | 6.6 | 6.2 | 10.0 |
| danger | surface-raised | 5.8 | 5.1 | 8.4 |
| on-primary | primary | 12.2 | 14.4 | 19.2 |
| on-accent | accent | 5.6 | 6.9 | 5.9 |
| good | good-soft | 5.0 | 5.7 | 7.9 |
| caution | caution-soft | 5.1 | 6.8 | 7.9 |
| danger | danger-soft | 5.3 | 5.8 | 7.9 |
| on-danger-fill | danger-fill | 6.7 | 6.0 | 8.1 |
| focus | bg | 4.5 | 9.1 | 8.9 |
| focus | surface | 4.7 | 7.7 | 8.9 |
| border-strong | bg | 3.6 | 4.3 | 19.2 |
| border-strong | surface | 3.7 | 3.7 | 19.2 |

## Day levels on the map

Each level's colour at its own fill opacity over map `land` (light #F7EFDD, dark #211640), and the map `label` (light #221640, dark #FFF4DA) on the result. Flight takes the light values. A fill is not text: it needs no ratio, the figures show how far it stands off the land.

| Level | Light fill | Over land | Label on fill | Dark fill | Over land | Label on fill |
|---|---|---|---|---|---|---|
| not-flyable | #D65F56 at 0.60 | 2.0 | 7.3 | #E56954 at 0.65 | 2.9 | 5.3 |
| glides-only | #8A7CA6 at 0.55 | 1.8 | 8.0 | #5F5A72 at 0.80 | 2.1 | 7.4 |
| weak | #FDD283 at 0.85 | 1.2 | 12.1 | #FDD283 at 0.60 | 5.0 | 3.1 |
| good | #6BD9A4 at 0.60 | 1.3 | 11.2 | #6BD9A4 at 0.55 | 3.8 | 4.1 |
| very-good | #6A4CB8 at 0.55 | 2.3 | 6.3 | #C3AEF5 at 0.60 | 3.9 | 3.9 |

- Light: the label passes 4.5 on every fill (6.25 at the least, on very good), with or without its halo.
- Dark: the label fails 4.5 on weak (3.1), good (4.1) and very good (3.9). Labels on a fill keep `label-halo` (#1C1236), on which the label is 16.2 (light 15.8).
- Neighbouring levels differ in lightness by 1.1 to 1.8 (light) and 1.05 to 2.4 (dark); hue and the key tell them apart. Colour blindness: to verify.
