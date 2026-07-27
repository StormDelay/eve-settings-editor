// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { addableButtons } from "./neocom.ts";
import type { NeocomButton } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const btn = (id: string, btn_type = 1, icon_path = `${id}.png`): NeocomButton =>
  ({ index: 0, id, btn_type, icon_path, children: 0 });

{
  const onBar = [btn("chat", 10), btn("wallet")];
  // "implants" is in Original but never in the catalog fixture below — the id
  // that actually proves Original contributes what the catalog can't. "mail"
  // is in BOTH fixtures on purpose: it's the conflict case, checked below.
  const original = [btn("chat", 10), btn("mail"), btn("wallet"), btn("implants")];
  const catalog = [
    { id: "chat", btnType: 10, iconPath: "chat.png" },
    { id: "fleet", btnType: 1, iconPath: "fleet.png" },
    { id: "mail", btnType: 1, iconPath: "mail-catalog.png" },
  ];

  const add = addableButtons(onBar, original, catalog);
  const ids = add.map((a) => a.id);

  check("what is already on the bar is not addable", !ids.includes("chat") && !ids.includes("wallet"));
  check("the catalog contributes buttons Original never had", ids.includes("fleet"));
  check("Original contributes buttons the catalog never had", ids.includes("implants"));
  check("the result is sorted by id", ids.join(",") === [...ids].sort().join(","));
  check("no id appears twice", new Set(ids).size === ids.length);

  // Original came from this character's own client, so it wins a conflict.
  const mail = add.find((a) => a.id === "mail")!;
  check("Original wins over the catalog on a conflict", mail.iconPath === "mail.png");
  check("the source is reported", mail.source === "original"
    && add.find((a) => a.id === "fleet")!.source === "catalog");
}

{
  // A character with no Original still gets the whole catalog.
  const add = addableButtons([], [], [{ id: "fleet", btnType: 1, iconPath: "fleet.png" }]);
  check("no Original still yields the catalog", add.length === 1 && add[0].id === "fleet");
}
