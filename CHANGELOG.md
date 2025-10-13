# Changelog

All notable changes to dl-nzb will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-13

### Initial Release

First public release of dl-nzb, a fast Usenet NZB downloader written in Rust.

#### Features

- Fast parallel downloads with configurable connection pooling
- Built-in PAR2 verification and repair using par2cmdline-turbo
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

- Single 3.3MB statically-linked binary
- Zero runtime dependencies (PAR2 and RAR libraries compiled in)
- Optimized build with LTO and size optimization
- Uses par2cmdline-turbo for fast PAR2 operations
