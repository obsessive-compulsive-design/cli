// A valid multi-byte file: an accented letter, an emoji, and U+FEFF inside a string
// (only a *leading* BOM is rejected). No expectation: no diagnostic.
const café = "😀"
const zwnbsp = "﻿"
