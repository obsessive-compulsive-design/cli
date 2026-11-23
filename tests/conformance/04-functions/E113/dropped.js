// expect: E113 floating-promise @7:1
// `save` gives a promise, and nothing waits for it: a failure would be lost.
const save = async (text) => {
  await Promise.resolve(text)
}

save("draft")
