# Changelog

All notable changes to atomio will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(with the v0.x caveat documented in [`docs/dev-practices.md`](docs/dev-practices.md#5-versioning--releases)).

## [Unreleased]

### Added
- Initial workspace scaffold: `atomio` binary + `editor_core` library.
- `editor_core::Buffer` (ropey-backed) with insert/remove/len.
- `editor_core::Selection` with caret helper.
- MIT license, README manifesto, roadmap, development practices.
- CI workflow: fmt, clippy, build, test on `macos-14`. Audit workflow.
- Issue templates and PR template.
- Release workflow stub (codesign + notarize — TODO).
- gpui 0.2.2 dependency and a Metal-rendered hello-window in `crates/atomio`.
  This is the v0.0 architecture go/no-go: gpui is workable as a crates.io
  dependency on Apple Silicon. Building locally requires Apple's Metal
  Toolchain (`xcodebuild -downloadComponent MetalToolchain`).
