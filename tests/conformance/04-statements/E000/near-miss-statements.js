// Every permitted statement form: no diagnostic.
const m = new Map()
const xs = [1, 2]
for (const [k, v] of m) {
  if (k === v) {
    continue
  }
}
for await (const x of xs) {
  f(x)
}
for (let i = 0; i < 3; i += 1) {
  if (i === 1) {
    break
  } else if (i === 2) {
    continue
  } else if (i === 3) {
  } else if (i === 4) {
  } else if (i === 5) {
  } else if (i === 6) {
  } else if (i === 7) {
  } else if (i === 8) {
  } else if (i === 9) {
  } else if (i === 10) {
  } else {
    f(i)
  }
}
for (let j = 3; j > 0; j--) {}
while (xs.length > 0) {
  xs.pop()
}
function f(a) {
  if (a === undefined) {
    throw new Error("a")
  }
  return a
}
{
  const local = 1
  f(local)
}
