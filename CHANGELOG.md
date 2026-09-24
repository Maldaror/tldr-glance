# Changelog

All notable changes to this project are documented in this file.

## [0.4.0] - 2026-09-24

### Changed

- Replaced the hand-rolled escape-aware quote scanner in RSC chunk extraction with serde_json's own string parsing.
- Unified tab-title formatting between tab drawing and click hit-testing so a format change can no longer desync click targets from what's rendered.
- Parser fallback errors are now only surfaced when both the new-format and old-format parsers fail, instead of logging on every routine fallback.

## [0.3.0] - 2026-09-24

### Added

- Loading spinner with per-edition progress during fetch.
- Retry of the same-date fetch before falling back to an older date; gzip support enabled.
- Parallelized edition fetches using `std::thread::scope`.

## [0.2.0] - 2026-09-20

### Added

- Prebuilt binary installation instructions.
- Release workflow building macOS and Linux binaries.
