// expect: E203 import-cycle @2:19
import { e } from "./entry.js"

export const p = e
