# MCP: the remaining editors — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the layout (with a rendered picture), autofill, keybinds, neocom, HUD, fleet and chat editors, plus copy-settings and settings presets, behind the existing MCP server — 22 new tools, 46 in total.

**Architecture:** Every new tool is another arm in `mcp.rs`'s `call` over an existing `ops`/`setup`/`presets` function, following slice 1's `_get` + batched `_edit` convention. Three pieces of new mechanism: the batch runner takes a `finish` closure so batches can re-project any editor; `call_tool` promotes a `png_base64` field into an MCP image block so `layout_render` can return a picture; and `mcp_render.rs` draws that picture in pure Rust (a 5×7 bitmap font, the `png` crate). Layout geometry and flags are written by building the same mutations the canvas builds, from paths the model never sees.

**Tech Stack:** Rust (`app` crate), `rmcp` 3, `serde_json`, `png` 0.18 + `base64` 0.22 (already in the lockfile via Tauri), Svelte/vitest for the one frontend change.

**Spec:** `docs/superpowers/specs/2026-09-20-mcp-all-surfaces-design.md` — read it first; slice 1's spec (`2026-09-19-mcp-server-design.md`) §3.1 holds the conventions it inherits.

## Global Constraints

- Branch: `feat/mcp-all-surfaces` (exists; the spec is committed on it). Work in the main checkout — `target/` is warm there and worktree caches have filled the disk before.
- Tool names `^[a-z][a-z0-9_]*$`, ≤ 40 chars. Schemas: hand-written `json!`, `type: object`, `additionalProperties: false`, explicit `required`, **no** `oneOf`/`anyOf`/`allOf`/`$ref`/`"null"`/type arrays; optional arguments omitted, never null. `schema_invariants_hold_for_every_tool` pins this for every entry of `tool_defs()`.
- Domain errors are tool results `{"code","message"}` (`fail`/`err`); protocol codes `unknown_tool`, `bad_arguments`, `missing_field`.
- Batched `_edit` tools: `ops: [{op, …}]` with `minItems: 1`, one `undo::group`, atomic, `op_index` on failure, return the editor's re-projected model.
- **Paths never reach the model**: no `NodePath`, no `set`, no `*_path` key in any `_get` result.
- **Immediate writes are named**: `copy_apply`, `copy_files` and every `settings_preset_edit` op say "WRITES TO DISK IMMEDIATELY" in their descriptions and tell the model to `open` a target again if it was open.
- Nothing in `ops.rs` or `crates/` changes. Direct deps added: `png = "0.18"`, `base64 = "0.22"` — both already in `Cargo.lock`; nothing else.
- Descriptions are the product — copy them verbatim from this plan.
- CI gates by exit code: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `app/`: `npm run check`, `npm test`.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Final tool count: **46**.

---

## File map

| File | Responsibility |
|---|---|
| `app/src-tauri/Cargo.toml` | `png`, `base64` |
| `app/src-tauri/src/mcp.rs` | tool table + dispatch (one file, so the invariants test has one source), views, op appliers, batch runner, image promotion, tests |
| `app/src-tauri/src/mcp_render.rs` | **new** — `layout_png`, the bitmap font, the legend, unit tests |
| `app/src-tauri/src/mcp_primer.md` | three new sections, the workflow paragraph |
| `app/src-tauri/src/names.rs` | `lookup_with` and `EsiName` become `pub(crate)` |
| `app/src-tauri/tests/mcp_stdio.rs` | 46 names |
| `app/src/lib/data/vk-labels.json` | **new** — the key table, moved from `keybinds.ts` |
| `app/src/lib/keybinds.ts` | imports the JSON |
| `README.md`, `CHANGELOG.md` | docs |

Throughout, "the helpers" means what `mcp.rs` already has: `Args`, `ToolResult = Result<Value, Value>`, `fail(ErrDto) -> Value`, `err(code, msg) -> Value`, `ok(T) -> ToolResult`, `req<T>(&Args, key)`, `opt<T>(&Args, key)`, `obj(properties, required)`, `op_item(ops, fields)`, `ToolDef { name, description, schema }`, `tool_defs()`, `unknown_op(op)`, `off_runtime(work)`, `EveMcp::call`, `EveMcp::batch`, and in `mod tests`: `open_user(bytes) -> (EveMcp, PathBuf)`, `args(Value) -> Args`, `overview_user_bytes()`, `empty_ui_bytes()`, `use crate::testkit::{b, temp_file}; use blue_marshal::{encode, Value as BmValue};`.

---

### Task 1: Batch runner takes a `finish` closure; image blocks; the two crates

Spec §1 (conventions), §2.1 (rendering's reply variant). Delivers the mechanism every later task uses, with no behaviour change to the 24 shipped tools.

**Files:**
- Modify: `app/src-tauri/Cargo.toml`
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `fn batch(&self, args: &Args, apply: fn(&AppState, &Args) -> Result<(), Value>, finish: impl FnOnce(&EveMcp) -> ToolResult) -> ToolResult`; `fn blocks(v: Value) -> Vec<ContentBlock>` (a `png_base64` string field becomes an `image/png` block and is removed from the text); `const PNG_KEY: &str = "png_base64"`.

- [ ] **Step 1: Add the crates**

In `app/src-tauri/Cargo.toml` under `[dependencies]`, after `dirs`:

```toml
# layout_render (mcp_render.rs): a PNG of the window layout as an MCP image
# block. Both crates are already in the lockfile through Tauri's icon handling.
png = "0.18"
base64 = "0.22"
```

- [ ] **Step 2: Failing tests**

Add to `mod tests` in `mcp.rs`:

```rust
    #[test]
    fn a_png_base64_field_becomes_an_image_block_and_leaves_the_text() {
        let v = json!({ "legend": 1, "png_base64": "iVBORw0KGgo=" });
        let blocks = blocks(v);
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Text(t) => {
                assert!(t.text.contains("\"legend\": 1"));
                assert!(!t.text.contains("png_base64"), "the image data is not repeated as text");
            }
            other => panic!("first block should be text, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Image(i) => {
                assert_eq!(i.mime_type, "image/png");
                assert_eq!(i.data, "iVBORw0KGgo=");
            }
            other => panic!("second block should be an image, got {other:?}"),
        }
        assert_eq!(blocks(json!({ "a": 1 })).len(), 1, "no image field, one text block");
    }
```

`TextContent`/`ImageContent` field names: check `rmcp::model::{TextContent, ImageContent}` (`text`, and `data` + `mime_type`); adjust the accessors if they differ and say so in the report.

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p app --lib mcp::a_png_base64` — expected: compile error, `blocks` undefined.

- [ ] **Step 4: Implement `blocks`, generalise `batch`**

In `mcp.rs`, next to `pretty`:

```rust
/// A tool that returns a picture puts it under this key as base64 PNG;
/// `blocks` lifts it into an MCP image block so the text never carries it.
const PNG_KEY: &str = "png_base64";

/// The content blocks for one tool result: the JSON as text, plus an image
/// block when the result carries one.
fn blocks(mut v: Value) -> Vec<ContentBlock> {
    let png = v.as_object_mut().and_then(|m| m.remove(PNG_KEY)).and_then(|p| p.as_str().map(str::to_owned));
    let mut out = vec![ContentBlock::text(pretty(&v))];
    if let Some(data) = png {
        out.push(ContentBlock::image(data, "image/png"));
    }
    out
}
```

In `call_tool`, replace the `Ok(v) =>` arm with `Ok(v) => CallToolResult::success(blocks(v)),`.

Change `batch`'s signature and tail:

```rust
    fn batch(
        &self,
        args: &Args,
        apply: fn(&AppState, &Args) -> Result<(), Value>,
        finish: impl FnOnce(&EveMcp) -> ToolResult,
    ) -> ToolResult {
        // … the loop is unchanged …
        finish(self)
    }
```

and the four overview arms in `call` become `self.batch(args, columns_op, |s| s.overview_get())` etc.

- [ ] **Step 5: Run the suite**

Run: `cargo test -p app --lib mcp::` — expected: all pass (the 41 from slice 1 plus the new one). Then `cargo clippy --workspace --all-targets -- -D warnings`.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/mcp.rs
git commit -m "MCP: batch takes a finish closure; a png_base64 field becomes an image block

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: The shared key table — `vk-labels.json`

Spec §2.3. Delivers one source of truth for key codes, read by the frontend and (from Task 4) by the server.

**Files:**
- Create: `app/src/lib/data/vk-labels.json`
- Modify: `app/src/lib/keybinds.ts:66-83`
- Modify: `app/src-tauri/src/mcp.rs` (the `include_str!` and a parse test)

**Interfaces:**
- Produces: `const VK_LABELS_JSON: &str`, `fn vk_labels() -> HashMap<i64, String>` (code → label), `fn vk_code(name: &str) -> Option<i64>` (label, case-insensitive → code) in `mcp.rs`.

- [ ] **Step 1: Move the table**

Create `app/src/lib/data/vk-labels.json` with the exact entries of `VK_LABELS` in `keybinds.ts` (lines 67–83), as a JSON object keyed by the decimal code as a string:

```json
{
  "8": "Backspace", "9": "Tab", "13": "Enter", "19": "Pause", "20": "Caps Lock", "27": "Esc",
  "32": "Space", "33": "Page Up", "34": "Page Down", "35": "End", "36": "Home",
  "37": "Left", "38": "Up", "39": "Right", "40": "Down",
  "45": "Insert", "46": "Delete",
  "48": "0", "49": "1", "50": "2", "51": "3", "52": "4", "53": "5", "54": "6", "55": "7", "56": "8", "57": "9",
  "65": "A", "66": "B", "67": "C", "68": "D", "69": "E", "70": "F", "71": "G", "72": "H", "73": "I",
  "74": "J", "75": "K", "76": "L", "77": "M", "78": "N", "79": "O", "80": "P", "81": "Q", "82": "R",
  "83": "S", "84": "T", "85": "U", "86": "V", "87": "W", "88": "X", "89": "Y", "90": "Z",
  "96": "Num 0", "97": "Num 1", "98": "Num 2", "99": "Num 3", "100": "Num 4", "101": "Num 5",
  "102": "Num 6", "103": "Num 7", "104": "Num 8", "105": "Num 9",
  "106": "Num *", "107": "Num +", "109": "Num -", "110": "Num .", "111": "Num /",
  "112": "F1", "113": "F2", "114": "F3", "115": "F4", "116": "F5", "117": "F6",
  "118": "F7", "119": "F8", "120": "F9", "121": "F10", "122": "F11", "123": "F12",
  "144": "Num Lock", "145": "Scroll Lock",
  "186": ";", "187": "=", "188": ",", "189": "-", "190": ".", "191": "/", "192": "`",
  "219": "[", "220": "\\", "221": "]", "222": "'"
}
```

In `keybinds.ts`, replace the `const VK_LABELS: Record<number, string> = { … };` literal with:

```ts
import vkLabels from "./data/vk-labels.json";

/** Windows virtual-key codes EVE can store, from the JSON the Rust side also
 *  reads (the MCP server turns "Q" into 81 with it). Serves both display and
 *  capture validation: a code absent here is rejected rather than written
 *  blind. */
const VK_LABELS: Record<number, string> = Object.fromEntries(
  Object.entries(vkLabels as Record<string, string>).map(([k, v]) => [Number(k), v]),
);
```

Keep the import next to the existing `names`/`defaults` JSON imports at the top of the file.

- [ ] **Step 2: Frontend gates**

From `app/`: `npx vitest run src/lib/keybinds.test.ts` — expected PASS unchanged; then `npm run check` — expected exit 0 (if `resolveJsonModule` complains, look at how `command-names.json` is imported in the same file and mirror it exactly).

- [ ] **Step 3: Failing Rust test**

Add to `mod tests` in `mcp.rs`:

```rust
    #[test]
    fn the_key_table_parses_and_maps_names_both_ways() {
        let labels = vk_labels();
        assert_eq!(labels.get(&81).map(String::as_str), Some("Q"));
        assert_eq!(labels.get(&112).map(String::as_str), Some("F1"));
        assert!(labels.len() >= 80);
        assert_eq!(vk_code("q"), Some(81));
        assert_eq!(vk_code("Page Up"), Some(33));
        assert_eq!(vk_code("num 5"), Some(101));
        assert_eq!(vk_code("Hyper"), None);
    }
```

Run: `cargo test -p app --lib mcp::the_key_table` — expected: compile error.

- [ ] **Step 4: Implement**

After `PRESET_NAMES_JSON` in `mcp.rs`:

```rust
/// Windows virtual-key codes → EVE's key names, the table `keybinds.ts`
/// renders and validates with. One file, two readers.
const VK_LABELS_JSON: &str = include_str!("../../src/lib/data/vk-labels.json");

fn vk_labels() -> HashMap<i64, String> {
    let raw: HashMap<String, String> = serde_json::from_str(VK_LABELS_JSON).expect("vk-labels.json");
    raw.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
}

/// A key by the name a person types — "Q", "f1", "page up" — or `None`.
fn vk_code(name: &str) -> Option<i64> {
    let want = name.trim();
    vk_labels().into_iter().find(|(_, label)| label.eq_ignore_ascii_case(want)).map(|(code, _)| code)
}
```

- [ ] **Step 5: Run, clippy, commit**

`cargo test -p app --lib mcp::` green; clippy clean; `npm test` from `app/` exit 0.

```bash
git add app/src/lib/data/vk-labels.json app/src/lib/keybinds.ts app/src-tauri/src/mcp.rs
git commit -m "Key-code table becomes data/vk-labels.json, read by the frontend and the MCP server

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Autofill and keybinds (5 tools)

Spec §2.2, §2.3. Account-file editors.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `vk_labels`, `vk_code` (Task 2); the helpers.
- Produces: `const COMMAND_NAMES_JSON`, `fn command_names() -> HashMap<String, (String, String)>` (command → (label, group)), `fn combo_label(keys: &[i64]) -> String`, `fn keybinds_view(k: &settings_model::Keybinds) -> Value`, tools `autofill_get`, `autofill_set`, `autofill_clear_all`, `keybinds_get`, `keybind_set`.

- [ ] **Step 1: Failing tests**

```rust
    /// root -> ui -> editHistory -> (ts, { "/a/box": ["Jita", "Amarr"] }) —
    /// `ops::tests::autofill_user_bytes`'s shape.
    fn autofill_user_bytes() -> Vec<u8> {
        let hist = BmValue::Dict(vec![(b("/a/box"), BmValue::List(vec![BmValue::Str("Jita".into()), BmValue::Str("Amarr".into())]))]);
        let ui = BmValue::Dict(vec![(b("editHistory"), BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), hist]))]);
        encode(&BmValue::Dict(vec![(b("ui"), ui)])).unwrap()
    }

    /// root -> cmd -> customCmds -> (ts, { command: codes }) — the model
    /// crate's `user_with_binds` shape: `cmd` bare, leaves bare tuples.
    fn keybinds_user_bytes() -> Vec<u8> {
        let codes = |v: &[i64]| BmValue::Tuple(v.iter().map(|&n| BmValue::Int(n)).collect());
        let table = BmValue::Dict(vec![
            (b("CmdActivateHighPowerSlot1"), codes(&[81])),
            (b("CmdActivateMediumPowerSlot1"), codes(&[17, 83])),
            (b("CmdToggleAutopilot"), BmValue::None),
        ]);
        let cmd = BmValue::Dict(vec![(b("customCmds"), BmValue::Tuple(vec![BmValue::Long(vec![0u8; 8]), table]))]);
        encode(&BmValue::Dict(vec![(b("cmd"), cmd)])).unwrap()
    }

    #[test]
    fn autofill_get_set_and_clear_all() {
        let (s, _) = open_user(&autofill_user_bytes());
        let v = s.call("autofill_get", &Args::new()).unwrap();
        assert_eq!(v[0]["widget"], "/a/box");
        assert_eq!(v[0]["entries"], json!(["Jita", "Amarr"]));
        let v = s.call("autofill_set", &args(json!({ "widget": "/a/box", "entries": ["Dodixie"] }))).unwrap();
        assert_eq!(v[0]["entries"], json!(["Dodixie"]));
        let v = s.call("autofill_clear_all", &Args::new()).unwrap();
        assert!(v.as_array().unwrap().iter().all(|l| l["entries"].as_array().unwrap().is_empty()));
        assert_eq!(s.call("status", &Args::new()).unwrap()["user"]["dirty"], true);
    }

    #[test]
    fn keybinds_get_labels_commands_and_renders_combos() {
        let (s, _) = open_user(&keybinds_user_bytes());
        let v = s.call("keybinds_get", &Args::new()).unwrap();
        assert_eq!(v["available"], true);
        let e = v["entries"].as_array().unwrap();
        let high = e.iter().find(|x| x["command"] == "CmdActivateHighPowerSlot1").unwrap();
        assert_eq!(high["label"], "Activate High Power Slot 1");
        assert_eq!(high["group"], "Modules");
        assert_eq!(high["combo"], "Q");
        assert_eq!(high["keys"], json!([81]));
        let med = e.iter().find(|x| x["command"] == "CmdActivateMediumPowerSlot1").unwrap();
        assert_eq!(med["combo"], "Ctrl+S");
        let ap = e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap();
        assert_eq!(ap["combo"], Value::Null);
        assert_eq!(ap["keys"], Value::Null);
    }

    #[test]
    fn keybind_set_binds_by_name_steals_and_unbinds() {
        let (s, _) = open_user(&keybinds_user_bytes());
        // Ctrl+S is CmdActivateMediumPowerSlot1's; giving it to autopilot steals it.
        let v = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot", "key": "s", "ctrl": true }))).unwrap();
        assert_eq!(v["stolen"], json!(["CmdActivateMediumPowerSlot1"]));
        let e = v["keybinds"]["entries"].as_array().unwrap();
        assert_eq!(e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap()["keys"], json!([17, 83]));
        let v = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot" }))).unwrap();
        let e = v["keybinds"]["entries"].as_array().unwrap();
        assert_eq!(e.iter().find(|x| x["command"] == "CmdToggleAutopilot").unwrap()["keys"], Value::Null);
        let err = s.call("keybind_set", &args(json!({ "command": "CmdToggleAutopilot", "key": "Hyper" }))).unwrap_err();
        assert_eq!(err["code"], "unknown_key");
    }

    #[test]
    fn combo_label_orders_modifiers_and_names_unknown_codes() {
        assert_eq!(combo_label(&[17, 18, 16, 68]), "Ctrl+Alt+Shift+D");
        assert_eq!(combo_label(&[999]), "VK999");
    }
```

Run: `cargo test -p app --lib mcp::autofill mcp::keybind mcp::combo` — expected: compile errors / `unknown_tool`.

- [ ] **Step 2: Implement**

Catalog and helpers after `VK_LABELS_JSON`:

```rust
/// Command → (label, group), the UI's `command-names.json`.
const COMMAND_NAMES_JSON: &str = include_str!("../../src/lib/data/command-names.json");

fn command_names() -> HashMap<String, (String, String)> {
    #[derive(serde::Deserialize)]
    struct Entry { label: String, group: String }
    let raw: HashMap<String, Entry> = serde_json::from_str(COMMAND_NAMES_JSON).expect("command-names.json");
    raw.into_iter().map(|(k, e)| (k, (e.label, e.group))).collect()
}

/// `[17, 81]` → "Ctrl+Q", as `keybinds.ts`'s `keysToLabel` renders it.
fn combo_label(keys: &[i64]) -> String {
    let labels = vk_labels();
    keys.iter()
        .map(|&c| match c {
            settings_model::MOD_CTRL => "Ctrl".to_string(),
            settings_model::MOD_ALT => "Alt".to_string(),
            settings_model::MOD_SHIFT => "Shift".to_string(),
            _ => labels.get(&c).cloned().unwrap_or_else(|| format!("VK{c}")),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn keybinds_view(k: &settings_model::Keybinds) -> Value {
    let names = command_names();
    let entries: Vec<Value> = k.entries.iter().map(|e| {
        let (label, group) = names.get(&e.command).cloned().unwrap_or_else(|| (e.command.clone(), "Other".into()));
        json!({
            "command": e.command, "label": label, "group": group,
            "keys": e.keys, "combo": e.keys.as_deref().map(combo_label), "malformed": e.malformed,
        })
    }).collect();
    json!({ "entries": entries, "available": k.available })
}
```

`MOD_CTRL`/`MOD_ALT`/`MOD_SHIFT` are `pub const i64` (`keybinds.rs:19-21`), re-exported at `settings_model`'s root.

Tool defs (append to `tool_defs()`):

```rust
        ToolDef {
            name: "autofill_get",
            description: "The account's remembered texts — what EVE autocompletes in search boxes, chat, market, and so on — as [{widget, entries}]. widget is EVE's internal box id. Needs the account file open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "autofill_set",
            description: "Replace one widget's remembered entries (an empty list clears that one box). Returns every list. Nothing reaches disk until save.",
            schema: || obj(json!({ "widget": { "type": "string" }, "entries": { "type": "array", "items": { "type": "string" } } }), &["widget", "entries"]),
        },
        ToolDef {
            name: "autofill_clear_all",
            description: "Clear every remembered text on this account — every search box, every chat, every market query. Returns the (now empty) lists. Nothing reaches disk until save.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "keybinds_get",
            description: "The account's key bindings: [{command, label, group, keys, combo, malformed}] where combo reads like \"Ctrl+Q\" and keys are the stored codes; available is false when the account never opened the in-game keybinding screen. Needs the account file open. Key names for keybind_set are the ones you see in combo.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "keybind_set",
            description: "Bind a command to a key: key is a name as keybinds_get shows it (\"Q\", \"F1\", \"Num 5\", \"Page Up\"), with ctrl/alt/shift as needed; omit key to unbind. A combo another command holds is taken from it — stolen lists the losers, so tell the user. Returns keybinds and stolen. Nothing reaches disk until save. Unsure: eve_guide keybinds.",
            schema: || obj(json!({
                "command": { "type": "string" },
                "key": { "type": "string" },
                "ctrl": { "type": "boolean" }, "alt": { "type": "boolean" }, "shift": { "type": "boolean" }
            }), &["command"]),
        },
```

`call` arms:

```rust
            "autofill_get" => ok(ops::autofill_lists(&self.state).map_err(fail)?),
            "autofill_set" => ok(ops::set_autofill_list(&self.state, &req::<String>(args, "widget")?, req(args, "entries")?).map_err(fail)?),
            "autofill_clear_all" => ok(ops::clear_all_autofill(&self.state).map_err(fail)?),
            "keybinds_get" => Ok(keybinds_view(&ops::keybinds(&self.state).map_err(fail)?)),
            "keybind_set" => self.keybind_set(args),
```

The method:

```rust
impl EveMcp {
    fn keybind_set(&self, args: &Args) -> ToolResult {
        let command: String = req(args, "command")?;
        let keys = match opt::<String>(args, "key")? {
            None => None,
            Some(name) => {
                let code = vk_code(&name).ok_or_else(|| {
                    err("unknown_key", format!("no key named `{name}`; names are the ones keybinds_get shows, e.g. Q, F1, Num 5, Page Up"))
                })?;
                let mut keys = Vec::new();
                if opt::<bool>(args, "ctrl")?.unwrap_or(false) { keys.push(settings_model::MOD_CTRL); }
                if opt::<bool>(args, "alt")?.unwrap_or(false) { keys.push(settings_model::MOD_ALT); }
                if opt::<bool>(args, "shift")?.unwrap_or(false) { keys.push(settings_model::MOD_SHIFT); }
                keys.push(code);
                Some(keys)
            }
        };
        let r = ops::set_keybind_cmd(&self.state, &command, keys).map_err(fail)?;
        Ok(json!({ "keybinds": keybinds_view(&r.keybinds), "stolen": r.stolen }))
    }
}
```

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp::` green (46 tests); clippy clean.

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: autofill and keybind tools

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: HUD and chat (4 tools)

Spec §2.5, §2.7. HUD is character-side (account optional); chat splits are account-side.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `fn hud_entry_view(e: &settings_model::HudEntry) -> Value`, `fn open_char(bytes) -> (EveMcp, PathBuf)` (test helper: writes `core_char_5.dat` beside a temp dir and opens the CHAR slot), tools `hud_get`, `hud_set`, `chat_get`, `chat_set_splits`.

- [ ] **Step 1: Failing tests**

```rust
    /// A character file with empty `windows` and `ui` sections — enough for
    /// the HUD projection to report every field absent and mint on write.
    fn hud_char_bytes() -> Vec<u8> {
        encode(&BmValue::Dict(vec![(b("windows"), BmValue::Dict(vec![])), (b("ui"), BmValue::Dict(vec![]))])).unwrap()
    }

    /// Like `open_user`, for the character slot. `temp_file` names every file
    /// core_user_5.dat; the name does not matter to `open_file`.
    fn open_char(bytes: &[u8]) -> (EveMcp, PathBuf) {
        let path = temp_file("mcp-char", bytes);
        let s = EveMcp::for_tests();
        ops::open_file(&s.state, Slot::Char, path.to_str().unwrap()).unwrap();
        (s, path)
    }

    fn assert_no_paths(v: &Value, tool: &str) {
        match v {
            Value::Object(m) => {
                for (k, c) in m {
                    assert!(!k.ends_with("_path") && k != "set" && k != "path" || tool.starts_with("copy") || tool == "save" || tool == "open", "{tool}: key `{k}` leaks a path");
                    assert_no_paths(c, tool);
                }
            }
            Value::Array(a) => a.iter().for_each(|c| assert_no_paths(c, tool)),
            _ => {}
        }
    }

    #[test]
    fn hud_get_strips_paths_and_hud_set_mints_a_field() {
        let (s, _) = open_char(&hud_char_bytes());
        let v = s.call("hud_get", &Args::new()).unwrap();
        assert_no_paths(&v, "hud_get");
        let ship = v["entries"].as_array().unwrap().iter().find(|e| e["name"] == "ship_offset").unwrap();
        assert_eq!(ship["value"], Value::Null);
        assert_eq!(ship["scope"], "char");
        let v = s.call("hud_set", &args(json!({ "name": "ship_offset", "value": "-77" }))).unwrap();
        let ship = v["entries"].as_array().unwrap().iter().find(|e| e["name"] == "ship_offset").unwrap();
        assert_eq!(ship["value"], "-77");
        assert!(s.call("hud_set", &args(json!({ "name": "no_such_field", "value": "1" }))).is_err());
    }

    #[test]
    fn hud_get_without_a_character_file_is_no_document() {
        let s = EveMcp::for_tests();
        assert_eq!(s.call("hud_get", &Args::new()).unwrap_err()["code"], "no_document");
    }

    #[test]
    fn chat_get_is_empty_on_a_bare_account_and_set_splits_mints_both_keys() {
        let (s, _) = open_user(&empty_ui_bytes());
        assert_eq!(s.call("chat_get", &Args::new()).unwrap(), json!([]));
        let v = s.call("chat_set_splits", &args(json!({ "window_ids": ["chatchannel_local"], "userlist_width": 135, "input_height": 64 }))).unwrap();
        let p = v.as_array().unwrap().iter().find(|p| p["window_id"] == "chatchannel_local").unwrap();
        assert_eq!(p["userlist_width"], 135);
        assert_eq!(p["input_height"], 64);
        assert_eq!(s.call("chat_set_splits", &args(json!({ "window_ids": ["chatchannel_local"] }))).unwrap_err()["code"], "missing_field");
    }
```

`HudScope` serialises as `char`/`account` (`#[serde(rename_all = "snake_case")]` on the enum — confirm in `hud.rs:23`).

Run: `cargo test -p app --lib mcp::hud mcp::chat` — expected: `unknown_tool` failures.

- [ ] **Step 2: Implement**

View helper:

```rust
/// A HUD-style entry without its `set` target (paths stay server-side).
fn hud_entry_view(e: &settings_model::HudEntry) -> Value {
    json!({ "name": e.name, "kind": e.kind, "value": e.value, "default": e.default, "scope": e.scope })
}
```

Tool defs:

```rust
        ToolDef {
            name: "hud_get",
            description: "The character's HUD furniture settings — ship HUD offset and scale, target row position, effect icons and the like — as {entries: [{name, kind, value, default, scope}]}: kind is float/int/bool, value null means EVE's default applies, scope says which file holds it (char or account). Needs the character file open; the account file adds its own entries.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "hud_set",
            description: "Set one HUD entry by name to a value written as text (\"-77\", \"0.5\", \"true\"), matching its kind. Returns hud_get's shape. Nothing reaches disk until save.",
            schema: || obj(json!({ "name": { "type": "string" }, "value": { "type": "string" } }), &["name", "value"]),
        },
        ToolDef {
            name: "chat_get",
            description: "Per chat channel, the member-list width and input-box height the player set, as [{window_id, userlist_width, input_height}] — window_id is the layout id (chatchannel_local, …); a null means never resized. Account-side: needs the account file open (empty list otherwise).",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "chat_set_splits",
            description: "Set the member-list width and/or input-box height (pixels) for one or more chat channels by window_id; at least one of the two. Returns chat_get's shape. Nothing reaches disk until save.",
            schema: || obj(json!({
                "window_ids": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                "userlist_width": { "type": "integer" }, "input_height": { "type": "integer" }
            }), &["window_ids"]),
        },
```

`call` arms:

```rust
            "hud_get" => {
                let h = ops::hud_layout(&self.state).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "hud_set" => {
                let h = ops::set_hud_field(&self.state, &req::<String>(args, "name")?, &req::<String>(args, "value")?).map_err(fail)?;
                Ok(json!({ "entries": h.entries.iter().map(hud_entry_view).collect::<Vec<_>>() }))
            }
            "chat_get" => ok(ops::chat_panels(&self.state).map_err(fail)?),
            "chat_set_splits" => {
                let ids: Vec<String> = req(args, "window_ids")?;
                let userlist: Option<i64> = opt(args, "userlist_width")?;
                let input: Option<i64> = opt(args, "input_height")?;
                if userlist.is_none() && input.is_none() {
                    return Err(err("missing_field", "give userlist_width and/or input_height"));
                }
                ok(ops::set_chat_splits(&self.state, ids, userlist, input).map_err(fail)?)
            }
```

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp::` green; clippy clean.

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: HUD and chat-split tools

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Neocom (2 tools)

Spec §2.4. Character-side, with the button catalog inlined.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `open_char`, `batch(args, apply, finish)`.
- Produces: `const NEOCOM_JSON`, `struct NeocomEntry { id, btn_type, icon_path }`, `fn neocom_available(bar: &NeocomBar) -> Vec<NeocomEntry>`, `fn neocom_view(bar) -> Value`, `fn neocom_op(state, a)`, tools `neocom_get`, `neocom_edit`.

- [ ] **Step 1: Failing tests**

```rust
    /// `ops::tests::neocom_char_bytes`'s shape: two buttons on the bar, one in
    /// the Original snapshot.
    fn neocom_char_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let button = |id: &str, btn_type: i64, icon: &str| BmValue::Instance {
            class: Box::new(b("utillib.KeyVal")),
            state: Box::new(BmValue::Dict(vec![
                (b("btnType"), BmValue::Int(btn_type)), (b("children"), BmValue::None),
                (b("iconPath"), b(icon)), (b("id"), b(id)),
            ])),
        };
        encode(&BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![
            (b("neocomButtonRawData"), BmValue::Tuple(vec![ts(), BmValue::List(vec![button("chat", 10, "chat.png"), button("wallet", 1, "wallet.png")])])),
            (b("neocomButtonRawDataOriginal"), BmValue::Tuple(vec![ts(), BmValue::Tuple(vec![button("chat", 10, "chat.png")])])),
        ]))])).unwrap()
    }

    #[test]
    fn neocom_get_lists_the_bar_and_the_union_catalog() {
        let (s, _) = open_char(&neocom_char_bytes());
        let v = s.call("neocom_get", &Args::new()).unwrap();
        let ids: Vec<&str> = v["buttons"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["chat", "wallet"]);
        let avail = v["available"].as_array().unwrap();
        assert!(avail.iter().any(|e| e["id"] == "market"), "bundled catalog present");
        assert!(avail.iter().any(|e| e["id"] == "chat" && e["icon_path"] == "chat.png"), "the file's Original entry present");
        assert!(avail.len() > 50);
    }

    #[test]
    fn neocom_edit_adds_by_id_from_the_catalog_reorders_and_resets() {
        let (s, _) = open_char(&neocom_char_bytes());
        let v = s.call("neocom_edit", &args(json!({ "ops": [
            { "op": "add", "id": "market" },
            { "op": "reorder", "order": [2, 0, 1] }
        ]}))).unwrap();
        let ids: Vec<&str> = v["buttons"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["market", "chat", "wallet"]);
        let e = s.call("neocom_edit", &args(json!({ "ops": [{ "op": "add", "id": "no_such_button" }] }))).unwrap_err();
        assert_eq!(e["code"], "unknown_button");
        let v = s.call("neocom_edit", &args(json!({ "ops": [{ "op": "reset" }] }))).unwrap();
        assert_eq!(v["buttons"].as_array().unwrap().len(), 1);
    }
```

Run: `cargo test -p app --lib mcp::neocom` — expected: `unknown_tool`.

- [ ] **Step 2: Implement**

```rust
/// The bundled neocom button catalog (`{id, btnType, iconPath}`), the base
/// the UI unions with a file's stale `Original` snapshot.
const NEOCOM_JSON: &str = include_str!("../../src/lib/data/neocom-buttons.json");

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
struct NeocomEntry { id: String, btn_type: i64, icon_path: String }

/// Bundled ∪ the file's Original, by id; the bundled entry wins a conflict.
fn neocom_available(bar: &settings_model::NeocomBar) -> Vec<NeocomEntry> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw { id: String, btn_type: i64, icon_path: String }
    let bundled: Vec<Raw> = serde_json::from_str(NEOCOM_JSON).expect("neocom-buttons.json");
    let mut out: Vec<NeocomEntry> = bundled.into_iter().map(|r| NeocomEntry { id: r.id, btn_type: r.btn_type, icon_path: r.icon_path }).collect();
    for o in &bar.original {
        if !out.iter().any(|e| e.id == o.id) {
            out.push(NeocomEntry { id: o.id.clone(), btn_type: o.btn_type, icon_path: o.icon_path.clone() });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn neocom_view(bar: &settings_model::NeocomBar) -> Value {
    json!({ "buttons": bar.buttons, "available": neocom_available(bar) })
}

fn neocom_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "reorder" => ops::neocom_reorder(state, req(a, "order")?),
        "remove" => ops::neocom_remove(state, req(a, "index")?),
        "add" => {
            let id: String = req(a, "id")?;
            let bar = ops::neocom_bar(state).map_err(fail)?;
            let e = neocom_available(&bar).into_iter().find(|e| e.id == id)
                .ok_or_else(|| err("unknown_button", format!("no button `{id}`; ids are neocom_get's available list")))?;
            ops::neocom_add(state, &e.id, e.btn_type, &e.icon_path)
        }
        "reset" => ops::neocom_reset(state),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}
```

`NeocomBar.buttons` serialises with `btn_type`/`icon_path` field names as in `neocom.rs:48-56` (plain `Serialize`, no rename) — the test's `x["id"]`/`icon_path` rely on it.

Tool defs:

```rust
        ToolDef {
            name: "neocom_get",
            description: "The character's Neocom bar (the vertical button strip): buttons in order [{index, id, btn_type, icon_path, children}] and available — every button that can be added, by id. Needs the character file open.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "neocom_edit",
            description: "Edit the Neocom bar as a batch (one undo step; first failure rolls back). Ops: reorder {order: [every current index, in the wanted sequence]}; remove {index}; add {id} (an id from neocom_get's available, appended at the end); reset {} (back to EVE's original bar). Returns neocom_get's shape. Nothing reaches disk until save.",
            schema: || obj(op_item(&["reorder", "remove", "add", "reset"], json!({
                "order": { "type": "array", "items": { "type": "integer" } },
                "index": { "type": "integer" }, "id": { "type": "string" }
            })), &["ops"]),
        },
```

`call` arms:

```rust
            "neocom_get" => Ok(neocom_view(&ops::neocom_bar(&self.state).map_err(fail)?)),
            "neocom_edit" => self.batch(args, neocom_op, |s| Ok(neocom_view(&ops::neocom_bar(&s.state).map_err(fail)?))),
```

- [ ] **Step 3: Run, clippy, commit**

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: neocom tools with the button catalog inlined

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Fleet and `lookup_character` (3 tools)

Spec §2.6. Both files; the lookup goes off-runtime.

**Files:**
- Modify: `app/src-tauri/src/names.rs` (`lookup_with`, `EsiName` → `pub(crate)`)
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `fn fleet_view(f: &settings_model::Fleet, names: &names::Cache) -> Value`, `fn fleet_op(state, a)`, `fn lookup_result(r: Result<Option<names::Found>, names::FetchError>) -> ToolResult`, tools `fleet_get`, `fleet_edit`, `lookup_character`.

- [ ] **Step 1: Failing tests**

```rust
    #[test]
    fn fleet_get_projects_both_sides_and_strips_paths() {
        let (s, _) = open_user(&empty_ui_bytes());
        let cpath = temp_file("mcp-fleet-char", &empty_ui_bytes());
        ops::open_file(&s.state, Slot::Char, cpath.to_str().unwrap()).unwrap();
        let v = s.call("fleet_get", &Args::new()).unwrap();
        assert_no_paths(&v, "fleet_get");
        assert_eq!(v["user_open"], true);
        assert_eq!(v["char_open"], true);
        assert!(v["fields"].as_array().unwrap().iter().any(|f| f["name"] == "listen_show_own"));
        assert!(v["colours"].as_array().unwrap().iter().any(|c| c["broadcast"] == "Target"));
        assert!(v["palette"].as_array().unwrap().len() >= 8);
    }

    #[test]
    fn fleet_edit_sets_a_field_a_colour_and_a_watchlist_entry_then_clears_the_colour() {
        let (s, _) = open_user(&empty_ui_bytes());
        let cpath = temp_file("mcp-fleet-char2", &empty_ui_bytes());
        ops::open_file(&s.state, Slot::Char, cpath.to_str().unwrap()).unwrap();
        let v = s.call("fleet_edit", &args(json!({ "ops": [
            { "op": "set_field", "name": "listen_show_own", "value": "1" },
            { "op": "set_colour", "broadcast": "Target", "rgb": [0.2, 0.5, 1.0] },
            { "op": "set_watchlist_colour", "char_id": 90000001, "rgb": [1.0, 0.0, 0.0] }
        ]}))).unwrap();
        let f = v["fields"].as_array().unwrap().iter().find(|f| f["name"] == "listen_show_own").unwrap();
        assert_eq!(f["value"], "1");
        let c = v["colours"].as_array().unwrap().iter().find(|c| c["broadcast"] == "Target").unwrap();
        assert_eq!(c["state"], "set");
        assert_eq!(c["rgb"], json!([0.2, 0.5, 1.0]));
        let w = v["watchlist"].as_array().unwrap().iter().find(|w| w["char_id"] == 90000001).unwrap();
        assert_eq!(w["rgb"], json!([1.0, 0.0, 0.0]));
        let v = s.call("fleet_edit", &args(json!({ "ops": [{ "op": "set_colour", "broadcast": "Target" }] }))).unwrap();
        let c = v["colours"].as_array().unwrap().iter().find(|c| c["broadcast"] == "Target").unwrap();
        assert_eq!(c["state"], "cleared");
        assert_eq!(undo::undo_state(&s.state).depth, 2, "two batches, two undo steps");
    }

    #[test]
    fn lookup_result_maps_a_hit_a_miss_and_a_transport_error() {
        let dir = std::env::temp_dir().join(format!("mcp-lookup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hit = names::lookup_with(&dir, "Some Pilot", |_| Ok(vec![]), |_| Ok(Some(names::Found { id: 42, name: "Some Pilot".into() })));
        assert_eq!(lookup_result(hit).unwrap(), json!({ "id": 42, "name": "Some Pilot" }));
        // Now cached: the fetchers must not be consulted.
        let cached = names::lookup_with(&dir, "some pilot", |_| panic!("cache miss"), |_| panic!("cache miss"));
        assert_eq!(lookup_result(cached).unwrap()["id"], 42);
        let miss = names::lookup_with(&dir, "Nobody", |_| Ok(vec![]), |_| Ok(None));
        assert_eq!(lookup_result(miss).unwrap_err()["code"], "not_found");
        let down = names::lookup_with(&dir, "Anyone", |_| Ok(vec![]), |_| Err(names::FetchError("timeout".into())));
        assert_eq!(lookup_result(down).unwrap_err()["code"], "esi");
    }
```

Run: `cargo test -p app --lib mcp::fleet mcp::lookup` — expected: compile errors (`lookup_with` private, `lookup_result` undefined).

- [ ] **Step 2: Implement**

In `names.rs`: change `fn lookup_with<N, I>(` to `pub(crate) fn lookup_with<N, I>(` and `struct EsiName {` to `pub(crate) struct EsiName {` (fields stay private; callers only ever produce an empty `Vec`). `Found` and `FetchError` are already `pub`.

In `mcp.rs`:

```rust
/// The fleet projection without `set` targets, watch-list ids named from the
/// cache when known, colours as {state, rgb?, default?}.
fn fleet_view(f: &settings_model::Fleet, names: &names::Cache) -> Value {
    let colours: Vec<Value> = f.colours.iter().map(|c| {
        let (state, rgb) = match &c.state {
            settings_model::Colour::Absent => ("absent", None),
            settings_model::Colour::Cleared => ("cleared", None),
            settings_model::Colour::Set { rgb } => ("set", Some(*rgb)),
            _ => ("unknown", None),
        };
        json!({ "broadcast": c.broadcast, "state": state, "rgb": rgb, "default": c.default })
    }).collect();
    let watchlist: Vec<Value> = f.watchlist.iter().map(|w| json!({
        "char_id": w.char_id, "name": names.get(&w.char_id).map(|n| n.name.clone()), "rgb": w.rgb,
    })).collect();
    json!({
        "fields": f.fields.iter().map(hud_entry_view).collect::<Vec<_>>(),
        "colours": colours, "watchlist": watchlist,
        "palette": f.palette.iter().map(|(n, rgb)| json!({ "name": n, "rgb": rgb })).collect::<Vec<_>>(),
        "char_open": f.char_open, "user_open": f.user_open,
    })
}

fn fleet_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_field" => ops::set_fleet_field(state, &req::<String>(a, "name")?, &req::<String>(a, "value")?),
        "set_colour" => ops::set_fleet_colour(state, &req::<String>(a, "broadcast")?, opt(a, "rgb")?),
        "set_watchlist_colour" => ops::set_watchlist_colour(state, req(a, "char_id")?, opt(a, "rgb")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}

fn lookup_result(r: Result<Option<names::Found>, names::FetchError>) -> ToolResult {
    match r {
        Ok(Some(f)) => Ok(json!({ "id": f.id, "name": f.name })),
        Ok(None) => Err(err("not_found", "no character by that name or id")),
        Err(e) => Err(err("esi", format!("ESI lookup failed: {}", e.0))),
    }
}

impl EveMcp {
    fn fleet_get(&self) -> ToolResult {
        let f = ops::fleet_settings(&self.state).map_err(fail)?;
        // Names from the cache only — no network on a read.
        let names = names::resolve_blocking(&self.dir, &[], false);
        Ok(fleet_view(&f, &names))
    }
}
```

`Colour` has a fourth variant this module "refuses to touch" (`fleet.rs:108-120`); the `_ =>` arm covers it. `names::resolve_blocking(dir, &[], false)` with no ids only loads the cache — confirm by reading `resolve_with` (it returns after `load_cache` when `needed` is empty); if it would still build a client, replace with a `names::load_cache`-style accessor made `pub(crate)`.

Tool defs:

```rust
        ToolDef {
            name: "fleet_get",
            description: "The fleet settings across both files: fields (broadcast toggles, formation, fleet finder — {name, kind, value, default, scope}), colours per broadcast type ({broadcast, state: absent|cleared|set, rgb, default}), the watch list ({char_id, name, rgb}) and EVE's nine-colour palette. Needs at least one file open; char_open/user_open say which sides are present.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "fleet_edit",
            description: "Edit fleet settings as a batch (one undo step; first failure rolls back). Ops: set_field {name, value} (value as text, matching the field's kind); set_colour {broadcast, rgb?} (rgb [r,g,b] 0–1 from the palette; omit rgb to clear to EVE's ✕); set_watchlist_colour {char_id, rgb?} (omit rgb to remove the colour; find ids with lookup_character). Returns fleet_get's shape. Nothing reaches disk until save.",
            schema: || obj(op_item(&["set_field", "set_colour", "set_watchlist_colour"], json!({
                "name": { "type": "string" }, "value": { "type": "string" },
                "broadcast": { "type": "string" },
                "rgb": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
                "char_id": { "type": "integer" }
            })), &["ops"]),
        },
        ToolDef {
            name: "lookup_character",
            description: "Find a character's id by name, or confirm an id, through EVE's ESI (cached afterwards). Returns {id, name}; not_found when ESI knows no such character. Use it for watch-list colours.",
            schema: || obj(json!({ "query": { "type": "string" } }), &["query"]),
        },
```

`call` arms:

```rust
            "fleet_get" => self.fleet_get(),
            "fleet_edit" => self.batch(args, fleet_op, |s| s.fleet_get()),
            "lookup_character" => {
                let q: String = req(args, "query")?;
                lookup_result(off_runtime(|| names::lookup_blocking(&self.dir, &q)))
            }
```

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp:: names::` green; clippy clean.

```bash
git add app/src-tauri/src/names.rs app/src-tauri/src/mcp.rs
git commit -m "MCP: fleet tools and lookup_character

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Layout — `layout_get` and `layout_edit`

Spec §2.1 (all but rendering). The one editor whose writes are built from projection paths.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Produces: `fn layout_view(wl: &settings_model::WindowLayout) -> Value`, `fn geometry_mutations(wl, window, x, y, w, h) -> Result<Vec<Mutation>, Value>`, `fn flag_mutation(wl, window, flag, on) -> Result<Mutation, Value>`, `fn layout_op(state, a)`, tools `layout_get`, `layout_edit`. Test fixture `fn layout_char_bytes() -> Vec<u8>`.

- [ ] **Step 1: Failing tests**

```rust
    /// A character file with three windows — `overview` open at a matching
    /// resolution, `market` open at a different one (so a move re-stamps the
    /// screen size), `fitting` closed — and a stack of m1+m2 in container C
    /// (`ops::tests::stacked_char_bytes`'s shape). Flags: `pinnedWindows` has
    /// an entry for overview only, so market's pinned flag is an Insert.
    fn layout_char_bytes() -> Vec<u8> {
        let ts = || BmValue::Long(vec![0u8; 8]);
        let geom = |x: i64, y: i64, w: i64, h: i64, sw: i64, sh: i64| {
            BmValue::Tuple(vec![BmValue::Int(x), BmValue::Int(y), BmValue::Int(w), BmValue::Int(h), BmValue::Int(sw), BmValue::Int(sh)])
        };
        encode(&BmValue::Dict(vec![(b("windows"), BmValue::Dict(vec![
            (b("windowSizesAndPositions_1"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                (b("overview"), geom(100, 200, 400, 600, 2560, 1440)),
                (b("market"), geom(10, 10, 300, 300, 1920, 1080)),
                (b("fitting"), geom(0, 0, 500, 500, 2560, 1440)),
                (b("m1"), geom(0, 0, 100, 80, 2560, 1440)), (b("m2"), geom(0, 0, 100, 80, 2560, 1440)), (b("C"), geom(0, 0, 100, 80, 2560, 1440)),
            ])])),
            (b("openWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![
                (b("overview"), BmValue::Bool(true)), (b("market"), BmValue::Bool(true)), (b("fitting"), BmValue::Bool(false)),
                (b("m1"), BmValue::Bool(true)), (b("m2"), BmValue::Bool(true)), (b("C"), BmValue::Bool(true)),
            ])])),
            (b("pinnedWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("overview"), BmValue::Bool(true))])])),
            (b("stacksWindows"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("m1"), b("C")), (b("m2"), b("C"))])])),
            (b("preferredIdxInStack3"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![(b("C"), BmValue::Dict(vec![(b("m1"), BmValue::Int(0)), (b("m2"), BmValue::Int(1))]))])])),
        ]))])).unwrap()
    }

    fn window<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["windows"].as_array().unwrap().iter().find(|w| w["id"] == id).unwrap_or_else(|| panic!("window {id}"))
    }

    #[test]
    fn layout_get_strips_every_path_and_reports_geometry_flags_and_stacks() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_get", &Args::new()).unwrap();
        assert_no_paths(&v, "layout_get");
        assert_eq!(v["reference_w"], 2560);
        let ov = window(&v, "overview");
        assert_eq!(ov["geom"], json!({ "x": 100, "y": 200, "w": 400, "h": 600, "screen_w": 2560, "screen_h": 1440 }));
        assert_eq!(ov["open"], true);
        assert_eq!(ov["resolution_matches"], true);
        assert!(ov["flags"].as_array().unwrap().iter().any(|f| f["name"] == "pinnedWindows" && f["value"] == true && f["settable"] == true));
        assert!(ov["flags"].as_array().unwrap().iter().any(|f| f["name"] == "lockedWindows" && f["settable"] == false), "no lockedWindows dict in the file → unavailable");
        assert_eq!(window(&v, "market")["resolution_matches"], false);
        assert_eq!(window(&v, "fitting")["open"], false);
        assert_eq!(v["stacks"][0]["members"], json!(["m1", "m2"]));
    }

    #[test]
    fn layout_edit_moves_a_window_and_restamps_a_mismatched_resolution() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_geometry", "window": "overview", "x": 0, "y": 0 },
            { "op": "set_geometry", "window": "market", "w": 640 }
        ]}))).unwrap();
        assert_eq!(window(&v, "overview")["geom"]["x"], 0);
        assert_eq!(window(&v, "overview")["geom"]["h"], 600, "untouched axis kept");
        let m = window(&v, "market");
        assert_eq!(m["geom"]["w"], 640);
        assert_eq!(m["geom"]["screen_w"], 2560, "re-stamped to the reference");
        assert_eq!(m["resolution_matches"], true);
        assert_eq!(undo::undo_state(&s.state).depth, 1);
    }

    #[test]
    fn layout_edit_sets_flags_including_an_insert_and_refuses_the_unknown() {
        let (s, _) = open_char(&layout_char_bytes());
        let before = s.call("layout_get", &Args::new()).unwrap();
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_flag", "window": "market", "flag": "pinnedWindows", "on": true },
            { "op": "set_flag", "window": "overview", "flag": "openWindows", "on": false }
        ]}))).unwrap();
        assert!(window(&v, "market")["flags"].as_array().unwrap().iter().any(|f| f["name"] == "pinnedWindows" && f["value"] == true), "an Insert target minted the key");
        assert_eq!(window(&v, "overview")["open"], false);
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_flag", "window": "market", "flag": "lockedWindows", "on": true }] }))).unwrap_err();
        assert_eq!(e["code"], "flag_unavailable");
        let e = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "set_geometry", "window": "market", "x": 5 },
            { "op": "set_flag", "window": "market", "flag": "no_such_flag", "on": true }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "unknown_flag");
        assert_eq!(e["op_index"], 1);
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_geometry", "window": "nope", "x": 1 }] }))).unwrap_err();
        assert_eq!(e["code"], "unknown_window");
        let e = s.call("layout_edit", &args(json!({ "ops": [{ "op": "set_geometry", "window": "overview" }] }))).unwrap_err();
        assert_eq!(e["code"], "missing_field");
        // The two failed batches left nothing behind (the first batch's edits stand).
        let after = s.call("layout_get", &Args::new()).unwrap();
        assert_eq!(window(&after, "market")["geom"]["x"], window(&before, "market")["geom"]["x"]);
    }

    #[test]
    fn layout_edit_stack_ops_round_trip() {
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_edit", &args(json!({ "ops": [
            { "op": "stack_unstack", "window": "m1" },
            { "op": "stack_add", "window": "m1", "container": "C" },
            { "op": "stack_reorder", "container": "C", "members": ["m2", "m1"] }
        ]}))).unwrap();
        assert_eq!(v["stacks"][0]["members"], json!(["m2", "m1"]));
        assert_eq!(undo::undo_state(&s.state).depth, 1, "stack ops open their own group; the batch is still one step");
    }
```

Flag names are the file's own keys, from `windows.rs`'s `BOOL_FLAGS`: `openWindows`, `collapsedWindows`, `minimizedWindows`, `lockedWindows`, `compactWindows`, `isOverlayedWindows`, `isLightBackgroundWindows`, `pinnedWindows`. A flag whose dict the file lacks projects as `SetTarget::Unavailable` (the fixture has no `lockedWindows` dict); `openWindows` doubles as the window's `open` field.

Run: `cargo test -p app --lib mcp::layout` — expected: `unknown_tool`.

- [ ] **Step 2: Implement**

```rust
use settings_model::{Mutation, NewValue, SetTarget, WindowLayout, WindowRect};

/// The layout without a single path: what the model sees.
fn layout_view(wl: &WindowLayout) -> Value {
    let windows: Vec<Value> = wl.windows.iter().map(|w| json!({
        "id": w.id, "label": w.label, "name": w.name, "open": w.open, "renderable": w.renderable,
        "resolution_matches": w.resolution_matches,
        "geom": w.geom.as_ref().map(|g| json!({ "x": g.x, "y": g.y, "w": g.w, "h": g.h, "screen_w": g.screen_w, "screen_h": g.screen_h })),
        "flags": w.flags.iter().map(|f| json!({ "name": f.name, "value": f.value, "settable": !matches!(f.set, SetTarget::Unavailable) })).collect::<Vec<_>>(),
        "stack": w.stack.as_ref().map(|s| json!({ "container_id": s.container_id, "role": s.role })),
    })).collect();
    json!({ "reference_w": wl.reference_w, "reference_h": wl.reference_h, "windows": windows, "stacks": wl.stacks })
}

fn find_window<'a>(wl: &'a WindowLayout, id: &str) -> Result<&'a WindowRect, Value> {
    wl.windows.iter().find(|w| w.id == id).ok_or_else(|| err("unknown_window", format!("no window `{id}`; ids are layout_get's")))
}

fn set_int(path: &settings_model::NodePath, v: i64) -> Mutation {
    Mutation::SetScalar { path: path.clone(), text: v.to_string() }
}

/// `LayoutView.svelte`'s `geomMutations`, in Rust: one set_scalar per changed
/// axis, plus the reference screen size when the window's own differs.
fn geometry_mutations(wl: &WindowLayout, id: &str, x: Option<i64>, y: Option<i64>, w: Option<i64>, h: Option<i64>) -> Result<Vec<Mutation>, Value> {
    let win = find_window(wl, id)?;
    let g = win.geom.as_ref().ok_or_else(|| err("no_geometry", format!("window `{id}` has no stored geometry")))?;
    let mut ms = Vec::new();
    for (next, cur, path) in [(x, g.x, &g.x_path), (y, g.y, &g.y_path), (w, g.w, &g.w_path), (h, g.h, &g.h_path)] {
        if let Some(n) = next {
            if n != cur { ms.push(set_int(path, n)); }
        }
    }
    if !ms.is_empty() && !win.resolution_matches {
        ms.push(set_int(&g.screen_w_path, wl.reference_w));
        ms.push(set_int(&g.screen_h_path, wl.reference_h));
    }
    Ok(ms)
}

/// `flagMutation`'s twin.
fn flag_mutation(wl: &WindowLayout, id: &str, flag: &str, on: bool) -> Result<Mutation, Value> {
    let win = find_window(wl, id)?;
    let f = win.flags.iter().find(|f| f.name == flag).ok_or_else(|| {
        let names: Vec<&str> = win.flags.iter().map(|f| f.name.as_str()).collect();
        err("unknown_flag", format!("window `{id}` has no flag `{flag}`; it has {}", names.join(", ")))
    })?;
    match &f.set {
        SetTarget::Set { path } => Ok(Mutation::SetScalar { path: path.clone(), text: on.to_string() }),
        SetTarget::Insert { parent, key } => {
            // NewValue is not Clone; a serde round trip copies it.
            let key: NewValue = serde_json::from_value(serde_json::to_value(key).unwrap()).unwrap();
            Ok(Mutation::InsertDictEntry { parent: parent.clone(), key, value: NewValue::Bool(on) })
        }
        SetTarget::Unavailable => Err(err("flag_unavailable", format!("`{flag}` cannot be set on `{id}` in this file"))),
    }
}

fn layout_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "set_geometry" => {
            let id: String = req(a, "window")?;
            let (x, y, w, h) = (opt(a, "x")?, opt(a, "y")?, opt(a, "w")?, opt(a, "h")?);
            if x.is_none() && y.is_none() && w.is_none() && h.is_none() {
                return Err(err("missing_field", "set_geometry needs at least one of x, y, w, h"));
            }
            let wl = ops::window_layout(state, Slot::Char).map_err(fail)?;
            let ms = geometry_mutations(&wl, &id, x, y, w, h)?;
            if ms.is_empty() { return Ok(()); }
            ops::apply_mutations(state, Slot::Char, &ms).map(drop).map_err(fail)
        }
        "set_flag" => {
            let wl = ops::window_layout(state, Slot::Char).map_err(fail)?;
            let m = flag_mutation(&wl, &req::<String>(a, "window")?, &req::<String>(a, "flag")?, req(a, "on")?)?;
            ops::apply_mutations(state, Slot::Char, &[m]).map(drop).map_err(fail)
        }
        "stack_create" => ops::stack_create(state, &req::<String>(a, "a")?, &req::<String>(a, "b")?).map(drop).map_err(fail),
        "stack_add" => ops::stack_add(state, &req::<String>(a, "window")?, &req::<String>(a, "container")?).map(drop).map_err(fail),
        "stack_unstack" => ops::stack_unstack(state, &req::<String>(a, "window")?).map(drop).map_err(fail),
        "stack_reorder" => ops::stack_reorder(state, &req::<String>(a, "container")?, req(a, "members")?).map(drop).map_err(fail),
        "stack_delete_orphans" => ops::stack_delete_orphans(state).map(drop).map_err(fail),
        _ => Err(unknown_op(&op)),
    }
}
```

`Mutation`'s variants and field names: `SetScalar { path, text }`, `InsertDictEntry { parent, key, value }` (`crates/settings-model/src/mutate.rs:48-53`); `ops::apply_mutations(state, slot, &[Mutation]) -> Result<Node, ErrDto>`. `StackRole` serialises snake_case (`container`/`member`).

Tool defs:

```rust
        ToolDef {
            name: "layout_get",
            description: "The character's window layout: reference_w/h (the screen size the file was saved at), windows [{id, label, name, open, renderable, resolution_matches, geom: {x, y, w, h, screen_w, screen_h}, flags: [{name, value, settable}], stack}] in pixels, and stacks [{container_id, container_label, anchor_id, members}] — tabbed groups drawn at their anchor window. Needs the character file open. To SEE it, call layout_render.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "layout_edit",
            description: "Edit the layout as a batch (one undo step; first failure rolls back). Ops: set_geometry {window, x?, y?, w?, h?} (pixels at reference_w/h; unmentioned axes keep their value; a window saved at another resolution is re-stamped to the reference); set_flag {window, flag, on} (flag is one of layout_get's flag names — openWindows, pinnedWindows, lockedWindows, compactWindows, … — and only where settable is true); stack_create {a, b}; stack_add {window, container}; stack_unstack {window}; stack_reorder {container, members: [every member id in tab order]}; stack_delete_orphans {}. Returns layout_get's shape. Nothing reaches disk until save. Unsure: eve_guide layout.",
            schema: || obj(op_item(&["set_geometry", "set_flag", "stack_create", "stack_add", "stack_unstack", "stack_reorder", "stack_delete_orphans"], json!({
                "window": { "type": "string" },
                "x": { "type": "integer" }, "y": { "type": "integer" }, "w": { "type": "integer" }, "h": { "type": "integer" },
                "flag": { "type": "string" }, "on": { "type": "boolean" },
                "a": { "type": "string" }, "b": { "type": "string" }, "container": { "type": "string" },
                "members": { "type": "array", "items": { "type": "string" } }
            })), &["ops"]),
        },
```

`call` arms:

```rust
            "layout_get" => Ok(layout_view(&ops::window_layout(&self.state, Slot::Char).map_err(fail)?)),
            "layout_edit" => self.batch(args, layout_op, |s| Ok(layout_view(&ops::window_layout(&s.state, Slot::Char).map_err(fail)?))),
```

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp::` green; clippy clean.

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: layout_get and layout_edit — the canvas's own mutations, built server-side

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: `layout_render` — `mcp_render.rs`

Spec §2.1 (rendering). Delivers the picture.

**Files:**
- Create: `app/src-tauri/src/mcp_render.rs`
- Modify: `app/src-tauri/src/lib.rs` (`mod mcp_render;`)
- Modify: `app/src-tauri/src/mcp.rs` (the tool)

**Interfaces:**
- Produces: `pub(crate) struct Legend { width, height, reference_w, reference_h, scale: f64, windows: Vec<LegendRow { id, label, x, y, w, h, drawn, stack: Option<String> }> }` (Serialize); `pub(crate) fn layout_png(wl: &WindowLayout, width: u32, include_closed: bool) -> (Vec<u8>, Legend)`; `pub(crate) fn glyph(c: char) -> [u8; 7]` (5-bit rows, MSB left); `pub(crate) const GROUND: [u8; 3]`, `pub(crate) const FILLS: [[u8; 3]; 8]`; tool `layout_render`.

- [ ] **Step 1: Failing tests in `mcp_render.rs`**

Create the file with the module doc and the tests first:

```rust
//! `layout_render`: a PNG of the window layout for an MCP image block, in
//! pure Rust — an RGB buffer, filled boxes with borders, labels from a 5×7
//! bitmap font, `png` for the encoding. Windows and stacks only; no HUD
//! furniture, no neocom. Spec §2.1 of the all-surfaces design.

use serde::Serialize;
use settings_model::WindowLayout;

#[cfg(test)]
mod tests {
    use super::*;
    use settings_model::{window_layout as project_window_layout, WindowLayout};
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    /// overview open at (100,200) 400×600 on 2560×1440; fitting closed; m1+m2
    /// stacked in C at (1000, 100).
    fn layout() -> WindowLayout {
        let ts = || Value::Long(vec![0u8; 8]);
        let geom = |x: i64, y: i64, w: i64, h: i64| Value::Tuple(vec![Value::Int(x), Value::Int(y), Value::Int(w), Value::Int(h), Value::Int(2560), Value::Int(1440)]);
        let doc = Value::Dict(vec![(b("windows"), Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("overview"), geom(100, 200, 400, 600)), (b("fitting"), geom(0, 0, 500, 500)),
                (b("m1"), geom(1000, 100, 300, 200)), (b("m2"), geom(1000, 100, 300, 200)), (b("C"), geom(1000, 100, 300, 200)),
            ])])),
            (b("openWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![
                (b("overview"), Value::Bool(true)), (b("fitting"), Value::Bool(false)),
                (b("m1"), Value::Bool(true)), (b("m2"), Value::Bool(true)), (b("C"), Value::Bool(true)),
            ])])),
            (b("stacksWindows"), Value::Tuple(vec![ts(), Value::Dict(vec![(b("m1"), b("C")), (b("m2"), b("C"))])])),
            (b("preferredIdxInStack3"), Value::Tuple(vec![ts(), Value::Dict(vec![(b("C"), Value::Dict(vec![(b("m1"), Value::Int(0)), (b("m2"), Value::Int(1))]))])])),
        ]))]);
        project_window_layout(&doc, None)
    }

    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let decoder = png::Decoder::new(png);
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        (info.width, info.height, buf[..info.buffer_size()].to_vec())
    }

    fn px(img: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
        let i = ((y * img.0 + x) * 3) as usize;
        [img.2[i], img.2[i + 1], img.2[i + 2]]
    }

    #[test]
    fn renders_at_the_requested_width_and_reference_aspect() {
        let (png, legend) = layout_png(&layout(), 640, false);
        let img = decode(&png);
        assert_eq!((img.0, img.1), (640, 360));
        assert_eq!((legend.width, legend.height), (640, 360));
        assert!((legend.scale - 0.25).abs() < 1e-9);
    }

    #[test]
    fn an_open_window_is_filled_a_closed_one_is_absent_unless_asked_and_a_stack_draws_once() {
        let wl = layout();
        let (png, legend) = layout_png(&wl, 640, false);
        let img = decode(&png);
        // overview's centre at scale 0.25: (100+200, 200+300) -> (75, 125).
        let inside = px(&img, 75, 125);
        assert!(FILLS.contains(&inside), "centre of overview is a fill colour, got {inside:?}");
        // fitting's box would be around (0..125, 0..125); pick a point no other window covers.
        assert_eq!(px(&img, 5, 100), GROUND, "closed window not drawn");
        let rows = &legend.windows;
        assert!(rows.iter().find(|r| r.id == "overview").unwrap().drawn);
        assert!(!rows.iter().find(|r| r.id == "fitting").unwrap().drawn);
        let drawn_stack_members = rows.iter().filter(|r| r.stack.as_deref() == Some("C") && r.drawn).count();
        assert_eq!(drawn_stack_members, 1, "a stack draws once, at its anchor");

        let (png, legend) = layout_png(&wl, 640, true);
        let img = decode(&png);
        assert_ne!(px(&img, 0, 62), GROUND, "closed window drawn as an outline when asked");
        assert!(legend.windows.iter().find(|r| r.id == "fitting").unwrap().drawn);
    }

    #[test]
    fn the_font_covers_its_glyphs_and_dots_the_rest() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_./".chars() {
            assert!(glyph(c).iter().any(|row| *row != 0), "{c} has a pattern");
        }
        assert_eq!(glyph('é'), glyph('.'));
        assert_eq!(glyph('a'), glyph('A'), "labels are uppercased");
    }

    #[test]
    fn width_is_clamped() {
        let (png, _) = layout_png(&layout(), 10, false);
        assert_eq!(decode(&png).0, 320);
        let (png, _) = layout_png(&layout(), 9999, false);
        assert_eq!(decode(&png).0, 2048);
    }
}
```

`settings_model::window_layout(root, user)` is the projection (`windows.rs:113`; `ops.rs` imports it `as project_window_layout`, as the test does). Add `mod mcp_render;` to `lib.rs`.

Run: `cargo test -p app --lib mcp_render::` — expected: compile errors.

- [ ] **Step 2: Implement the renderer**

Above the tests:

```rust
pub(crate) const GROUND: [u8; 3] = [24, 26, 30];
const GRID: [u8; 3] = [40, 43, 48];
const BORDER: [u8; 3] = [230, 232, 235];
const TEXT: [u8; 3] = [20, 20, 20];
const OUTLINE: [u8; 3] = [120, 124, 130];
/// Eight muted fills, cycled per drawn window.
pub(crate) const FILLS: [[u8; 3]; 8] = [
    [120, 170, 210], [200, 150, 110], [140, 190, 140], [200, 170, 120],
    [170, 140, 200], [120, 190, 190], [210, 140, 160], [180, 180, 130],
];
const MIN_W: u32 = 320;
const MAX_W: u32 = 2048;

#[derive(Serialize)]
pub(crate) struct LegendRow {
    pub id: String, pub label: String, pub x: i64, pub y: i64, pub w: i64, pub h: i64,
    pub drawn: bool, pub stack: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct Legend {
    pub width: u32, pub height: u32, pub reference_w: i64, pub reference_h: i64, pub scale: f64,
    pub windows: Vec<LegendRow>,
}

struct Canvas { w: u32, h: u32, px: Vec<u8> }

impl Canvas {
    fn new(w: u32, h: u32) -> Self { Canvas { w, h, px: vec![0; (w * h * 3) as usize] } }
    fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 { return; }
        let i = ((y as u32 * self.w + x as u32) * 3) as usize;
        self.px[i..i + 3].copy_from_slice(&c);
    }
    fn fill(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for yy in y..y + h { for xx in x..x + w { self.set(xx, yy, c); } }
    }
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for xx in x..x + w { self.set(xx, y, c); self.set(xx, y + h - 1, c); }
        for yy in y..y + h { self.set(x, yy, c); self.set(x + w - 1, yy, c); }
    }
    /// Draw `text` in the bitmap font at (x, y), `s` pixels per font pixel,
    /// stopping at `max_w` pixels.
    fn text(&mut self, x: i64, y: i64, text: &str, s: i64, max_w: i64, c: [u8; 3]) {
        let mut cx = x;
        for ch in text.chars() {
            if cx + 5 * s > x + max_w { break; }
            let g = glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0b10000 >> col) != 0 {
                        self.fill(cx + col * s, y + row as i64 * s, s, s, c);
                    }
                }
            }
            cx += 6 * s;
        }
    }
}

/// 5×7 glyphs, one byte per row, bit 4 = leftmost column. Lower case maps
/// to upper; anything outside the set is `.`.
pub(crate) fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        ' ' => [0x00; 7],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
    }
}

/// The picture and its legend. `width` is clamped to 320..=2048; the height
/// follows the file's reference aspect ratio (16:9 when the file has none).
pub(crate) fn layout_png(wl: &WindowLayout, width: u32, include_closed: bool) -> (Vec<u8>, Legend) {
    let width = width.clamp(MIN_W, MAX_W);
    let (rw, rh) = if wl.reference_w > 0 && wl.reference_h > 0 { (wl.reference_w, wl.reference_h) } else { (1920, 1080) };
    let scale = width as f64 / rw as f64;
    let height = ((rh as f64 * scale).round() as u32).max(1);
    let mut c = Canvas::new(width, height);
    c.fill(0, 0, width as i64, height as i64, GROUND);
    for i in 1..10 {
        let gx = (width as i64 * i) / 10;
        let gy = (height as i64 * i) / 10;
        c.fill(gx, 0, 1, height as i64, GRID);
        c.fill(0, gy, width as i64, 1, GRID);
    }

    // A stack draws once, at its anchor, with its members' labels stacked.
    let anchor_of = |id: &str| wl.stacks.iter().find(|s| s.members.iter().any(|m| m == id)).map(|s| (s.container_id.clone(), s.anchor_id.clone(), s.members.clone()));
    let mut rows = Vec::new();
    let mut fill_i = 0usize;
    for w in &wl.windows {
        let Some(g) = &w.geom else {
            rows.push(LegendRow { id: w.id.clone(), label: w.label.clone(), x: 0, y: 0, w: 0, h: 0, drawn: false, stack: None });
            continue;
        };
        let stack = anchor_of(&w.id);
        let is_anchor_or_free = stack.as_ref().map_or(true, |(_, anchor, _)| anchor == &w.id);
        let open = w.open && w.renderable;
        let drawn = is_anchor_or_free && (open || include_closed);
        rows.push(LegendRow { id: w.id.clone(), label: w.label.clone(), x: g.x, y: g.y, w: g.w, h: g.h, drawn, stack: stack.as_ref().map(|(cid, _, _)| cid.clone()) });
        if !drawn { continue; }
        let (x, y, bw, bh) = ((g.x as f64 * scale) as i64, (g.y as f64 * scale) as i64, ((g.w as f64 * scale) as i64).max(2), ((g.h as f64 * scale) as i64).max(2));
        if open {
            let fill = FILLS[fill_i % FILLS.len()];
            fill_i += 1;
            c.fill(x, y, bw, bh, fill);
            c.rect(x, y, bw, bh, BORDER);
        } else {
            c.rect(x, y, bw, bh, OUTLINE);
        }
        // Labels: the window's, or every member's for a stack, top-down.
        let labels: Vec<String> = match &stack {
            Some((_, _, members)) => members.iter().map(|m| wl.windows.iter().find(|x| &x.id == m).map(|x| x.label.clone()).unwrap_or_else(|| m.clone())).collect(),
            None => vec![w.label.clone()],
        };
        let s = if bh >= 40 && bw >= 60 { 2 } else { 1 };
        let mut ty = y + 3;
        for label in labels {
            if ty + 7 * s > y + bh - 2 { break; }
            c.text(x + 3, ty, &label, s, bw - 6, if open { TEXT } else { OUTLINE });
            ty += 8 * s;
        }
        if stack.is_some() {
            // A tab strip along the top edge marks a stack.
            c.fill(x, y, bw, 2, BORDER);
        }
    }

    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(&c.px).expect("png data");
    }
    let legend = Legend { width, height, reference_w: rw, reference_h: rh, scale, windows: rows };
    (png, legend)
}
```

Check the `png` 0.18 encoder API names (`Encoder::new(w: impl Write, width, height)`, `set_color`, `set_depth`, `write_header`, `write_image_data`) against the crate's docs if the compiler objects; the decoder in the tests (`Decoder::new`, `read_info`, `output_buffer_size`, `next_frame`, `ColorType::Rgb`) likewise. In the "closed window drawn as outline" assertion, the pixel `(0, 62)` sits on fitting's left edge (`x = 0`, and `y = 62` is inside `0..125`); if the outline lands one pixel off, use `(0, 60)`.

- [ ] **Step 3: Run the renderer tests**

Run: `cargo test -p app --lib mcp_render::` — expected: 4 passed.

- [ ] **Step 4: The tool**

In `mcp.rs`, tool def:

```rust
        ToolDef {
            name: "layout_render",
            description: "A picture of the character's window layout: every open window as a labelled box on the screen at the file's reference aspect ratio, stacks drawn once at their anchor with a tab strip, plus a legend {width, height, reference_w, reference_h, scale, windows: [{id, label, x, y, w, h, drawn, stack}]}. Windows and stacks only — no HUD, no Neocom. Call it before moving anything. Needs the character file open.",
            schema: || obj(json!({
                "width": { "type": "integer", "minimum": 320, "maximum": 2048, "description": "Image width in pixels, default 1024." },
                "include_closed": { "type": "boolean", "description": "Also outline closed windows. Default false." }
            }), &[]),
        },
```

`call` arm:

```rust
            "layout_render" => {
                use base64::Engine as _;
                let wl = ops::window_layout(&self.state, Slot::Char).map_err(fail)?;
                let width: u32 = opt::<u32>(args, "width")?.unwrap_or(1024);
                let include_closed = opt::<bool>(args, "include_closed")?.unwrap_or(false);
                let (png, legend) = crate::mcp_render::layout_png(&wl, width, include_closed);
                let mut v = serde_json::to_value(legend).map_err(|e| err("serialize", e.to_string()))?;
                v[PNG_KEY] = json!(base64::engine::general_purpose::STANDARD.encode(png));
                Ok(v)
            }
```

Test in `mcp.rs`'s `mod tests`:

```rust
    #[test]
    fn layout_render_returns_a_png_and_a_legend_through_the_tool() {
        use base64::Engine as _;
        let (s, _) = open_char(&layout_char_bytes());
        let v = s.call("layout_render", &args(json!({ "width": 400 }))).unwrap();
        assert_eq!(v["width"], 400);
        assert_eq!(v["windows"].as_array().unwrap().len(), 6);
        let png = base64::engine::general_purpose::STANDARD.decode(v["png_base64"].as_str().unwrap()).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let blocks = blocks(v);
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[1], ContentBlock::Image(_)));
    }
```

- [ ] **Step 5: Run, clippy, commit**

`cargo test -p app --lib mcp:: mcp_render::` green; clippy clean.

```bash
git add app/src-tauri/src/mcp_render.rs app/src-tauri/src/lib.rs app/src-tauri/src/mcp.rs
git commit -m "MCP: layout_render — the layout as a PNG image block with a legend

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Copy settings (3 tools)

Spec §2.8. Cross-file; writes immediately.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `locate`, `discover`, `accounts::load_roster`; `crate::setup::{setup_preview, setup_apply, copy_files, BatchSource, Aspect, SetupPlan, TargetResult}`, `crate::presets::preset_path`.
- Produces: `struct CopyArgs { source: BatchSource, targets: Vec<String>, aspects: Vec<Aspect>, allow_other_folders: bool }`, `fn copy_args(&self, args) -> Result<CopyArgs, Value>`, tools `copy_preview`, `copy_apply`, `copy_files`. Test helper `fn temp_profile() -> (PathBuf /*root*/, PathBuf /*profile dir*/, PathBuf /*app dir*/)`.

- [ ] **Step 1: Failing tests**

```rust
    /// A discovery root with one install/profile holding source char 100 on
    /// account 500 and target char 200 on account 600, paired in accounts.json
    /// under a separate app dir — `setup::tests`' layout.
    fn temp_profile() -> (PathBuf, PathBuf, PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("mcp-copy-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();
        let ts = || BmValue::Long(vec![0u8; 8]);
        let overview = |c: &str| BmValue::Dict(vec![(b("overview"), BmValue::Dict(vec![(b("overviewColumns"), BmValue::List(vec![b(c)]))]))]);
        let widths = || BmValue::Dict(vec![(b("ui"), BmValue::Dict(vec![(b("SortHeadersSizes"), BmValue::Tuple(vec![ts(), BmValue::Dict(vec![])]))]))]);
        std::fs::write(prof.join("core_char_100.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_500.dat"), encode(&overview("SRC")).unwrap()).unwrap();
        std::fs::write(prof.join("core_char_200.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_600.dat"), encode(&overview("TGT")).unwrap()).unwrap();
        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut store = crate::accounts::AccountsStore::default();
        store.accounts.insert(500, crate::accounts::Account { alias: None, characters: vec![100] });
        store.accounts.insert(600, crate::accounts::Account { alias: None, characters: vec![200] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();
        (base.join("root"), prof, app_dir)
    }

    fn copy_server() -> (EveMcp, PathBuf) {
        let (root, prof, app_dir) = temp_profile();
        (EveMcp::new(app_dir, vec![root]), prof)
    }

    #[test]
    fn copy_preview_plans_by_char_id_and_names_the_account_write() {
        let (s, _) = copy_server();
        let v = s.call("copy_preview", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["char_writes"][0]["char_id"], 200);
        assert_eq!(v["account_writes"][0]["user_id"], 600);
        assert_eq!(v["excluded"], json!([]));
        assert_eq!(v["source_error"], Value::Null);
    }

    #[test]
    fn copy_apply_writes_backs_up_and_a_fresh_open_shows_the_copy() {
        let (s, prof) = copy_server();
        let v = s.call("copy_apply", &args(json!({ "source_char_id": 100, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap();
        let results = v.as_array().unwrap();
        assert!(results.iter().all(|r| r["ok"] == true), "{v}");
        let acct = results.iter().find(|r| r["path"].as_str().unwrap().contains("core_user_600")).unwrap();
        assert!(PathBuf::from(acct["backup_path"].as_str().unwrap()).exists());
        s.call("open", &args(json!({ "user_file": prof.join("core_user_600.dat").to_string_lossy() }))).unwrap();
        let ov = s.call("overview_get", &Args::new()).unwrap();
        assert!(ov.to_string().contains("SRC"), "the source's overview landed: {ov}");
    }

    #[test]
    fn copy_args_need_exactly_one_source_and_a_target() {
        let (s, _) = copy_server();
        assert_eq!(s.call("copy_preview", &args(json!({ "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "missing_field");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 100, "source_char_file": "x", "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "bad_arguments");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 100, "aspects": ["overview"] }))).unwrap_err()["code"], "missing_field");
        assert_eq!(s.call("copy_preview", &args(json!({ "source_char_id": 999, "target_char_ids": [200], "aspects": ["overview"] }))).unwrap_err()["code"], "unknown_character");
    }

    #[test]
    fn copy_files_clones_a_file_onto_another_of_the_same_kind() {
        let (s, prof) = copy_server();
        let src = prof.join("core_user_500.dat");
        let tgt = prof.join("core_user_600.dat");
        let v = s.call("copy_files", &args(json!({ "source": src.to_string_lossy(), "targets": [tgt.to_string_lossy()] }))).unwrap();
        assert_eq!(v[0]["ok"], true);
        assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&tgt).unwrap());
        let e = s.call("copy_files", &args(json!({ "source": src.to_string_lossy(), "targets": [prof.join("core_char_200.dat").to_string_lossy()] }))).unwrap();
        assert_eq!(e[0]["ok"], false, "a different kind is refused per target: {e}");
    }
```

If `copy_files` refuses a mixed-kind target as a whole-call `ErrDto` rather than per-target `ok: false`, assert `unwrap_err()["code"]` instead and say which.

Run: `cargo test -p app --lib mcp::copy` — expected: `unknown_tool`.

- [ ] **Step 2: Implement**

```rust
use crate::setup::{self, Aspect, BatchSource};
use crate::presets;

struct CopyArgs { source: BatchSource, targets: Vec<String>, aspects: Vec<Aspect>, allow_other_folders: bool }

impl EveMcp {
    /// One source (by char id, char file, or settings-preset name), one or
    /// more targets (by char id or char file), the aspects, the folder flag.
    fn copy_args(&self, args: &Args) -> Result<CopyArgs, Value> {
        let profiles = discover(&self.roots);
        let roster = accounts::load_roster(&self.roots, &self.dir);
        let char_path = |id: u64| -> Result<String, Value> {
            locate(id, &profiles, &roster)
                .map(|(c, _)| c.to_string_lossy().into_owned())
                .ok_or_else(|| err("unknown_character", format!("no core_char_{id}.dat in any profile; call list_characters")))
        };

        let src_id: Option<u64> = opt(args, "source_char_id")?;
        let src_file: Option<String> = opt(args, "source_char_file")?;
        let src_preset: Option<String> = opt(args, "source_settings_preset")?;
        let given = src_id.is_some() as u8 + src_file.is_some() as u8 + src_preset.is_some() as u8;
        if given == 0 { return Err(err("missing_field", "one of source_char_id, source_char_file, source_settings_preset")); }
        if given > 1 { return Err(err("bad_arguments", "give exactly one source")); }

        let mut targets: Vec<String> = opt::<Vec<String>>(args, "target_char_files")?.unwrap_or_default();
        for id in opt::<Vec<u64>>(args, "target_char_ids")?.unwrap_or_default() {
            targets.push(char_path(id)?);
        }
        if targets.is_empty() { return Err(err("missing_field", "target_char_ids and/or target_char_files, at least one target")); }

        let source = if let Some(id) = src_id {
            BatchSource::Character { path: char_path(id)? }
        } else if let Some(p) = src_file {
            BatchSource::Character { path: p }
        } else {
            let name = src_preset.expect("checked");
            let dir = presets::preset_path(&self.dir, &name).map_err(|e| err("unknown_settings_preset", e.0))?;
            if !dir.is_dir() { return Err(err("unknown_settings_preset", format!("no settings preset `{name}`; call settings_presets_list"))); }
            let anchor_dir = Path::new(&targets[0]).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            BatchSource::Preset { dir: dir.to_string_lossy().into_owned(), anchor_dir }
        };
        Ok(CopyArgs {
            source, targets,
            aspects: req(args, "aspects")?,
            allow_other_folders: opt(args, "allow_other_folders")?.unwrap_or(false),
        })
    }
}
```

`presets::preset_path(app_data, name) -> Result<PathBuf, NameError>` with `NameError(pub String)` (`presets.rs:39, 84`). `Aspect` deserialises from its snake_case names, so `req::<Vec<Aspect>>` accepts `["overview"]` and rejects anything else as `bad_arguments`.

Tool defs — the schema's `aspects` enum lists the seven names:

```rust
        ToolDef {
            name: "copy_preview",
            description: "Plan a copy of one character's settings onto others without changing anything: which files would be written (char_writes, account_writes — an account file is shared by every character on it, so collateral_char_ids names the siblings that change too), which targets are excluded and why, and source_error if the source cannot be used. Source: source_char_id, source_char_file, or source_settings_preset (a saved bundle from settings_presets_list). Targets: target_char_ids and/or target_char_files. aspects: layout, overview, autofill, keybinds, probe_formations, fleet, or everything (whole files). allow_other_folders lets targets in another profile folder in. ALWAYS call this before copy_apply and show the user the plan.",
            schema: || obj(json!({
                "source_char_id": { "type": "integer" }, "source_char_file": { "type": "string" }, "source_settings_preset": { "type": "string" },
                "target_char_ids": { "type": "array", "items": { "type": "integer" } },
                "target_char_files": { "type": "array", "items": { "type": "string" } },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] }, "minItems": 1 },
                "allow_other_folders": { "type": "boolean" }
            }), &["aspects"]),
        },
        ToolDef {
            name: "copy_apply",
            description: "Perform the copy copy_preview planned, with the same arguments. WRITES TO DISK IMMEDIATELY: every target file is backed up, then replaced atomically; returns [{path, ok, backup_path, error}] per file. Only copy onto characters that are logged out. If a target file is open here, open it again afterwards — the in-memory copy is stale.",
            schema: || obj(json!({
                "source_char_id": { "type": "integer" }, "source_char_file": { "type": "string" }, "source_settings_preset": { "type": "string" },
                "target_char_ids": { "type": "array", "items": { "type": "integer" } },
                "target_char_files": { "type": "array", "items": { "type": "string" } },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] }, "minItems": 1 },
                "allow_other_folders": { "type": "boolean" }
            }), &["aspects"]),
        },
        ToolDef {
            name: "copy_files",
            description: "Copy one settings file's bytes as-is onto other files of the same kind (character onto characters, account onto accounts) — no pairing, no aspects. WRITES TO DISK IMMEDIATELY with a backup per target; returns [{path, ok, backup_path, error}]. Only onto logged-out characters; reopen a target if it was open here.",
            schema: || obj(json!({ "source": { "type": "string" }, "targets": { "type": "array", "items": { "type": "string" }, "minItems": 1 } }), &["source", "targets"]),
        },
```

`call` arms:

```rust
            "copy_preview" => {
                let c = self.copy_args(args)?;
                ok(setup::setup_preview(&self.roots, &self.dir, &c.source, &c.targets, &c.aspects, c.allow_other_folders))
            }
            "copy_apply" => {
                let c = self.copy_args(args)?;
                ok(setup::setup_apply(&self.roots, &self.dir, &c.source, &c.targets, &c.aspects, c.allow_other_folders).map_err(fail)?)
            }
            "copy_files" => ok(setup::copy_files(&self.roots, &req::<String>(args, "source")?, &req::<Vec<String>>(args, "targets")?).map_err(fail)?),
```

`SetupPlan`, `CharWrite`, `AccountWrite`, `ExcludedTarget`, `TargetResult` derive `Serialize` (they are Tauri command results). `setup` is `mod setup;` in `lib.rs` — `crate::setup` is reachable; if any of its functions are not `pub`, they are `pub` already for `lib.rs`'s use (`setup.rs:446, 523, 617`).

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp::` green; clippy clean.

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: copy_preview, copy_apply, copy_files

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Settings presets (2 tools)

Spec §2.9. App-owned bundles.

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`

**Interfaces:**
- Consumes: `presets::{list, rename, delete, export_to, import_from}`, `setup::preset_save`.
- Produces: `fn settings_presets_view(dir) -> Value`, tools `settings_presets_list`, `settings_preset_edit`.

- [ ] **Step 1: Failing tests**

```rust
    #[test]
    fn settings_presets_create_from_the_open_files_then_rename_export_import_delete() {
        let (root, prof, app_dir) = temp_profile();
        let s = EveMcp::new(app_dir.clone(), vec![root]);
        s.call("open", &args(json!({ "user_file": prof.join("core_user_500.dat").to_string_lossy(), "char_file": prof.join("core_char_100.dat").to_string_lossy() }))).unwrap();
        assert_eq!(s.call("settings_presets_list", &Args::new()).unwrap(), json!([]));

        let v = s.call("settings_preset_edit", &args(json!({ "op": "create", "name": "PvP kit", "aspects": ["overview"] }))).unwrap();
        assert_eq!(v["presets"][0]["name"], "PvP kit");
        assert_eq!(v["presets"][0]["aspects"], json!(["overview"]));
        assert_no_paths(&v, "settings_preset_edit");
        assert_eq!(s.call("settings_preset_edit", &args(json!({ "op": "create", "name": "PvP kit", "aspects": ["overview"] }))).unwrap_err()["code"], "exists");

        let v = s.call("settings_preset_edit", &args(json!({ "op": "rename", "name": "PvP kit", "new_name": "Fleet kit" }))).unwrap();
        assert_eq!(v["presets"][0]["name"], "Fleet kit");

        let out = app_dir.join("fleet-kit.esp");
        s.call("settings_preset_edit", &args(json!({ "op": "export", "name": "Fleet kit", "path": out.to_string_lossy() }))).unwrap();
        assert!(out.exists());
        s.call("settings_preset_edit", &args(json!({ "op": "delete", "name": "Fleet kit" }))).unwrap();
        assert_eq!(s.call("settings_presets_list", &Args::new()).unwrap(), json!([]));
        let v = s.call("settings_preset_edit", &args(json!({ "op": "import", "path": out.to_string_lossy() }))).unwrap();
        assert_eq!(v["imported_as"], "Fleet kit");
        assert_eq!(v["presets"].as_array().unwrap().len(), 1);
    }
```

The `exists` code: read what `setup::preset_save` returns when `overwrite` is false and the name is taken (`setup.rs:661` onward) — use its actual `ErrDto.code` in the assertion and say which. If the export file extension the app uses is not `.esp`, use `presets::export_to`'s expectation (it takes any path).

Run: `cargo test -p app --lib mcp::settings_presets` — expected: `unknown_tool`.

- [ ] **Step 2: Implement**

```rust
/// The app's saved bundles, without their folder paths.
fn settings_presets_view(dir: &Path) -> Value {
    let list: Vec<Value> = presets::list(dir).into_iter().map(|p| json!({
        "name": p.name, "aspects": p.aspects, "full": p.full, "modified_unix": p.modified_unix, "error": p.error,
    })).collect();
    Value::Array(list)
}

impl EveMcp {
    fn settings_preset_edit(&self, args: &Args) -> ToolResult {
        let op: String = req(args, "op")?;
        let mut extra = Map::new();
        match op.as_str() {
            "create" => {
                let name: String = req(args, "name")?;
                let aspects: Vec<Aspect> = req(args, "aspects")?;
                let overwrite = opt::<bool>(args, "overwrite")?.unwrap_or(false);
                setup::preset_save(&self.state, &self.dir, &name, &aspects, overwrite).map_err(fail)?;
            }
            "rename" => presets::rename(&self.dir, &req::<String>(args, "name")?, &req::<String>(args, "new_name")?).map_err(|e| err("preset", e))?,
            "delete" => presets::delete(&self.dir, &req::<String>(args, "name")?).map_err(|e| err("preset", e))?,
            "export" => presets::export_to(&self.dir, &req::<String>(args, "name")?, Path::new(&req::<String>(args, "path")?)).map_err(|e| err("preset", e))?,
            "import" => {
                let name = presets::import_from(&self.dir, Path::new(&req::<String>(args, "path")?)).map_err(|e| err("preset", e))?;
                extra.insert("imported_as".into(), json!(name));
            }
            _ => return Err(unknown_op(&op)),
        }
        let mut v = json!({ "presets": settings_presets_view(&self.dir) });
        v.as_object_mut().unwrap().extend(extra);
        Ok(v)
    }
}
```

Tool defs:

```rust
        ToolDef {
            name: "settings_presets_list",
            description: "A settings preset is a saved bundle of one character's settings kept by this app — not an overview preset (see overview_presets_edit). Lists them: [{name, aspects, full, modified_unix, error}]. full means whole files were saved (aspects: everything).",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "settings_preset_edit",
            description: "A settings preset is a saved bundle of one character's settings kept by this app — not an overview preset. One op per call. create {name, aspects, overwrite?} saves the OPEN files' chosen aspects (open the character and its account first); rename {name, new_name}; delete {name}; export {name, path} writes a shareable file; import {path} adds one (returns imported_as). WRITES TO DISK IMMEDIATELY — the app's own preset folders, never a settings file. Returns {presets: [...]}. To put a preset onto characters, use copy_preview/copy_apply with source_settings_preset.",
            schema: || obj(json!({
                "op": { "type": "string", "enum": ["create", "rename", "delete", "export", "import"] },
                "name": { "type": "string" }, "new_name": { "type": "string" }, "path": { "type": "string" },
                "aspects": { "type": "array", "items": { "type": "string", "enum": ["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"] } },
                "overwrite": { "type": "boolean" }
            }), &["op"]),
        },
```

`call` arms:

```rust
            "settings_presets_list" => Ok(settings_presets_view(&self.dir)),
            "settings_preset_edit" => self.settings_preset_edit(args),
```

- [ ] **Step 3: Run, clippy, commit**

`cargo test -p app --lib mcp::` green; clippy clean.

```bash
git add app/src-tauri/src/mcp.rs
git commit -m "MCP: settings presets

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Primer, smoke test, docs, verification, end-to-end

Spec §3, §4 (invariants/smoke), §5 (README/CHANGELOG), §6.

**Files:**
- Modify: `app/src-tauri/src/mcp_primer.md`, `app/src-tauri/src/mcp.rs` (`TOPICS`, tests), `app/src-tauri/tests/mcp_stdio.rs`, `README.md`, `CHANGELOG.md`, the spec's status line.

- [ ] **Step 1: Failing tests**

Change the existing `primer_has_the_five_topics_in_order_and_none_is_empty` to compare against the new `TOPICS` (rename it `primer_has_every_topic_in_order_and_none_is_empty`) and add:

```rust
    #[test]
    fn the_workflow_section_names_every_editor() {
        let w = primer("workflow").unwrap();
        for prefix in ["layout_", "autofill_", "keybind", "neocom_", "hud_", "fleet_", "chat_", "copy_", "settings_preset"] {
            assert!(w.contains(prefix), "workflow does not mention {prefix}");
        }
        assert!(w.contains("layout_render"));
    }

    #[test]
    fn eve_guide_serves_the_three_new_topics() {
        let s = EveMcp::for_tests();
        for (t, word) in [("layout", "anchor"), ("keybinds", "stolen"), ("copy", "collateral")] {
            let v = s.call("eve_guide", &args(json!({ "topic": t }))).unwrap();
            assert!(v["text"].as_str().unwrap().contains(word), "{t} should mention {word}");
        }
    }
```

In `tests/mcp_stdio.rs`, the sorted list becomes the 46 names:

```rust
        [
            "autofill_clear_all", "autofill_get", "autofill_set", "builtin_presets", "chat_get", "chat_set_splits",
            "copy_apply", "copy_files", "copy_preview", "eve_guide", "fleet_edit", "fleet_get", "groups_search",
            "hud_get", "hud_set", "keybind_set", "keybinds_get", "layout_edit", "layout_get", "layout_render",
            "list_backups", "list_characters", "lookup_character", "neocom_edit", "neocom_get", "open",
            "overview_appearance_edit", "overview_columns_edit", "overview_get", "overview_pack_export",
            "overview_pack_import", "overview_pack_preview", "overview_presets_edit", "overview_tabs_edit",
            "probes_add_yaml", "probes_export_yaml", "probes_get", "probes_remove", "probes_reorder", "probes_set",
            "restore_backup", "save", "settings_preset_edit", "settings_presets_list", "status", "undo",
        ]
```

Run: `cargo test -p app --lib mcp::` and `cargo test --test mcp_stdio` (from `app/src-tauri`) — expected: the primer tests fail, the smoke test fails on the list.

- [ ] **Step 2: The primer**

In `mcp.rs`: `pub(crate) const TOPICS: [&str; 8] = ["workflow", "overview", "presets", "states", "probes", "layout", "keybinds", "copy"];`.

In `mcp_primer.md`, add to the end of `## workflow` (before `Unsure about a concept:`) this paragraph, and extend that last line's list to `overview`, `presets`, `states`, `probes`, `layout`, `keybinds` or `copy`:

```markdown
Beyond the overview and probes, the same files hold: the window **layout** (`layout_get`, `layout_render` for a picture, `layout_edit`), the **Neocom** bar (`neocom_get`/`neocom_edit`) and **HUD** (`hud_get`/`hud_set`) — character file; **autofill** (`autofill_get`/`autofill_set`/`autofill_clear_all`), **keybinds** (`keybinds_get`/`keybind_set`) and **chat** splits (`chat_get`/`chat_set_splits`) — account file; **fleet** settings (`fleet_get`/`fleet_edit`, `lookup_character`) — both. Two operations work across files and WRITE IMMEDIATELY rather than through `save`: **copy settings** (`copy_preview`, then `copy_apply`; `copy_files` for a whole file) and **settings presets** (`settings_presets_list`, `settings_preset_edit`) — saved bundles this app keeps, unrelated to overview presets.
```

Append three sections, in this order after `## probes`:

```markdown
## layout

Call `layout_render` first — look before moving anything. The picture shows every open window as a labelled box on the screen at the file's reference size (`reference_w` × `reference_h`, the resolution the settings were saved at); the legend carries each window's id and pixel geometry. Window ids are EVE's internal names (`overview`, `market`, `fitting`, `chatchannel_local`, …); `label` is the readable one. Geometry is `x, y, w, h` in pixels from the top-left, and `layout_edit`'s `set_geometry` takes any subset of the four. A window saved at a different resolution (`resolution_matches` false) is re-stamped to the reference when you move it, so the client places it where you put it.

A **stack** is a tabbed container: its `members` are windows shown as tabs, drawn at the `anchor_id` window's geometry. `stack_create {a, b}` starts one, `stack_add` / `stack_unstack` change membership, `stack_reorder` sets the tab order (list every member), `stack_delete_orphans` removes containers with no members. Flags per window are the file's own keys — `openWindows`, `pinnedWindows`, `lockedWindows`, `compactWindows`, `collapsedWindows`, `minimizedWindows`, … — listed by `layout_get` with `settable`; a flag the file has no table for is not settable. Closed windows keep their geometry; `include_closed` on `layout_render` outlines them. The picture shows windows and stacks only — not the HUD or the Neocom.

## keybinds

A binding is optional modifiers plus one key, shown as `Ctrl+Alt+Q`. `keybinds_get` lists every command with a label and group; `combo` is what the player reads, `keys` the stored codes. `keybind_set` takes the key by name (`Q`, `F1`, `Num 5`, `Page Up` — the names that appear in `combo`) and `ctrl` / `alt` / `shift` flags; omit the key to unbind. A combo another command already holds moves to the new command and `stolen` names the losers — tell the user, since that command is now unbound. `available` false means the account never opened the in-game keybinding screen and has no table to edit yet.

## copy

Copy settings moves aspects from one source onto other characters. Aspects: `layout` (windows, stacks, Neocom, HUD), `overview` (presets and appearance — account side — plus column widths — character side), `autofill`, `keybinds`, `probe_formations`, `fleet`, or `everything` (whole files). Character-side aspects write the target's `core_char` file; account-side aspects write the target's `core_user` file — and an account file is shared by every character on that account, so `collateral_char_ids` lists the siblings that change too. Always `copy_preview` first and show the user the plan: which files, which collateral characters, which targets are excluded and why (an unpaired target, a file in another profile folder unless `allow_other_folders`). `copy_apply` then writes each target with a backup; it does not go through `save`, so a target that was open here must be opened again. The source can be a character or a **settings preset** — a bundle saved by `settings_preset_edit` from the open files, listed by `settings_presets_list`, exportable as a file; not an overview preset.
```

- [ ] **Step 3: Run the primer and smoke tests**

`cargo test -p app --lib mcp::` green; `cargo test --test mcp_stdio` (from `app/src-tauri`) green with 46 names.

- [ ] **Step 4: README and CHANGELOG**

In README's "AI access" section, replace the sentence "is now an MCP server exposing the overview and probe-formation editors" (or its equivalent) with: "is an MCP server exposing every editor — overview, probe formations, window layout (with a rendered picture), autofill, keybinds, Neocom, HUD, fleet and chat — plus copy settings and settings presets."

CHANGELOG, under `## [Unreleased]` → `### Added`, after the existing AI-access bullet:

```markdown
- **The assistant can now edit everything the app edits.** Window layout — with a picture of it — autofill, keybinds, the Neocom bar, HUD, fleet and chat settings, plus copy settings and settings presets, over the same MCP server.
```

Spec status line → `Status: designed 2026-09-20, planned 2026-09-20 (docs/superpowers/plans/2026-09-20-mcp-all-surfaces.md).`

- [ ] **Step 5: The four gates, by exit code**

```powershell
cargo clippy --workspace --all-targets -- -D warnings; echo "clippy exit $LASTEXITCODE"
cargo test --workspace; echo "cargo test exit $LASTEXITCODE"
cd app; npm run check; echo "check exit $LASTEXITCODE"; npm test; echo "npm test exit $LASTEXITCODE"; cd ..
```

Four zeros or stop.

- [ ] **Step 6: End-to-end**

1. Rebuild the release the Claude Code registration points at: from `app/src-tauri`, `cargo build --release` **in the background, polling** (a foreground build can sit silent past the 10-minute watchdog).
2. Extend the stdio script from slice 1's approach — a Node script under `.superpowers/sdd/<this plan>/` (git-ignored) — to drive, on temp copies of the synthetic fixtures: `open` (char + user), `layout_get`, `layout_render` (assert one `image/png` block whose base64 decodes to a PNG signature, and that the legend lists the windows), `layout_edit` `set_geometry`, `keybinds_get`, `keybind_set`, `neocom_get`, `hud_get`, `fleet_get`, `chat_get`, `autofill_get`, `settings_presets_list`, `copy_preview` on the temp profile, `save`. Exit non-zero on any `isError` or failed expectation; record the output.
3. Model-in-the-loop (the user, in a fresh Claude Code session): "show me my layout", "move my market window to the top-left", "bind Q to approach", "add <name> to my watchlist in red", "copy my main's overview onto <alt>" (expect `copy_preview` before `copy_apply` without being told).

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/mcp_primer.md app/src-tauri/src/mcp.rs app/src-tauri/tests/mcp_stdio.rs README.md CHANGELOG.md docs/superpowers/specs/2026-09-20-mcp-all-surfaces-design.md
git commit -m "MCP: primer for layout, keybinds and copy; 46 tools; docs

<gate exit codes and the e2e outcome>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

Then `superpowers:finishing-a-development-branch` — branch `feat/mcp-all-surfaces`, target `master`.

---

## Self-review against the spec

- §1 conventions → every task's schemas go through `obj`/`op_item` and the existing invariants test; paths stripped in Tasks 4, 6, 7 (`assert_no_paths` in the tests), immediate-write wording in Tasks 9–10's descriptions; `png`/`base64` only (Task 1).
- §2.1 layout → Task 7 (get/edit) and Task 8 (render, `mcp_render.rs`, the reply variant via `blocks` in Task 1). §2.2 autofill, §2.3 keybinds → Task 3 (+ Task 2 for the JSON table). §2.4 neocom → Task 5. §2.5 HUD, §2.7 chat → Task 4. §2.6 fleet + lookup → Task 6. §2.8 copy → Task 9. §2.9 settings presets → Task 10.
- §3 primer → Task 11. §4 tests → distributed; the render tests in Task 8 match §4's list; the invariants and smoke test in Task 11. §5 files → all present. §6 DoD → Task 11 steps 5–6. §7 deferred → untouched.
- Counts: 24 + 3 (layout) + 3 (autofill) + 2 (keybinds) + 2 (neocom) + 2 (HUD) + 2 (chat) + 3 (fleet) + 3 (copy) + 2 (presets) = 46; the smoke list in Task 11 has 46 entries.
- Type consistency: `batch(args, apply, finish)` from Task 1 is what Tasks 5, 6, 7 call; `open_char`/`assert_no_paths` (Task 4) used by 5, 6, 7, 8; `hud_entry_view` (Task 4) used by `fleet_view` (Task 6); `temp_profile` (Task 9) used by Task 10; `PNG_KEY`/`blocks` (Task 1) used by Task 8; `vk_code`/`vk_labels` (Task 2) used by Task 3.
- Known soft spots called out inside the tasks, each with what to do: flag names from `windows.rs` (Task 7), the `png` API names and one outline pixel (Task 8), `copy_files`' mixed-kind error shape (Task 9), the `exists` code from `preset_save` (Task 10), `TextContent`/`ImageContent` field names (Task 1).
