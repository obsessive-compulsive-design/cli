// `src/orphan.js` is beneath `./src` and nothing the root imports reaches it.
import { u } from "./src/used.js"

export const e = u
