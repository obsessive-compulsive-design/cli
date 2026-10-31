// expect: S008 layout @12:14 "?.5:" -> " ? .5 : "
// A valid module exercising punctuators that need longest match; a?.5:1 is a ternary, not
// an optional chain, which the layout spaces (S008).
export const f = (a, b, ...rest) => {
  const c = a ** b >>> 1
  const d = c?.x ?? rest[0]
  let e = 1
  e **= 2
  e >>>= 1
  e ??= 3
  const t = a ? .5 : 1
  const u = a?.5:1
  return { c, d, e, t, u, ...rest }
}
