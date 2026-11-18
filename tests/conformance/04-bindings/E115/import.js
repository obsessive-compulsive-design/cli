// expect: E115 unused-binding @3:17
// Nothing reads `size`.
import { scale, size } from "./size.js"

export const kept = scale
