import { createRawSnippet } from "svelte";

/** Fixed text as a Snippet, so a spec can hand `children` to a primitive. */
export const text = (s: string) => createRawSnippet(() => ({ render: () => `<span>${s}</span>` }));
