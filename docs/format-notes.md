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

Per-opcode encoding details (payload layout, shared-map mechanics, index
bases, sign handling) are verified against vendored marshal.c in Task 4 and
recorded below as they are confirmed.

## Decoder coverage log

(filled by Tasks 8–9)

## Mappings

(filled by Tasks 10–11)
