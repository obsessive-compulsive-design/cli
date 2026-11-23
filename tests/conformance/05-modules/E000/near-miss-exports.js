// Every permitted export form: no diagnostic.
import { a } from "./a.js"

export const b = 1

export let c = 2

c += 1

export function d() {}

export async function e() {
  await Promise.resolve(1)
}

export class F {}

export { a, b as g }

export { h } from "./h.js"
