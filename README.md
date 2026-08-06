# typstation

A [Typst](https://typst.app) editor with live preview, written in Rust.

> **Status: early work in progress.** The multi-document editor, native file
> operations, project tree, multi-file compilation, preview workflow, and
> session recovery are usable. Broader editing features are still in
> development.

## What works today

- A reusable `World` implementation for the Typst compiler, built on
  [`typst-kit`](https://crates.io/crates/typst-kit).
- An [Iced](https://iced.rs) interface with movable project, editor, and preview panes.
- Debounced live compilation in a persistent background worker that coalesces
  stale preview requests without dropping exports.
- A scrollable SVG preview with separate pages and persistent zoom controls.
- Bidirectional source navigation: click the preview to reveal its Typst source,
  or use the platform command modifier with click/J, or the preview's Locate
  command, to reveal the editor cursor in the rendered page.
- Typst errors and warnings attached to their source ranges, including errors
  in imported project files and navigation from the Problems panel.
- Selection formatting for strong emphasis, emphasis, underline, bullet lists,
  and numbered lists.
- New, Open, Save, Save As, Save All, and optional autosave operations using
  native file dialogs and atomic writes.
- Keyboard shortcuts for file operations and inline formatting.
- Atomic document replacement that preserves existing file permissions and
  symbolic links.
- PDF, SVG, and experimental HTML export from the current editor snapshot.
- Unicode-aware search and replace with match navigation, whole-word and case
  options, and single-step undo for Replace All.
- Project-wide search and replace with navigation to file, line, and column.
- A recursive Typst project tree, multiple document tabs, and create, rename,
  and delete operations for project files.
- Keyboard tree navigation plus move, duplicate, and copy-path operations.
- Tab cycling, reordering, reopening, recent projects, and drag-and-drop opening.
- A persistent project main document: edit imported files while the preview and
  PDF export continue compiling the selected entry point.
- Unsaved open imports compiled as in-memory overlays, without writing them to
  disk first.
- Native filesystem notifications through `notify`, debounced before project
  rescans, external-change checks, and preview recompilation.
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
- Toolbar commands for undo, redo, line comments, duplication, and line
  movement.
- Persistent settings for indentation, automatic pairs, line wrapping, gutter,
  editor font size, preview zoom, and light/dark theme.

## Roadmap

- [x] Open and save real files
- [x] Warn about unsaved changes when closing the window
- [x] Add keyboard shortcuts and atomic document saves
- [x] Export PDF from the interface
- [x] Project file tree and multi-file editing
- [x] Search and replace
- [x] Recover open tabs and unsaved drafts after restarting
- [x] Compile unsaved imported files as overlays
- [x] Navigate diagnostics from imported project files
- [x] Create, rename, and delete project files
- [x] Render and zoom individual preview pages
- [x] Expose basic editor commands in the toolbar
- [x] Replace filesystem polling with native notifications
- [x] Persist editor, preview, and theme settings
- [x] Navigate bidirectionally between source text and preview regions
- [x] Keep a persistent project main document while editing imports
- [x] Keep the project tree inside the movable, persistent pane grid
- [x] Coalesce obsolete preview compilation requests
- [x] Search and replace across the project
- [x] Save all, autosave, recent projects, and complete tab shortcuts
- [x] Navigate and manipulate the project tree with the keyboard

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
