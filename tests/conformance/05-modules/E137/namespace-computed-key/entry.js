// expect: E137 missing-property @6:21
import * as ns from "./lib.js"

const key = "a"

export const b = ns[key]
