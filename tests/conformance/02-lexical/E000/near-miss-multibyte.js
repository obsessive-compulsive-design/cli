// A valid multi-byte file: an accented letter in a comment (café) and a string, an
// emoji, and U+FEFF inside a string (only a *leading* BOM is rejected). Identifiers
// are ASCII, so the letter stays out of them. No expectation: no diagnostic.
const cafe = "café 😀"

const zwnbsp = "﻿"
