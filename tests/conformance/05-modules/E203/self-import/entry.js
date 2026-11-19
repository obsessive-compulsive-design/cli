// expect: E203 import-cycle @3:30
// The smallest cycle: a module that imports itself.
import { kept as mine } from "./entry.js"

export const kept = mine
