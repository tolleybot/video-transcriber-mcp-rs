# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Video Transcriber MCP is a Rust MCP (Model Context Protocol) server that transcribes videos from 1000+ platforms and local files using whisper.cpp via `whisper-rs` bindings. It uses `yt-dlp` for downloading, `ffmpeg` for audio extraction, and outputs TXT/JSON/Markdown transcripts.

## Build & Development Commands

This project uses [Task](https://taskfile.dev/) as its task runner (see `Taskfile.yml`).

```bash
# Build
task build            # Release build (optimized, LTO)
task build:dev        # Debug build (fast compilation)
cargo build           # Standard cargo build

# Lint & Format
task lint             # cargo clippy -- -D warnings
task fmt              # cargo fmt
task fix              # clippy --fix + cargo fmt

# Check
cargo check           # Type-check without building

# Test (integration tests via Taskfile, requires whisper models + yt-dlp + ffmpeg)
task test:quick                                    # Short YouTube video with base model
task test:local VIDEO_PATH=/path/to/video.mp4      # Local file
task test:url VIDEO_URL=https://...                # Custom URL

# Setup
task setup            # Build + download base whisper model
task deps:check       # Verify yt-dlp, ffmpeg, models are installed

# Release
task release:check    # Pre-release: fmt + lint + build
task release VERSION=0.4.1  # Full release workflow
```

There are no unit tests (`cargo test` has nothing to run). Testing is done via the Taskfile integration tests that run the binary end-to-end.

## Architecture

### Transport Layer (`src/main.rs`)
The binary supports two MCP transport modes selected via `--transport`:
- **stdio** (default): For local MCP clients like Claude Code
- **http**: Streamable HTTP via axum on `--host`/`--port` for remote access

### MCP Server (`src/mcp/server_rmcp.rs`)
`VideoTranscriberServer` implements `rmcp::ServerHandler`. It wraps a `TranscriberEngine` in `Arc<Mutex<>>` and exposes 8 MCP tools: `transcribe_video`, `check_dependencies`, `list_supported_sites`, `list_transcripts`, `get_latest_transcript`, `delete_transcript`, `cleanup_old_transcripts`, `delete_all_transcripts`.

Tool dispatch is a match on tool name in `call_tool()` — all tool definitions (schemas, descriptions) are inline in `list_tools()`.

### Transcription Pipeline (`src/transcriber/`)
`TranscriberEngine` orchestrates three components:
- **`downloader.rs`** — `VideoDownloader`: shells out to `yt-dlp` for metadata and audio download
- **`audio.rs`** — `AudioProcessor`: shells out to `ffmpeg` to extract/convert audio to WAV
- **`whisper.rs`** — `WhisperTranscriber`: loads ggml model files and runs inference via `whisper-rs`
- **`types.rs`** — Domain types: `WhisperModel` (Tiny/Base/Small/Medium/Large), `TranscriptionOptions`, `TranscriptionResult`, `VideoMetadata`

Flow: URL → yt-dlp download → ffmpeg extract audio → whisper transcribe → save TXT/JSON/MD

### Paths (`src/utils/paths.rs`)
- Models: `~/.cache/video-transcriber-mcp/models/`
- Transcripts: `~/Downloads/video-transcripts/`

## Key Dependencies

- `rmcp` — MCP protocol server (stdio + streamable HTTP)
- `whisper-rs` — Rust bindings for whisper.cpp (requires C++ compilation)
- `axum` — HTTP server for the streamable HTTP transport
- `clap` — CLI argument parsing
- `async-process` — Spawning yt-dlp/ffmpeg as async subprocesses

## Runtime Dependencies

The binary requires these external tools at runtime:
- `yt-dlp` — video downloading
- `ffmpeg` — audio extraction
- Whisper model files (`.bin`) in the models directory
