# EVE settings file format notes

Living document. Sources: ntt/reverence (src/blue/marshal.{h,c}), our own
corpus diffing. No character/account names in this file, and — since the
2026-07-13 history scrub — no real numeric character/account/item IDs
either: every ID appearing in examples below is a **synthetic placeholder**
standing in for a real corpus file. Offsets and byte-level claims were
measured on the real files; only the identifying numbers were replaced.

## Status

- 2026-07-12: fresh files from the current Tranquility client confirmed
  blue-marshal (magic 0x7E) — 20/20 recently-written `core_*.dat` files pass
  the magic-byte check. Historical (2020–2022) and fresh snapshots both in
  corpus (586 files each snapshot, all profiles).
- Observation: current-client files are 2–4× larger than their 2022
  counterparts (e.g. char files ~122–175 KB vs ~52–91 KB) — same format,
  more settings.
- Corpus contains two anomalous real files worth keeping as edge cases:
  `core_char_('char', None, 'dat').dat` and the tiny `core_char__.dat` /
  `core_user__.dat`.
- **2026-07-13 — M0 complete.** Format confirmed for the current client;
  decoder coverage 100% (`bmdump scan`: 5022 files scanned across
  historical, fresh-baseline, and experiment snapshots, 0 failures; corpus
  gate test un-ignored and green). Mappings complete: window geometry,
  overview columns (per-tab), suggestion lists, resolution keys (see
  *Mappings*). Name decision recorded: local extraction rejected, ESI
  primary for character IDs (spec §6 revised). Surprises affecting M1:
  (1) leaf values are `(FILETIME, value)` wrapper tuples — the M1 encoder
  and mutation layer must preserve/update them; (2) overview column config
  spans BOTH files (visibility/order per-tab in `core_user`, widths
  per-char in `core_char`) — the OverviewColumns category is two-file;
  (3) INSTANCE/REDUCE objects are pervasive (~94% of historical files
  failed until implemented) — the M1 encoder must re-emit them exactly;
  (4) geometry is absolute pixels with per-window saved resolution —
  batch-apply resolution warnings can compare per window, not per file.
- Known decoder deviation (documented 2026-07-13): cyclic shared references
  (a REF back into its own still-open container — legal for the reference
  decoder, which stores container slots at open) fail with `BadRef`. No
  corpus file contains a cycle; a Rust `Value` tree could not represent one.
- **2026-07-13 — M1a complete.** Native encoder shipped; corpus gate proves
  decode → encode reproduces all 5022 corpus files **byte-identically**
  (tests/corpus.rs `every_corpus_file_reencodes_byte_identically`). The
  `Value` model is fidelity-tagged (Str/StrUcs2/StrTable split, explicit
  Shared/Ref slots, Instance/Reduce split); see "Corpus canonicality
  measurements" below.
- **2026-07-13 — M1b-1 complete.** `settings-model` crate shipped: fidelity-
  gated `Document::load` (Editable only when `encode(decode(bytes))` is
  byte-identical — corpus gate `every_corpus_file_loads_editable`, 5022/5022),
  JSON tree projection, guarded mutations, the spec §5 save chain
  (verify → backup → atomic write, all abort paths integration-tested),
  backups/restore, and profile discovery. blue-marshal additions:
  `Value::bits_eq` (NaN-safe verify) and `DuplicateSharedSlot` promoted to a
  hard decode error (measured: 0 duplicates across 4,986 corpus files with
  shared maps), making `Ref(slot)` unambiguous for the mutation layer.
- **2026-07-15 — M1 manual validation PASSED.** A live `core_user` file was
  edited through the app's full save chain (backup taken, encode-verified,
  atomically written), the EVE client accepted the file, and the injected
  autofill marker appeared in-game with the client otherwise behaving
  normally. This is the spec §8 exit gate: **M1 (M1a + M1b-1 + M1b-2) is
  complete** — the format round-trips, and the client reads what we write.

## Opcode table (from reverence marshal.h, fetched 2026-07-12)

Protocol: magic `0x7E`, then u32 LE shared-object count, then object stream.
Flag `SHARED_FLAG = 0x40` on the opcode byte. Length encoding: one byte, or
`0xFF` + u32 LE.

| Opcode | Value | Meaning |
|---|---|---|
| NONE | 0x01 | None |
| GLOBAL | 0x02 | type/function/class name reference |
| INT64 | 0x03 | 8-byte signed LE |
| INT32 | 0x04 | 4-byte signed LE |
| INT16 | 0x05 | 2-byte signed LE |
| INT8 | 0x06 | 1-byte signed |
| MINUSONE | 0x07 | -1 |
| ZERO | 0x08 | 0 |
| ONE | 0x09 | 1 |
| FLOAT | 0x0A | f64 LE |
| FLOAT0 | 0x0B | 0.0 |
| STRINGL | 0x0D | long string |
| STRING0 | 0x0E | empty string |
| STRING1 | 0x0F | 1-char string |
| STRING | 0x10 | string with byte count |
| STRINGR | 0x11 | global string-table reference |
| UNICODE | 0x12 | unicode string |
| BUFFER | 0x13 | buffer |
| TUPLE | 0x14 | tuple with count |
| LIST | 0x15 | list with count |
| DICT | 0x16 | dict with count; wire order per entry: value, then key |
| INSTANCE | 0x17 | class instance |
| BLUE | 0x18 | blue object |
| CALLBACK | 0x19 | callback |
| REF | 0x1B | shared-object reference |
| CHECKSUM | 0x1C | stream checksum (adler32) |
| TRUE | 0x1F | True |
| FALSE | 0x20 | False |
| PICKLER | 0x21 | standard pickle |
| REDUCE | 0x22 | reduce protocol |
| NEWOBJ | 0x23 | new-style class object |
| TUPLE0 | 0x24 | empty tuple |
| TUPLE1 | 0x25 | 1-element tuple |
| LIST0 | 0x26 | empty list |
| LIST1 | 0x27 | 1-element list |
| UNICODE0 | 0x28 | empty unicode |
| UNICODE1 | 0x29 | 1-char unicode |
| DBROW | 0x2A | database row |
| STREAM | 0x2B | embedded marshal stream |
| TUPLE2 | 0x2C | 2-element tuple |
| MARK | 0x2D | NEWOBJ/REDUCE iterator marker |
| UTF8 | 0x2E | UTF-8 unicode string |
| LONG | 0x2F | big integer |

### Opcode table verification (Task 4, 2026-07-12)

Verified against `vendor/reverence/src/blue/marshal.h` at commit `9ded855`
(ntt/reverence master, shallow clone). All 43 opcode values above, plus
`PROTOCOL_ID = 0x7E` (marshal.h:35) and `SHARED_FLAG = 0x40` (marshal.h:89),
match exactly — no corrections needed. Additional facts from marshal.h:

- Retired values, commented out and never emitted: `0x0C` COMPLEX, `0x1A`
  PICKLE, `0x1D` COMPRESS, `0x1E` UNUSED (marshal.h:50,64,67-68).
- `BLUE` (0x18), `CALLBACK` (0x19) and `PICKLER` (0x21) have **no case** in
  the reference load loop; they fall into `default:` where `constants[]` is
  NULL and decoding fails with "Unsupported type" (marshal.c:800-809). Treat
  as decode errors. (`BLUE` is nonetheless listed in the length-prefixed set,
  marshal.c:1221-1224.)

All line references below are to `vendor/reverence/src/blue/marshal.c` at
the same commit.

## Per-opcode encoding details (verified against marshal.c)

### Stream framing

- Byte 0: magic `0x7E`; anything else (or total size < 6) makes reverence
  fall back to cPickle (marshal.c:467-475). Minimum stream size is 6 bytes.
- Bytes 1-4: `shared_mapsize`, read as **signed** i32 LE (marshal.c:478;
  in practice non-negative) — the number of shared objects in the stream.
- Bounds check: `5 + shared_mapsize*4 <= size` (marshal.c:482).
- Object stream: bytes `[5, size - shared_mapsize*4)`.
- Tail: the last `shared_mapsize * 4` bytes are the shared-object **map**:
  `shared_mapsize` i32 LE slot numbers (marshal.c:489). `end` — the limit
  for all payload reads — is set to the start of this tail (marshal.c:502),
  so **the tail table must be excluded from the object stream's readable
  range**; CHECK_SIZE (marshal.c:150-155) errors past it.

### Length encoding ("count" below)

`READ_LENGTH` (marshal.c:99-108): one unsigned byte; if it equals `0xFF`,
the real length follows as a **signed** i32 LE (4 bytes). This pre-opcode
length is only read for opcodes in the `needlength` set (marshal.c:1221-1224):
TUPLE, DICT, LIST, STRINGL, STRINGR, UNICODE, GLOBAL, STREAM, UTF8, LONG,
REF, BLUE, BUFFER. All other opcodes read fixed-size payloads (or none).
Exception: `STRING` (0x10) is *not* in `needlength` and reads its own bare
one-byte count with **no** 0xFF escape (marshal.c:643-650).

### Shared-object mechanics (checklist item 1)

- The decoder keeps `shared_obj`, an array of `shared_mapsize` slots (all
  NULL initially, marshal.c:505-512), plus a running counter `shared_count`
  starting at 0.
- Objects are flagged shared by OR-ing `SHARED_FLAG` (0x40) into the opcode
  byte; the flag is masked off before dispatch (marshal.c:524-526).
- Population is **store order mapped through the tail table**: when the
  N-th shared-flagged object (N = `shared_count`, 0-based, in stream order)
  is stored, it lands in `shared_obj[shared_map[N] - 1]` (STORE,
  marshal.c:111-125). The tail-map entry translates encounter order to
  slot; entries are validated to lie in `1..=shared_mapsize`
  (marshal.c:492-499), i.e. **map entries are 1-based slot numbers**.
- Sequence/dict/scalar-ish types store at construction time via
  CHECK_SHARED (marshal.c:158-162): LONG (602), STREAM/STRINGL/BUFFER (667),
  LIST0 (714), DICT (733,739), GLOBAL (782), and TUPLE/LIST/DICT containers
  at container-*open* time inside NEW_SEQUENCE (212) — before their elements
  decode, which is what makes cyclic references possible.
- Deferred containers (DBROW/INSTANCE/NEWOBJ/REDUCE) instead *reserve* the
  next map slot at container open (RESERVE_SLOT, marshal.c:128-139, 792) and
  fill it once the object exists (UPDATE_SLOT, marshal.c:143-147, at 907,
  926, 967, 997). Reservation order is therefore still stream-encounter
  order of the opcode byte, even though the object is built later.
- The SHARED_FLAG on opcodes *not* listed above (fixed ints, floats,
  STRING/STRING1/STRINGR, UNICODE*, UTF8, constants) is ignored by the
  reference decoder — no store happens.
- `REF` (0x1B, marshal.c:748-764): payload is a count (READ_LENGTH) used as
  a **1-based** index; valid range `1..=shared_mapsize`; the object is
  `shared_obj[index - 1]`. Referencing a slot not yet populated is an error
  ("Shared reference points to invalid object").

### Scalars

- `INT8`/`INT16`/`INT32`/`INT64` (0x06/0x05/0x04/0x03): 1/2/4/8-byte
  signed little-endian (marshal.c:568-590).
- `FLOAT` (0x0A): 8-byte IEEE-754 double LE (marshal.c:606-610). `FLOAT0`
  (0x0B): no payload, constant 0.0 (marshal.c:1244).
- `MINUSONE`/`ZERO`/`ONE`/`NONE`/`TRUE`/`FALSE` and the empty
  `STRING0`/`TUPLE0`/`UNICODE0`: no payload; served from the constants
  table (marshal.c:1238-1247) via the `default:` branch.

### LONG (0x2F) — checklist item 2

marshal.c:592-604. Length = READ_LENGTH = **byte count** of the payload.
Zero bytes decode to integer 0. Otherwise
`_PyLong_FromByteArray(s, length, /*little_endian=*/1, /*is_signed=*/1)`:
payload is a **little-endian two's-complement** signed big integer, so the
top bit of the *last* byte is the sign. Honors SHARED_FLAG (CHECK_SHARED
at 602).

### UNICODE family (0x12/0x28/0x29) — checklist item 3

- `UNICODE` (0x12, marshal.c:680-688): count = READ_LENGTH = number of
  **2-byte code units (chars), not bytes**; payload is `count*2` bytes
  (CHECK_SIZE(length*2), line 681) of UCS-2 little-endian — decode as
  **UTF-16LE**. (Reference casts to `wchar_t*`, i.e. native LE UCS-2 on the
  Windows-origin data.)
- `UNICODE0` (0x28): no payload, empty string (constants table, line 1247).
- `UNICODE1` (0x29, marshal.c:670-678): exactly 2 bytes = one UTF-16LE code
  unit; no count byte.
- `UTF8` (0x2E, marshal.c:690-694): count = READ_LENGTH = **byte** count;
  payload is UTF-8 bytes.

### STRINGL vs STRING vs BUFFER — checklist item 4

All three yield byte strings; differences are only in length encoding and
shared handling (marshal.c:643-668):

| Opcode | Count encoding | SHARED_FLAG honored? | Notes |
|---|---|---|---|
| STRING (0x10) | its own single byte 0-255, no 0xFF escape | no | "deprecated since machoVersion 213" (644) |
| STRINGL (0x0D) | READ_LENGTH (1 byte or 0xFF + i32 LE) | yes | deprecated, same note (661) |
| BUFFER (0x13) | READ_LENGTH | yes | "type identifier re-used by CCP. treat as string" (663) |
| STREAM (0x2B) | READ_LENGTH | yes | same decode path (658-668), see below |

STRINGL/BUFFER/STREAM share one case body: read `count` raw bytes, done.
Also: `STRING0` (0x0E) = empty, `STRING1` (0x0F) = exactly 1 byte, no count.

### STRINGR (0x11) — string-table reference

marshal.c:630-641. Count = READ_LENGTH = index **directly** into the fixed
global table; index 0 is rejected (`length < 1` → error, line 631), so
valid indices are effectively **1-based** (`1..=255`). The table itself is
*not* in marshal.c: it lives in the reverence tree at `src/strings.py`
(`stringTable`, 256 entries: `None` at index 0 + 255 strings, of which
indices 196-255 are `"#196"`..`"#255"` placeholders), loaded into
`marshal._stringtable` by `src/blue.py:233`. Extracted verbatim to
`crates/blue-marshal/src/string_table.rs` (`STRING_TABLE`, 256 entries,
index 0 = `""` placeholder so wire indices map directly).

### STREAM (0x2B) — checklist item 5

marshal.c:658-668: framing is simply `0x2B` + READ_LENGTH count + `count`
raw bytes. The reference does **not** recurse; the payload is returned as a
plain byte string. By convention the payload is itself a complete marshal
stream (its own `0x7E` magic, shared count, object data, tail map) that the
consumer feeds back through Load — so nested settings values require a
recursive decode of the byte payload as a fresh stream with its own shared
table.

### `0x7E`-prefixed Bytes payloads are NOT nested streams — Task 9 / re-measured

This is a **different mechanism from the `STREAM` opcode above** — keep the
two distinct: `STREAM` (0x2B) is a dedicated opcode whose payload is, by
convention, guaranteed to be a nested marshal stream (previous section).
What follows is about ordinary `BUFFER`/`STRINGL` (opcode 0x13/0x0D)
payloads — decoded as plain `Value::Bytes`, with no opcode-level signal
either way — that merely happen to start with the same `0x7E` magic byte
that begins every marshal stream.

An earlier version of this section claimed many such payloads "are
themselves complete marshal streams" (a "double-marshaled" pattern: a
cached blob storing an already-serialized sub-object). That claim is
**wrong** and is corrected here. Full re-scan of the current corpus
(`testdata/corpus`, all snapshots — historical, fresh-baseline, and the
exp1–exp5 experiment directories added later — 5022 `.dat` files, decoding
every file and recursively visiting the full `Value` tree including Dict
keys/values and Tuple/List/Instance/Stream children):

- **8739** `Value::Bytes` payloads start with the `0x7E` byte.
- **0** of them decode successfully via `blue_marshal::decode` as a nested
  marshal stream.

(The original Task 9 diagnostic, run over just the 1116 historical +
fresh-baseline files before the experiment snapshots existed, found 1942
such payloads on that smaller set — same conclusion, zero decodable, just
counted before the corpus grew. Re-running the identical method against
that same 1116-file subset today still reproduces exactly 1942, confirming
the method is consistent; the 8739 figure is the full current corpus.)

So: `0x7E`-prefixed `Bytes` payloads exist in real settings files, but in
every corpus file observed, none of them are valid nested marshal streams —
the `0x7E` is coincidental (or comes from data that merely starts with that
byte value), not a marker of embedded serialization. There is no confirmed
"double-marshaled settings value" pattern in this corpus; that description
was never traced to reverence/marshal.c or any CCP source and should be
treated as unverified.

Zero negative `Long` values were also found in the same scan (see the
`dump_text` Long-rendering TODO below — left unfixed since the corpus never
exercises it).

**Consequences:**
- `decode` deliberately does **not** attempt to auto-decode `Bytes`
  payloads: bytes stay exact and lossless regardless of what they happen to
  contain, matching the "no lossy conversions" constraint and keeping
  `Value::Bytes` a single, predictable shape. This is correct and unaffected
  by the correction above.
- `dump_text` (value.rs) still attempts one `decode` call on any `Bytes`
  payload starting with `0x7E`, rendering `stream?` + the decoded value on
  success and falling back to normal `Bytes` rendering (printable-quoted or
  hex) on failure. Given the measurement above, **this arm is dead code on
  every real corpus file** — it exists purely as a synthetic-data /
  future-proofing convenience for `dump_text` output readability, not
  because real settings data needs it. It is bounded (`stream_depth <
  MAX_DEPTH`) so it is safe to keep, but it should not be relied on or
  assumed to fire.
- **M1 guidance:** treat `Value::Bytes` as opaque bytes, period. Do not plan
  encoder/mutation logic around an assumption that some `Bytes` payloads are
  secretly nested marshal streams needing their own re-encode step — the
  corpus gives no evidence that pattern exists.

### CHECKSUM (0x1C) — checklist item 6

marshal.c:613-623. Payload: 4 bytes u32 LE = stored adler32 value. Yields
no object (`continue`). Verification as implemented (line 615):

    *(uint32_t *)s != (uint32_t)adler32(1, s, end - s)

i.e. seed = 1 (standard adler32 init, vendored `adler32.c`), coverage =
`[s, end)` where `s` is the position of **the 4 stored checksum bytes
themselves** and `end` is the start of the tail shared map. Note the
self-inclusion: as written, the checked range *includes* the stored
checksum bytes and everything after them, excluding only the tail map. This
looks self-referential; either CCP's writer computes the value to make this
hold, or the opcode simply never appears in settings files (reverence also
exposes a `skipcrc` Load flag, marshal.c:1140-1142, suggesting it is
routinely bypassed). **M1 encoder guidance: do not emit 0x1C**; it is
optional, and if we ever must, verify the exact coverage empirically
against a real client-written file first. Decoder: read 4 bytes, optionally
verify per the reference formula, produce nothing.

### DICT (0x16) — wire order confirmed

marshal.c:725-741 + POPULATE_DICT (183-197) + inner-loop case (1061-1063).
Count = READ_LENGTH = number of **entries**; the container then consumes
`count*2` objects. Wire order per entry is **value first, then key**
(macro comment line 182: "counted (val,key)"; the first-arriving object is
parked in `obj2`, the second becomes the dict key at 1062). Empty dict
(count 0) is constructed immediately. Confirms our empirical observation.

### TUPLE/LIST (0x14/0x15 + fixed variants)

Count = READ_LENGTH = element count; elements follow in order
(marshal.c:700-723, filled at 870-877). `TUPLE1`/`TUPLE2`/`LIST1` have
implicit counts 1/2/1 and no count byte; `TUPLE0`/`LIST0` are empty.
NEW_SEQUENCE does a sanity CHECK_SIZE(count) before allocating (202-213).

### INSTANCE / GLOBAL / NEWOBJ / REDUCE — checklist item 7

- `GLOBAL` (0x02, marshal.c:766-784): count = READ_LENGTH, payload =
  `count` bytes, a dotted name (`module.object` or a builtin name) resolved
  to a Python object. Honors SHARED_FLAG. For our decoder: keep the name.
- `INSTANCE` (0x17, marshal.c:787-793 open; 913-944 fill): no immediate
  payload; reserves a shared slot, then exactly **two objects** follow:
  (1) the class name (string object; resolved via find_global, instance
  created and shared slot filled *before* the state decodes, 917-929), then
  (2) the state object, applied via `__setstate__` or `__dict__.update`
  (931-940).
- `NEWOBJ` (0x23, open 788-793; fill 947-982): reserves a slot, then one
  **tuple** follows shaped `(args_tuple[, state])` where `args_tuple[0]` is
  the class (`cls.__new__(*args_tuple)`, state applied if present). After
  that, an iterator tail: zero or more objects appended as list items,
  `MARK` (0x2D), zero or more **key, value** pairs (key first here —
  "iterated (key,val)", line 182; POPULATE_DICT at 1057), terminated by a
  second `MARK` (1014-1058).
- `REDUCE` (0x22, open 789-793; fill 985-1010): identical framing to
  NEWOBJ, but the tuple is `(callable, args_tuple[, state])` and the object
  is `callable(*args_tuple)`. Same MARK-delimited list-then-dict iterator
  tail.
- `MARK` (0x2D, marshal.c:795-798): no payload, never yields an object;
  only meaningful inside the NEWOBJ/REDUCE iterator tails.
- `DBROW` (0x2A, open 786-793; fill 881-910), for completeness: one object
  follows (a DBRowDescriptor, typically built via the above), then a
  READ_LENGTH-prefixed blob of packed (CCP zero-compressed) row data read
  inline from the stream, then `descriptor.rd_num_objects` trailing
  non-scalar objects. Complex; hope settings files avoid it.

### GLOBAL and INSTANCE — implemented (Task 9)

Confirmed against real corpus bytes (`core_char_123456789.dat` offset
0x34c, historical snapshot):

- `GLOBAL` decodes to a new `Value::Global(Vec<u8>)` (kept distinct from
  `Bytes` so M1's encoder can re-emit opcode 0x02 rather than a string
  opcode). Observed both un-shared (a one-off callable name) and
  `SHARED_FLAG`-ed (a class/callable name reused across many objects in the
  same file, e.g. `__builtin__.set` cached and REF'd back) — the latter
  needed adding `GLOBAL` to the decoder's `stores_shared` set.
- `INSTANCE` decodes to a new `Value::Instance { class: Box<Value>, state:
  Vec<Value> }`. Confirmed the two children are ordinary objects read
  through the normal recursive path (no MARK/iterator framing, unlike
  NEWOBJ/REDUCE): `class` is the class-name object (observed as a `BUFFER`,
  itself `SHARED_FLAG`-ed since class names repeat across instances — e.g.
  `utillib.KeyVal`), `state` is `vec![the one state object]` (observed as a
  plain `DICT`, e.g. `{"id": "agency", "children": None, "btnType": 1}`).
  Both fields are handled through `stores_shared`'s existing
  reserve-before-children/store-after-completion path (same encounter-order
  mechanics already verified for containers in Task 4) — `INSTANCE`'s own
  RESERVE_SLOT/UPDATE_SLOT (marshal.c:128-147) turned out to need no special
  casing beyond adding it to that match arm.
- No `GLOBAL`-encoded class name was observed for `INSTANCE` in the corpus
  (always a plain string opcode, per marshal.c's `find_global(obj)` where
  `obj` can be any string-typed object) — implementing `INSTANCE` alone
  (with `GLOBAL` available for anything nested in its `state`) was
  independently verified to close all corpus files that don't also contain
  a nested `REDUCE`.

### REDUCE — implemented (Task 9)

Confirmed against real corpus bytes (`core_char_123456788.dat` offset
0x22, historical snapshot, and `core_user_987654.dat` offset 0x684, fresh
snapshot — both reconstructing a Python `set` via `__builtin__.set`):

- Decodes to the same `Value::Instance { class, state }` shape as INSTANCE:
  `class` is the whole ctor object as decoded — a `Tuple` of 2 elements,
  `(callable, args)`, observed as `(Global("__builtin__.set"), Tuple([the
  set's elements as a List]))` — since the wire only ever hands REDUCE one
  plain object here (the tuple itself), not two separate class/state reads
  like INSTANCE. `state` holds whatever the list-then-dict iterator tail
  contributes (see below); the field is reused rather than adding a new
  variant, per the brief's guidance to mirror load order generically across
  INSTANCE/NEWOBJ/REDUCE.
- The callable was observed both as a plain `GLOBAL` (unshared) and as
  `GLOBAL|SHARED_FLAG` (cached across multiple REDUCE occurrences in the
  same file) — both handled by the generic recursive decode of the ctor
  tuple's first element, no REDUCE-specific code needed.
- The list-then-dict iterator tail (marshal.c's unconditional switch to
  `LIST_ITERATOR` then `DICT_ITERATOR` after building the object,
  985-1011/1014-1058) is **always present** on the wire — every REDUCE
  observed in the corpus is followed immediately by two consecutive `MARK`
  (0x2D) bytes with nothing in between, i.e. an empty tail. The decoder
  implements the general MARK-terminated loop (not just "expect two MARKs
  immediately") since that's the actual mandatory framing per marshal.c, not
  a speculative extension: it reads objects into `state` until a MARK, then
  `(key, value)` pairs (note: *iterated* order is key-then-value, the
  opposite of plain DICT's counted value-then-key, per the POPULATE_DICT
  macro comment at marshal.c:182) into `state` as 2-element `Tuple`s until a
  second MARK. A non-empty tail is exercised only by a synthetic unit test
  (`decodes_reduce_with_nonempty_iterator_tail`), never by the corpus.
- `NEWOBJ` (0x23) shares this exact framing (`(args_tuple[, state])` instead
  of `(callable, args_tuple[, state])`, per marshal.c:947-982) but is never
  observed in the corpus, so it is left `Unsupported` — implementing it
  would be speculative (YAGNI). Likewise `DBROW` (0x2A) is left
  `Unsupported`; it never appears either.

## Decoder coverage log

Task 8 baseline (2026-07-12): scanned 1116, ok 70, failed 1046.
Distinct error kinds:
- `INSTANCE` (694 files): offsets 0x34c, 0x113f, 0x795, 0xd716, 0xd6d4, 0xd98e, etc.
- `REDUCE` (352 files): offsets 0x22, 0x2d, 0xcc, 0x138f8, 0xebed, 0xfc42, etc.

Task 9 increment 1 (2026-07-12), GLOBAL + INSTANCE implemented: scanned
1116, ok 550, failed 566. Remaining failures are all `Unsupported("REDUCE")`
(352 top-level, plus files where a REDUCE is nested inside what used to be
the first INSTANCE failure).

Task 9 increment 2 (2026-07-12), REDUCE implemented: scanned 1116, ok 1116,
failed 0. Full corpus coverage reached — both the historical and fresh
snapshots decode cleanly, including the two anomalous files
(`core_char_('char', None, 'dat').dat` and `core_char__.dat`).

## Corpus canonicality measurements (2026-07-13, M1a)

Instrumented decoder run over all 5022 corpus files (0 decode failures),
taken before the encoder was designed; these facts justify the encoder's
canonical opcode rules and the fidelity tags on `Value`. The byte-identical
round-trip gate (tests/corpus.rs) re-proves all of them on every run, so a
future client patch that breaks one fails loudly there.

| Measurement | Count |
|---|---|
| READ_LENGTH 0xFF escapes / of which non-minimal (< 255) | 228,549 / **0** |
| Streams with slack between root object and tail map | **0** |
| Tail-map entries with slot ≠ encounter-index+1 | **963,660** |
| SHARED_FLAG on opcodes the reference ignores it for | **0** |
| STREAM (0x2B) / STRING (0x10) / STRINGL (0x0D) opcodes | **0** / **0** / **0** |
| BUFFER with payload ≤ 1 byte | **0** |
| Counted TUPLE with n ≤ 2 / counted LIST with n ≤ 1 | **0** / **0** |
| INTn holding a value a narrower opcode could hold | **0** |
| LONG with 0 payload bytes | **0** |
| FLOAT whose payload is +0.0 bits | **0** |
| UTF8 opcodes / with 0 bytes | 11,030,207 / 14,355 |
| UNICODE (0x12) / UNICODE0 / UNICODE1 opcodes | **0** / 1,458 / 15,480 |
| STRINGR opcodes | 71,847 |
| Non-STRINGR strings whose content equals a table entry | 1,269 |

Encoder consequences:

- **Canonical (no tag needed):** length encoding always minimal; `Int` by
  magnitude (constants, then narrowest INTn); `Float` +0.0 bits → FLOAT0;
  `Bytes` by length (STRING0/STRING1/BUFFER); `Tuple`/`List` by count.
- **Tagged (content cannot recover the opcode):** `Str` (UTF8) vs
  `StrUcs2` (UNICODE0/UNICODE1/UNICODE by UTF-16 unit count) vs
  `StrTable` (STRINGR index) — the client emits both UTF8("") and UNICODE0,
  and writes table-content strings as UTF8 too (1,269 collisions), so
  "look it up in the table" would mis-encode.
- **Explicit sharing:** tail maps are heavily non-identity, so decoded trees
  carry `Shared { slot }` wrappers and `Ref(slot)` nodes; the encoder
  replays slots in encounter order. SHARED_FLAG is never emitted on
  non-storing opcodes (and never observed there).
- **Slack promoted to hard error:** `ErrorKind::TrailingBytes` — measured
  zero occurrences, so any slack now means a mis-parse.

## Mappings

All paths below are dict-key sequences from the file root (the root of every
`core_char_*.dat` decodes to a Dict). Established by diffing before/after
dumps around single in-game changes (Task 10 experiments).

### Value-wrapper convention (applies file-wide)

Most leaf settings are stored as a 2-tuple `(timestamp, value)`, where
`timestamp` is a LONG holding a Windows FILETIME (100 ns ticks since
1601-01-01 UTC). The client rewrites these timestamps for many entries on
every save, so timestamp churn is the dominant diff noise between two saves
of the same file. Editors must preserve the wrapper (rewriting the timestamp
on edit is what the client itself does; keeping the old one also appears
harmless — not yet verified).

### Window geometry (experiments 1–2: moved, then resized the overview window)

- File: `core_char_<id>.dat` (character-scoped; the paired `core_user` file
  was also rewritten in the session but contains no geometry).
- Path: root → `b"windows"` → `b"windowSizesAndPositions_1"` →
  `(timestamp, dict)`; the inner dict maps window id → 6-tuple
  `(x, y, width, height, screenW, screenH)`, all absolute pixels
  (observed: overview `(2114, 424, 446, 1016, 2560, 1440)` on a 2560×1440
  client; moving the window vertically changed only element 1 (y): 0 → 424).
- Resize confirmation (experiment 2): resizing changed the tuple to
  `(1707, 288, 853, 1152, 2560, 1440)` — w/h are elements 2–3; x/y moved
  too because the drag origin moved. Apart from timestamp churn this tuple
  was the *only* change in the file, so geometry is fully self-contained
  here (no shadow copies elsewhere).
- `screenW`/`screenH` are the client resolution the geometry was saved at,
  embedded per window — this is the resolution source for the layout
  canvas (spec §6); there is no separate global resolution key needed.
- Window ids: plain byte-strings for singleton windows (`b"overview"`,
  `b"overview_1"` for a second overview window, `b"fitting"`, …) and
  stringified Python tuples (Str, e.g. `"('corpassets', 1000000000001L)"`)
  for parameterized windows. Both kinds appear as keys of the same dict.
- Generic across windows (experiment 5): moving the market window changed
  only its own 6-tuple in the same `windowSizesAndPositions_1` dict (x/y
  `(1544, 55)` → `(16, 825)`) plus its `b"openWindows"` flag — every
  window uses the same structure, so the layout canvas can enumerate this
  dict generically; no per-window special cases.
- Stored window flags live in sibling `(timestamp, dict-by-window-id)`
  entries under root → `b"windows"`: `b"openWindows"`,
  `b"collapsedWindows"`, `b"minimizedWindows"`, `b"lockedWindows"`,
  `b"compactWindows"`, `b"isOverlayedWindows"`,
  `b"isLightBackgroundWindows"`, `b"stacksWindows"` (values bool, except
  stacksWindows: stack id). These are the spec's WindowLayout flag fields.
- Overview column *widths* observed under root → `b"ui"` →
  `b"SortHeadersSizes"` / `b"SortHeadersSettings2"` keyed by tuple
  `(b"overviewScroll2", presetIndex)` → dict of column-name → width px
  (details to be confirmed in experiment 3).

### Overview columns (experiments 3a–3b: added a column, reordered columns)

Column visibility and order are **per overview tab**, stored in
`core_user_<id>.dat` (account-scoped), with widths per character in
`core_char_<id>.dat`:

- Visible set + order: user-file root → `b"overview"` →
  `b"tabsettings_new"` → `(timestamp, dict)` keyed by tab index (Int).
  Each tab is a dict: `"name"` (Str label), `b"bracket"` (bracket preset
  name), `b"color"` (None or 3-float RGB tuple), `b"overview"` (overview
  preset name), `b"showAll"`/`b"showNone"`/`b"showSpecials"` (bools),
  `b"tabColumnOrder"` (list of column-name Bytes, full ordering) and
  `b"tabColumns"` (list of column-name Bytes, the **visible** set — adding
  Transversal Velocity to one tab appended `b"TRANSVERSALVELOCITY"` here).
  A legacy sibling `b"tabsettings"` also exists (older shape, still
  rewritten by the client).
- Per-tab keys are sparse with inheritance (experiment 3b): a tab without
  its own `b"tabColumnOrder"`/`b"tabColumns"` inherits the account defaults
  below; the first drag-reorder on such a tab **creates** the tab's own
  `b"tabColumnOrder"` (observed on tab 0: full 14-column list written with
  the dragged column in its new slot). Reordering touched **only** the
  user file — the char file had no non-timestamp change.
- Account-level defaults: user-file root → `b"overview"` →
  `b"overviewColumns"` (visible set) and `b"overviewColumnOrder"` (order)
  — did **not** change when a single tab's columns were edited; they appear
  to be the defaults applied to tabs without their own settings.
- Widths: char-file root → `b"ui"` → `b"SortHeadersSizes"` →
  `(timestamp, dict)` keyed by tuple `(b"overviewScroll2", tabIndex)` →
  dict column-name → width px (adding the column created
  `b"TRANSVERSALVELOCITY": 159` for the edited tab). Sibling
  `b"SortHeadersSettings2"` has the same keying and holds per-tab sort
  state. Width entries appear lazily when a tab is rendered.
### Autofill / remembered-string lists (experiment 4: ran a People & Places search)

All remembered text-input history in the client is **one structure**, in
`core_user_<id>.dat` only (no `editHistory` key exists in the char file):

- Path: user-file root → `b"ui"` → `b"editHistory"` → `(timestamp, dict)`;
  the inner dict is keyed by UI widget path (Bytes, e.g.
  `b"/addressbook/content/main/SearchPanel/Container/SingleLineEditText"`)
  → list of remembered strings (Str; occasionally an empty Bytes entry).
  New entries append at the end of the widget's list (the test search
  appeared as the last element of the People & Places search list).
- ~40 widget-path lists observed in one real file: People & Places
  searches, inventory quick-filters, structure browser search, skill
  catalogue search, fleet/fitting names, wallet transfer "reason",
  overview-export filename, chat channel names, bug-report title, etc.
  This whole dict is the spec's `SuggestionLists` category: the editor can
  enumerate the keys generically and offer per-list add/remove/reorder/
  clear without a hardcoded list of widgets.

- Overview *filter presets* (tab contents) live in user-file root →
  `b"overview"` → `b"overviewProfilePresets"` (dict keyed by preset-name
  Str) with `b"overviewProfilePresets_notSaved"`, `b"presetHistoryKeys"`,
  `b"restoreData"` as session-state siblings — raw-tree-only in V1 per
  spec §6.

### Name presence (Task 11 decision: local extraction NOT viable)

Investigated on fresh current-client files with the account's real
character names known out-of-band (names verified interactively, never
recorded here):

- The char file does **not** store its own character's name structurally.
  It does not even contain its own character ID — the ID exists only in
  the filename.
- The user file has **no** characterID→name structure at all; none of the
  account's three character IDs appear anywhere in its decoded tree.
- Every name occurrence found is incidental UI state: chat-channel labels
  (a channel named after the character), container-window ids of the form
  `containerWnd_<CharacterName>'s Capsule` (capsule cargo windows, in the
  **user** file), station-container keys embedding pilot names, and
  editHistory search strings (names of *other* players searched for).
- `core_public__.yaml` (machine audio/device settings, same
  `(FILETIME, value)` convention in YAML) and `prefs.ini` contain no names.

**Decision for spec §6:** ESI `POST /universe/names` is the primary
resolution source for character IDs (batched, cached, offline-safe,
disableable); account IDs get user aliases. Local extraction is demoted to
a *suggestion-only* heuristic.

**Account↔character correlation (works):** at logout the client writes the
played character's `core_char` and its account's `core_user` within a few
seconds of each other (observed 3 s apart), so mtime clustering is a solid
"account of character <id>" suggestion. Weak corroborating in-file hint:
the user file's capsule container-window ids embed the account's own
characters' names (unreliable — capsules can be renamed; suggestion only,
per spec §6's confirm-into-alias flow).

**Guided-capture account-write trigger (M3b live smoke, 2026-07-15):** the
controlled-logout capture needs an *account-level* change so `core_user`
advances (the played character's `core_char` mtime advances on logout
regardless, which identifies the character). Confirmed reliable trigger:
toggling **Camera Shake** (Settings → Display & Graphics), an account-scoped
graphics setting — this is the example the app's capture dialog names. Other
account-level writers per the mappings above: any Display/Graphics or Audio
setting; an overview column add/remove/reorder (`overview → tabsettings_new`;
the char file only timestamp-bumps); an overview filter-preset edit
(`overview → overviewProfilePresets`); or appending to `ui → editHistory` by
typing into an autocompleting field. (M3b dropped the passive name-match
suggestion tier — parsing every user file froze the UI — so manual pairing +
this capture are the association paths that shipped.)
