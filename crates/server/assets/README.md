# Files compiled into the server

| File | What it is | Licence |
|---|---|---|
| `sample-pilots/*.png` | The pictures of the seven made-up sample pilots (`src/dev/sample_pilots.rs`, seeded by `tools/sample-community/`, not by the development mode): a paraglider over a ridge on a coloured sky, no face. Drawn by `sample-pilots/make.py` (`python3 crates/server/assets/sample-pilots/make.py` writes them again, the same bytes every time) | The project's own, under Venti's licence |
| `Nunito-wght.ttf` | The Nunito typeface as one variable font (weights 200 to 1000). The numbers on a flight's preview picture are set in it; see `src/flights/picture.rs`. | SIL Open Font License 1.1, in `Nunito-OFL.txt` |

## Nunito

- Copyright 2014 The Nunito Project Authors (<https://github.com/googlefonts/nunito>).
- Taken on 2026-11-19 from Google Fonts' repository, unchanged apart from its file name: <https://github.com/google/fonts/tree/main/ofl/nunito>, file `Nunito[wght].ttf`. SHA-256 `bb55a5ca5c2042335b3991af27c4d0705d0ef41cac6164ac737fd8f2a1e85207`.
- Licence read the same day in that folder's `OFL.txt`, which is copied here. It lets the font be bundled and embedded with any software as long as the copyright notice and the licence go with it, and forbids selling the font by itself. It names no reserved font name. The font stays under its own licence; it is not part of Venti's AGPL code.

To replace the typeface, put another variable font with a `wght` axis here, with its licence beside it, and change the path in `picture.rs`. The test `the_typeface_has_every_character_the_picture_sets` says whether it will do.
