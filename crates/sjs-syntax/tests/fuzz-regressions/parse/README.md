# Parser fuzz regressions

Every `*.bin` here is an input that once made `fuzz_parse` panic. The fuzz loop replays
them before generating new inputs (`sjs_testkit::fuzz`), so a fixed crash stays fixed.
A new crash is saved here automatically; commit the file with the fix.
