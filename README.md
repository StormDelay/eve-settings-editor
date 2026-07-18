# EVE Settings Editor

A small desktop app for editing your local EVE Online client settings — the
`core_char_*.dat` / `core_user_*.dat` files EVE stores per character and per
account. It gives a safe, visual way to do things the in-game UI makes tedious:
place and size windows, configure overview columns, edit remembered-text
(autofill) lists, name your accounts and link characters to them, and copy one
character's whole setup onto your others.

Every change goes through a **backup → verify → atomic-write** chain: the file is
backed up first, the new bytes are decoded and checked before they replace the
original, and a failed step never leaves a half-written file. One-click restore
from any backup.

## Features

- **Layout editor** — drag and resize windows on a scaled mock of your screen, or
  type exact geometry.
- **Overview columns** — show/hide, reorder, and resize columns per overview tab.
- **Autofill** — edit or clear the text the client autocompletes.
- **Accounts** — give accounts readable names and associate characters with them,
  manually or via guided capture.
- **Batch apply** — copy one character's layout / overview / autofill / everything
  onto other characters, with a preview of exactly which files (and which other
  characters) each copy will affect.
- **Raw tree editor** with search, for everything else.

## Install

Download the installer for your OS from the
[Releases](https://github.com/StormDelay/eve-settings-editor/releases) page —
Windows (`.msi` / `.exe`), macOS (`.dmg`), or Linux (`.AppImage` / `.deb` /
`.rpm`).

Builds are **unsigned**, so your OS warns on first launch (Windows SmartScreen →
"More info" → "Run anyway"; macOS → right-click → Open). That is expected.

## Scope & safety

This tool only reads and writes the local settings files you already own — the
same ones players copy by hand. It never touches the game client, your account
credentials, network traffic, or anything in-game. It is not affiliated with or
endorsed by CCP Games.

## Build from source

Requires [Rust](https://rustup.rs) and [Node](https://nodejs.org). From `app/`:
`npm install`, then `npm run tauri dev` to run, or `npm run tauri build` to
package.

## License

MIT — see [LICENSE](LICENSE).
