// `one.js` does export `a`; the finding is where the chain breaks, in `one.js`.
import { a } from "./one.js"

export const b = a
