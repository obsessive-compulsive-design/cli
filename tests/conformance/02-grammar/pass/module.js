// Module: imports first, then module items. No diagnostic.
import { a } from "./a.js"

export const b = a

const c = b

c.run()
