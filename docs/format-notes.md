# EVE settings file format notes

Living document. Sources: ntt/reverence (src/blue/marshal.{h,c}), our own
corpus diffing. No character/account names in this file — numeric IDs only.

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

## Decoder coverage log

(filled by Tasks 8–9)

## Mappings

(filled by Tasks 10–11)
