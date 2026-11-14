Inputs that once made a fuzz target of this crate fail (STORIES.md §0.2 item 7, US-050).
`fuzz_format` replays every file under `format/` before it generates anything, so a
counter-example committed here stays fixed.
