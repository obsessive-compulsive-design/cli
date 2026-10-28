// Every permitted export form: no diagnostic.
import { a } from "./a.js"
export const b = 1
export let c = 2
export function d() {}
export async function e() {}
export class F {}
export { a, b as g }
export { h } from "./h.js"
