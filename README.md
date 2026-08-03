# typstation

A [Typst](https://typst.app) editor with live preview, written in Rust.

> **Status: early work in progress.** The multi-document editor, native file
> operations, project tree, preview workflow, and session recovery are usable.
> Broader editing features are still in development.

## What works today

- A reusable `World` implementation for the Typst compiler, built on
  [`typst-kit`](https://crates.io/crates/typst-kit).
- An [Iced](https://iced.rs) interface with movable editor and preview panes.
- Debounced live compilation in a persistent background worker.
- A scrollable SVG preview containing every page of the document.
- Typst errors and warnings attached to their source ranges in the editor.
- Selection formatting for strong emphasis, emphasis, underline, bullet lists,
  and numbered lists.
- New, Open, Save, and Save As operations using native file dialogs.
- Keyboard shortcuts for file operations and inline formatting.
- Atomic document replacement that preserves existing file permissions and
  symbolic links.
- PDF export from the current editor snapshot using the persistent compiler
  worker.
- Unicode-aware search and replace with match navigation, whole-word and case
  options, and single-step undo for Replace All.
- A recursive Typst project tree and multiple document tabs.
- Automatic reload of clean files changed on disk, with explicit conflict
  handling when local edits exist.
- Unsaved-change tracking and confirmation before closing a tab or the window.
- Debounced, atomic session recovery for the project root, open tabs, unsaved
  drafts, active document, and pane layout.
- Embedded and system fonts.
- Package loading from [Typst Universe](https://typst.app/universe), with
  download and on-disk caching.
- Incremental source replacement while reusing fonts, packages, and imported
  file caches.

## Roadmap

- [x] Open and save real files
- [x] Warn about unsaved changes when closing the window
- [x] Add keyboard shortcuts and atomic document saves
- [x] Export PDF from the interface
- [x] Project file tree and multi-file editing
- [x] Search and replace
- [x] Recover open tabs and unsaved drafts after restarting

## Building

```sh
cargo run --release
```

To compile the bundled demonstration directly to `out/tutorial.pdf`:

```sh
cargo run --example export_pdf
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
