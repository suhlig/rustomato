# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.0] - 2026-05-31

### Added

- Add --target to interrupt command
- Add missing `-1..-9` positional shortcuts
- Add log to break command
- Add export command

### Changed

- Move the positional indexes to be standalone
- Fix the target when annotating
- Update TODOs
- Promote the section about target selection to top level
- Unify target resolution with `0`, `-N`, and kind-aware defaults
- Allow HH:MM at more places
- Fix ill-documented start behavior
- Join hard-wrapped prose paragraphs onto single lines
- Improve consistency around upper case
- Stay silent when running without a terminal
- Use indicatif as it is more actively maintained
- Hide the cursor during progress bar
- Fix example for export command
- Move hooks documentation to separate document

## [2.0.0] - 2026-05-30

### Added

- Add cancel subcommand for pomodoro and break
- Add `rustomato list` command to show recent pomodori and breaks
- Add --target option for annotating by GUID, index, or timestamp
- Add show command
- Add paragraph policy
- Add man page

### Changed

- Reflow paragraph in README
- Extract inline command handlers from main() into separate functions
- Document interactive annotation
- Document annotate command
- Default to long duration every fourth break
- In verbose mode, print the abbreviated UUID upon start
- Handle empty RUSTOMATO_ROOT gracefully

### Removed

- Remove the journal command

## [1.2.0] - 2026-05-29

### Added

- Add daily report
- Add weekly report
- Add monthly report
- Add rolling report
- Add AGENTS.md

### Changed

- Refactor day report and print annotations
- Label hook output
- Infer subcommands
- Add --force

## [1.1.0] - 2026-05-29

### Added

- Add log command

### Changed

- Update README
- Enable renovate
- Update state transition diagram
- Enable full renovate mode
- Update actions/checkout action to v6 (#3)
- Update GitHub Artifact Actions (#4)
- Update softprops/action-gh-release action to v3 (#6)

## [1.0.0] - 2026-05-28

### Added

- Add release notes for v1.0.0

### Changed

- Add hook support
- Add interrupt command to pomodoro
- Add annotate command

### Fixed

- Remove duplicate changelog entry
- Strip the released binary

## [1.0.0] - 2026-05-28

### Changed

- Add hook support
- Add interrupt command to pomodoro
- Add annotate command

### Fixed

- Remove duplicate changelog entry
- Strip the released binary

## [0.1.0] - 2026-05-28

### Added

- Add completions

### Changed

- Update TODOs
- Bump Homebrew formula on release

## [0.0.12] - 2026-05-27

### Changed

- Fix release workflow

## [0.0.11] - 2026-05-27

### Added

- Address clippy findings

### Changed

- Fix pipeline
- Update dependencies
- Replace non-critical dependencies
- Add journal command
- Add changelog
- Change release workflow to use GitHub actions
- Use cargo-release again

## [0.0.11] - 2026-05-27

### Added

- Address clippy findings

### Changed

- Fix pipeline
- Update dependencies
- Replace non-critical dependencies
- Add journal command
- Add changelog
- Change release workflow to use GitHub actions
- Use cargo-release again

## [0.0.10] - 2025-05-22

### Changed

- Change status timestamps to appear in a more user-friendly format
- Use cargo fmt instead of rustfmt

### Removed

- Remove unwanted newlines

## [0.0.9] - 2025-05-21

### Added

- Add metadata necessary for cargo release

### Changed

- Use task library
- Update Rust to ed.2024
- Update dependencies
- Simplify project

## [0.0.8] - 2022-02-09

### Changed

- Change pipeline to be Linux only
- Derive app version from git (at built time)
- Bump to v0.0.8

## [0.0.7] - 2022-02-07

### Added

- Add status `stale`
- Add status command

### Changed

- Enforce `finished_at >= started_at` and `cancelled_at >= started_at`
- Move tasks to suhlig/concourse-task-store

## [0.0.6] - 2021-09-04

### Added

- Add simplest integration test
- Add pipelines for Linux and Windows
- Add ls task
- Add clippy task

### Changed

- Show progress bar
- Consistent naming of Pomodoro
- Migrate database on startup
- Separate library and cli
- Re-structure docs
- More tests
- Fix wording
- Set timestamps from Rust code
- Replace remaining shell tests with Rust code
- Clean up use of progress bar
- Embed state machine drawing in README
- Rename task output
- Install gcc-mingw-w64 for windows target only
- Simplify build in CI
- Cache .cargo directory
- Update CI cache path
- Add verbose argument
- Make Clippy happy
- Make rustfmt happy
- Change background to white
- Document Control-C behaviour on a Pomodoro vs. a break

### Removed

- Remove GitHub action

## [0.0.5] - 2021-08-30

### Changed

- Install sqlite3 dev dependencies

## [0.0.4] - 2021-08-30

### Changed

- Handle already running pom or break
- Rustfmt
- Update notes

### Removed

- Remove WASM target

## [0.0.3] - 2021-08-29

### Changed

- How to release
- Create database schema and simple tests
- Bring back the new state
- Split persistence and scheduling
- Beautification
- Persist kind and duration
- Fix insert order
- Update TODOs
- Implement singularity by PID

### Removed

- Remove new state

## [0.0.2] - 2021-08-19

### Added

- Add badge
- Add ideas about persistence

### Changed

- Explain how to notify
- Implement `pomodoro start`
- Pomodoro and Break get a UUID
- Simplify usage
- Move `run` to a separate function

## [0.0.1] - 2021-08-16

### Added

- Add `pomodoro` and `break` subcommands
- Add pomodoro and break subcommands
- Add state machine draft
- Add cancel and interrupt
- Add release workflow

### Changed

- Initial commit
- Read version and author from manifest
- Be more precise in help texts
- More active wording in help text
- Limit duration to 255 minutes
- Cancel a break on SIGINT

### Removed

- Remove authors
- Remove --verbose and --debug
