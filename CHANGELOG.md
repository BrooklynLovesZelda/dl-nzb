# Changelog

All notable changes to dl-nzb will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-12-08

### Added
- New `TuningConfig` for performance parameters (pipeline_size, connection_wait_timeout, large_file_threshold)
- Centralized `patterns` module with regex-based RAR and PAR2 detection
- Smooth byte-level progress reporting for PAR2 verification
- Real-time RAR extraction progress via file size monitoring
- Connection wait feedback ("Waiting for connection..." messages)
- Comprehensive unit tests for file pattern matching

### Changed
- Replaced par2cmdline-turbo (C++ FFI) with par2-rs (pure Rust)
- Split `post_process.rs` (714 lines) into focused modules:
  - `post_processor.rs` (157 lines) - orchestration
  - `par2.rs` (236 lines) - PAR2 verification/repair
  - `rar.rs` (308 lines) - RAR extraction
- Improved connection pool management with exponential backoff
- Reduced default connections from 40 to 20 for stability
- Use `Arc<Config>` in download hot path to reduce cloning
- Progress bar templates now use `expect()` with descriptive messages
- PAR2 message parsing now uses level + content matching for reliability

### Fixed
- Connection pool exhaustion when downloading many files
- Error logs breaking terminal progress bar rendering
- Mutex lock panics on poisoned locks (now gracefully handled)
- RAR multi-part detection edge cases (.part01, .part001, .part0001)

## [0.1.0] - 2025-01-13

### Initial Release

First public release of dl-nzb, a fast Usenet NZB downloader written in Rust.

#### Features

- Fast parallel downloads with configurable connection pooling
- Built-in PAR2 verification and repair
- Built-in RAR extraction support
- Automatic file deobfuscation for common obfuscated naming patterns
- Real-time progress display with speed and ETA
- SSL/TLS support for secure connections
- Configurable via TOML config file or environment variables
- Command-line overrides for all major settings
- Async I/O with Tokio runtime for maximum throughput
- Memory-efficient streaming downloads
- Automatic retry on failed segments
- Smart connection management with health checks
- Cross-platform support (Linux, macOS, Windows)

#### Commands

- `dl-nzb <file.nzb>` - Download NZB files
- `dl-nzb test` - Test server connection
- `dl-nzb config` - Show configuration file location and contents
- `dl-nzb -l <file.nzb>` - List NZB contents without downloading

#### Configuration

- Auto-generated config file on first run
- Support for local `dl-nzb.toml` override files
- Environment variable overrides with `DL_NZB_` prefix
- Configurable download directory, connections, memory limits, and post-processing options

#### Technical Details

- Single binary with no runtime dependencies
- PAR2 support via pure Rust par2-rs library with SIMD
- RAR extraction compiled in
- Optimized build with LTO
