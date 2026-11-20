// A function called, by name and through another binding, and a class made:
// no diagnostic.
const make = () => ({ size: 1 })

const again = make

export class Box {}

export const made = [make(), again(), new Box()]
