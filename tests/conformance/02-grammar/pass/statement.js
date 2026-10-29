// Statement: one of each kind.
function f(xs) {
  {
    const a = 1
    f(a)
  }
  if (xs) {
    f(xs)
  }
  for (const x of xs) {
    f(x)
  }
  for (let i = 0; i < 2; i += 1) {
    f(i)
  }
  while (xs.length > 9) {
    break
  }
  switch (xs.length) {
    case 1: {
      break
    }
    default: {
      f(0)
    }
  }
  try {
    f(1)
  } catch {
    throw new Error("x")
  }
  f(2)
  return xs
}
