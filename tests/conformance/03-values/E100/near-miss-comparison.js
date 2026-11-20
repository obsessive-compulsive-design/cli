// The right operand of `===` and `!==` narrows a host value: no diagnostic.
export const isNull = (value) => value === null

export const isSet = (value) => value !== null
