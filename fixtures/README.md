# Synthetic corpus

Committed, deterministic settings files that stand in for the real corpus.

Regenerate with:

```
cargo run -p settings-model --bin gen_fixtures
```

## Why it exists

The real corpus lives in `testdata/`, which is gitignored because it is personal
data. Every corpus gate therefore used to return early on a missing `testdata/`
— which meant the four strongest tests in the project asserted **nothing** on CI
or on any machine but the author's.

It was also enormously redundant: **6140 `.dat` files across eleven snapshots,
of which 413 are distinct by content** (93 % duplicate, 574 MB). Decoding the
other 93 % over and over cost about five minutes per `cargo test` and proved
nothing new.

So: the synthetic corpus is always present and covers the shapes, the real
corpus still runs when checked out and is deduplicated by content hash.

| | files | gate time |
|---|---|---|
| before | 6140 real, 0 synthetic | ~295 s, skipped entirely on CI |
| after | 13 synthetic + 413 distinct real | ~30 s locally, ~0.1 s on CI |

## Layout

```
fixtures/synthetic/
  codec/                     wire-format coverage
    scalars.dat              every scalar opcode, incl. negative + short LONG
    containers.dat           tuple/list/dict variants + the 0xFF length escape
    sharing.dat              non-identity tail map; Ref as timestamp, as dict
                             key, and inside a tuple key
    objects.dat              GLOBAL / INSTANCE / REDUCE (empty and non-empty
                             iterator tail) / STREAM
    odd_keys.dat             every dict-key kind, incl. the None key and the
                             tuple key real account files carry
  decode-only/
    deprecated_strings.dat   hand-authored STRING/STRINGL bytes
  profile/settings_Default/
    core_char_90000001.dat   modern character: all flag dicts, a stack, both
                             HUD anchors, per-tab widths and sort, dockPanels
    core_char_90000002.dat   minimal character: absent means default
    core_char_90000003.dat   legacy character: 1920x1080, Int-where-Float
    core_user_80000001.dat   modern account: full overview container, keybinds,
                             audio, suppress, tabgroups, autofill, graphics
    core_user_80000002.dat   clean account: empty overview container
    core_user_80000003.dat   legacy account: `tabsettings` only, Int booleans
    core_user_80000004.dat   interned account: every list behind a Shared/Ref
```

Every id is synthetic (characters `9000000x`, accounts `8000000x`). Nothing here
was copied from a real file.

## Which gates consume them

| Gate | What it asserts |
|---|---|
| `blue-marshal/tests/corpus.rs` | decodes, re-encodes byte-identically, survives `reshare` |
| `settings-model/tests/corpus_load.rs` | loads `Editable` (and `decode-only/` loads `ReadOnly`) |
| `settings-model/tests/projection_smoke.rs` | every projection runs, and each fixture yields the data it was built to contain |
| `settings-model/tests/mutation_smoke.rs` | every write path survives mutate → reshare → encode → decode → verify, and the edit is still there afterwards |
| `settings-model/tests/hud_corpus.rs` | each HUD anchor projects from at least one fixture |
| `settings-model/tests/overview_pack_corpus.rs` | every account file round-trips as a pack |

The two `_smoke` gates are the ones that catch the "passes every hand-built unit
test while reading nothing from a real file" class. Both have already caught
shipped bugs on their first run.

## What the fixtures prove, and what they do not

`codec/` and `profile/` are **golden files**: a `Value` tree in
`gen_fixtures.rs`, run through `blue_marshal::encode`, and committed. Over them,
the byte-identity gate is a *regression* check — encoder output for these shapes
must not drift — and a decoder-coverage check. It is **not** independent evidence
that our bytes match the client's. Only the real corpus proves that, and it still
runs locally.

`decode-only/` is hand-authored wire bytes for opcodes the encoder never emits
(the deprecated `STRING` / `STRINGL`). Those bytes cannot survive a byte-identity
check by construction, so the gate skips that directory by name, and
`corpus_load.rs` asserts the inverse: they must load, and must load `ReadOnly`.

Several shapes here exist in **no** real corpus file and are covered nowhere else:
a negative `LONG`, a zero-byte `LONG`, a non-empty `REDUCE` iterator tail, the
`STREAM` opcode, and the deprecated string opcodes.

## Running only the synthetic corpus

```
EVE_SYNTHETIC_ONLY=1 cargo test --workspace
```

Ignores `testdata/` even when it is present: reproduces exactly what CI sees and
turns the full suite into a ~2 second loop.

## Adding a fixture

Add the tree to `gen_fixtures.rs`, add its filename to
`synthetic_corpus_is_complete` in `crates/blue-marshal/tests/corpus.rs` (so a
deleted fixture fails loudly instead of quietly shrinking the gates), regenerate,
and commit the bytes. The generator refuses to write a file that does not
round-trip, so a broken fixture never reaches the gates.
