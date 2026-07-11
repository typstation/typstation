# typstation

A [Typst](https://typst.app) editor with live preview, written in Rust.

> **Status: early work in progress.** This release reserves the name and
> contains the compilation core only — there is no editor UI yet. It compiles a
> built-in sample document to `out/tutorial.pdf`. Not useful to end users yet.

## What works today

- A reusable `World` implementation for the Typst compiler, built on
  [`typst-kit`](https://crates.io/crates/typst-kit).
- Embedded and system fonts.
- Package loading from [Typst Universe](https://typst.app/universe), with
  download and on-disk caching.
- Compiler-style diagnostics, with file, line, column and a source excerpt.
- Incremental recompilation: the expensive environment (font scanning, package
  resolution) is built once, and only the source text is swapped between
  compilations.

The split matters. Scanning system fonts costs ~165 ms, while compiling the
sample document costs ~21 ms. Rebuilding the `World` on every keystroke — the
easy mistake — would make live preview unusable:

| | cost |
| --- | --- |
| `TypstationWorld::new` (once) | ~170 ms |
| First compilation | ~21 ms |
| Recompilation after an edit | **~1.6 ms** |

## Roadmap

- [ ] Editor UI
- [ ] Live preview pane
- [ ] Open and save real files, instead of a built-in sample
- [ ] Inline diagnostics in the editor

## Building

```sh
cargo run --release
```

Linux needs `fontconfig` and OpenSSL development headers (`libssl-dev` /
`openssl-devel`), pulled in by the `scan-fonts` and `system-downloader` features
of `typst-kit`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
