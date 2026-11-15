// Every `.js` file beneath `./src` is reached (one through `import()` alone) or is a test;
// `src/data.json` is not `.js`, so the rule does not name it.
import { u } from "./src/used.js"

const lazy = await import("./src/lazy.js")

export const e = [u, lazy]
