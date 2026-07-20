# Character-centric entry point (design)

Date: 2026-07-20
Status: approved, pre-plan
Builds on: M1 (discovery, app shell), M3b (char↔account roster + pairing),
the overview/autofill editors, and the two-slot `char`/`user` app state.

Interrupts the planned roadmap (overview-depth slice 2) to rework the tool's
entry-point model. Ships after v0.9.0.

## 1. Goal

Today the tool is **file-centric**: the sidebar lists both `core_char_<id>.dat`
and `core_user_<id>.dat` files as clickable entry points, and a
**Character/Account toggle** in the editor header flips which of the two open
documents you edit. Accounts are a first-class thing you open.

Make it **character-centric all the way**: you pick a *character*, and the
account (`core_user`) file is edited only incidentally, through that character.
The account file stops being a browsable/openable target. Because an EVE account
is shared by up to three characters, account-scoped edits made through a
character are labelled as shared.

This is mostly a frontend re-framing. The backend two-slot model, the
`char`/`user` documents, the reconcile machinery (`reconcileUserSlot`,
`pairedFilePath`, `userSlotFor`), the codec, and every editor stay. The net is a
deletion of the toggle and the file-kind sidebar groups.

## 2. The core mechanic — the view selects the active document

Two editing slots (`char`, `user`) remain open at once, each with its own dirty
flag, exactly as now. What goes away is the user-facing **Character/Account
toggle** that sets `active`.

Instead, **`active` is derived from the current view tab**:

| View     | Active document | Notes                                              |
|----------|-----------------|----------------------------------------------------|
| Tree     | char (default)  | A Tree-local switch can flip it to the account file (§7). |
| Layout   | char            | Char-only, as today.                               |
| Overview | char            | Spans both; edits both slots via explicit callbacks, as today. Backups/search reflect the character. |
| Autofill | user            | Account-scoped; only when the account file is loaded (else §5 prompt). |

`runMutation`, tree search, and the backups panel keep reading `slots[active]`
unchanged — they simply no longer have a manual toggle feeding `active`.
Switching tabs sets `active`; the user never sets it directly.

Rule: `active` is `"user"` only when (a) the Autofill tab is selected **and** the
account file is loaded, or (b) the Tree tab has its account-file switch on.
Otherwise `active` is `"char"`. This keeps `current`/backups sensible on the
unpaired path (they show the character, and the account editors render their own
pairing prompt).

## 3. Entry point — the sidebar lists characters

`Sidebar` stops grouping files by kind. For each discovered profile (primary
profile pinned open, as now) it lists **the profile's char-kind files** sorted by
resolved name, showing `resolvedName ?? file_name` + size — identical rendering
to today's "Characters" group, just promoted to be the whole list.

- **Kept, unchanged in behavior:** the **hide-non-standard** toggle (default on).
  Scoped to characters now: on → only `core_char_<id>.dat` names; off → backup /
  anomalous char files too. Char files whose id doesn't resolve to an ESI name
  still show their `core_char_<id>.dat` label — same as currently.
- **Account alias as metadata:** where a character is paired, its account alias
  shows as dim, non-clickable metadata on the row (`Jita Trader · Main`), via
  `aliasFor(accountOf(charId, roster))`. The sidebar already loads the roster.
- **Removed:** the **Accounts** and **Other** file groups (no user files or
  non-`char` files as clickable list items).
- **Kept as escape hatches:** **Open file…** (dialog; routes any `.dat` to the
  correct slot generically — for backups/anomalous/account files with no local
  character), the **Accounts** button (now the pairing manager, §5), the **Batch
  apply** button (already character-centric — untouched), refresh, refresh names.

## 4. Opening a character

The main list item calls `openCharacter(path)` — a thin specialization of today's
`openFile`: open the char file into the char slot, then run the existing
`reconcileUserSlot`, which loads the paired account file into the user slot (or
leaves it empty when the character is unpaired). No change to reconcile logic; it
already does exactly this on a char open.

The **Open file…** dialog keeps the generic `openFile(path)` path (kind derived
from the name) so an arbitrary `.dat` — including a raw account file — can still
be opened directly.

## 5. Unpaired characters

A character always opens. **Layout and Tree (char file) work immediately.**
**Overview and Autofill need the account file**, so their tabs are always
*visible* once a character is open, but when the character is unpaired they render
an inline pairing prompt instead of the editor:

> Link **Jita Trader** to an account to edit shared settings. **[Pair…]**

- Overview: `OverviewView` already shows an "Accounts nudge" when `userOpen` is
  false — reword it to this prompt; `[Pair…]` calls the existing
  `onShowAccounts` to enter the Accounts view.
- Autofill: add the same prompt to `AutofillView` for the `!userOpen` case.

Pairing itself is unchanged — the **Accounts view** (alias editing, the three
character slots, guided capture) is the pairing manager, reached from the prompt
or the sidebar button. After the user pairs the character, reopening it (or a
roster-driven reconcile) loads the account file and the editors appear.

## 6. Shared-account labelling

Account-scoped surfaces — the Autofill editor and the account-global columns in
Overview — carry a persistent header naming the siblings an edit also affects:

> **Shared account settings** — also applies to Amarr Alt, Nullsec Ratter

No confirmation modal; edits apply immediately. The sibling list is
`associatedCharacters(userId, roster)` minus the current character, mapped to
names through the existing names store. When the account has no other known
characters, the header omits the "also applies to" clause (still labels the
section as shared account settings).

## 7. Raw account-file tree (escape hatch, kept)

The Tree view shows the **character** file by default. To retain raw editing of
the account file, the Tree view gains a small local **Character file / Account
file** switch, shown only when an account file is loaded. Flipping it to Account
sets `active = "user"` for the Tree view (per §2's rule); flipping back restores
`active = "char"`. This replaces the old global toggle with a Tree-scoped one, so
the top-level model stays character-centric while power users keep full raw access
to both documents.

## 8. Deleted / unchanged

**Deleted:** the Character/Account header toggle; the sidebar's Accounts and Other
groups; the "open a user file as a primary target" mental model (the dialog path
still opens one generically).

**Unchanged:** the two-slot backend and all Tauri commands; `reconcileUserSlot` /
`reconcileCharSlot` / `pairedFilePath` / `userSlotFor` / `charSlotFor`; the
Layout, Overview, Autofill, and Batch editors' internals; the Accounts view; the
codec; save/backups. Overview and Autofill already operate on the loaded
`char`/`user` slots — they need only the §5 prompt and the §6 label.

## 9. Edge cases

- **Account with no local character.** If none of an account's characters have a
  char file on this machine, there's no entry point to reach it. Accepted known
  limitation; **Open file…** reaches the raw account file if truly needed.
- **Anomalous char file (`id == None`, e.g. `core_char__.dat`).** Kind is still
  char; it lists (when hide-non-standard is off) with its filename, opens for
  char-file editing, and is simply never pairable (no id) — Overview/Autofill stay
  on the pairing prompt. Same tolerance as discovery already has.
- **Character paired to an account whose file is absent from this profile.**
  `reconcileUserSlot` clears the user slot (no path found); the character behaves
  as unpaired for account-scoped editing until the file is present.
- **Two open documents, both dirty.** Save still writes each dirty slot and keeps
  the per-slot "character: unsaved" / "account: unsaved" badges — unchanged.

## 10. Testing

- **Frontend (`node --test`, zero-dep):** the only new pure logic is the
  view→active mapping and the sibling-name derivation for the §6 label; cover both
  as small pure helpers. Existing `overview.ts` reconcile/pairing helpers are
  already tested and unchanged.
- **Manual smoke (live profiles):** pick a paired character → Layout/Overview/
  Autofill all populate, account alias shows in the sidebar, the shared-settings
  label names the right siblings; pick an unpaired character → Layout/Tree work,
  Overview/Autofill show the pairing prompt, pairing through it loads the account
  file; hide-non-standard off reveals backup char files; Open file… still opens a
  raw account file into the account tree.

## 11. Out of scope / deferred

- **Auto-resolving the account without manual pairing** (name-match or
  single-account-in-folder heuristics) — considered and declined for this rework;
  manual pairing stays the prerequisite. Revisit only if the pairing step proves
  to be friction in the live smoke.
- **A dedicated entry point for character-less accounts** — the Open file… escape
  hatch covers the rare case.
- **Overview-depth slice 2** (filter presets + tab→preset mapping) — resumes after
  this rework ships.
