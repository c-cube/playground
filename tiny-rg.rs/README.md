# Tiny-rg

A simple reimplementation of 1% of [ripgrep](https://github.com/BurntSushi/ripgrep/), as a simple-ish benchmark.

It is used as: `tiny-rg <regex> <dir1> <dir2>…`.

What it does:
- traverse the directories, looking for regular files. It doesn't ignore any file.
- for each file, read it line by line assuming UTF-8; match the regex against each such line. As soon as a unicode error is met (e.g. on a binary file) it abandons that file.
- if `--print` is passed, print the matched lines
- print statistics at the end.
- if `--par` is passed, it can process multiple files in parallel. Each file is still processed sequentially.

Unlike `rg`, it doesn't highlight the matched part of each line, or care about `.gitignore` and such, or any of the many other features.

Use `RUST_LOG` to control the logging level.

