# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
