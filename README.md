# Video Transcriber MCP

**High-performance video transcription MCP server using whisper.cpp (Rust)**

A Model Context Protocol (MCP) server that transcribes videos from **1000+ platforms** using whisper.cpp. For YouTube videos, it first tries to fetch existing captions directly (instant) before falling back to whisper transcription. Built with Rust for maximum performance.

## Features

- **YouTube captions fast path** - fetches existing captions directly via YouTube's InnerTube API (no download or transcription needed)
- **Whisper fallback** - full transcription pipeline for videos without captions (yt-dlp + ffmpeg + whisper.cpp)
- **1000+ platforms** supported via yt-dlp (YouTube, Vimeo, TikTok, Twitter, etc.)
- **Local video and audio files** supported (mp4, mov, mkv, mp3, wav, m4a, flac, etc.)
- **5 whisper model sizes** (tiny, base, small, medium, large)
- **Multilingual** - 90+ languages with medium/large models, best English results with small+
- **Multiple output formats** (TXT, JSON, Markdown)
- **Dual transport** - stdio (local) and Streamable HTTP (remote)
- **Single binary** - no Python or Node.js required

## Quick Start with Claude Code

The easiest way to get started — just ask Claude Code to do everything for you:

```
Clone https://github.com/tolleybot/video-transcriber-mcp-rs.git,
build it, download the medium whisper model, and add it as an MCP server.
```

Claude Code will handle the entire setup: cloning, building, downloading the model, and registering the MCP server. Once it's done, restart Claude Code and the transcription tool is ready to use.

Then just ask:

```
Transcribe this YouTube video: https://www.youtube.com/watch?v=VIDEO_ID

Transcribe this local file: /path/to/video.mp4
```

## Manual Installation

### Prerequisites

- **Rust** 1.85+ (for building from source)
- **cmake** 3.5+ (required to build whisper.cpp)
- **yt-dlp** - for downloading videos from non-YouTube platforms, or YouTube videos without captions
- **ffmpeg** - for audio extraction

```bash
# macOS (including M1/Apple Silicon)
brew install cmake yt-dlp ffmpeg

# Linux (Debian/Ubuntu)
sudo apt install cmake ffmpeg
pip install yt-dlp
```

### Build from Source

```bash
git clone https://github.com/tolleybot/video-transcriber-mcp-rs.git
cd video-transcriber-mcp-rs
cargo build --release
```

The binary will be at `target/release/video-transcriber-mcp`.

## Whisper Models

Whisper models are required for transcribing local files and videos without existing captions. Models are stored in `~/.cache/video-transcriber-mcp/models/`.

### Download Models

```bash
# Download a specific model
bash scripts/download-models.sh base
bash scripts/download-models.sh medium

# Download all models
bash scripts/download-models.sh all
```

### Model Comparison

| Model | Size | Speed | Accuracy | Best For |
|-------|------|-------|----------|----------|
| **tiny** | ~75 MB | Fastest | Low | Quick drafts, testing |
| **base** | ~142 MB | Fast | Good | Default, general use |
| **small** | ~466 MB | Moderate | Better | Biggest accuracy jump from base for English |
| **medium** | ~1.5 GB | Slow | High | Accented speech, technical jargon, non-English |
| **large** | ~2.9 GB | Slowest | Highest | Best accuracy, multilingual content |

**Recommendation:** Start with **base** for testing. Use **small** or **medium** for production. The **base to small** jump gives the biggest accuracy improvement for English. **Medium** is worth it for non-English or noisy audio.

Note: For YouTube videos with existing captions, the model choice doesn't matter - captions are fetched directly and no whisper transcription is performed.

## MCP Server Registration

After building, register the MCP server:

```bash
claude mcp add video-transcriber-mcp -s user -- /path/to/target/release/video-transcriber-mcp
```

Or add manually to your MCP client config:

```json
{
  "mcpServers": {
    "video-transcriber-mcp": {
      "command": "/path/to/video-transcriber-mcp"
    }
  }
}
```

## How It Works

### YouTube Videos

1. Extract video ID from URL
2. Fetch YouTube page to get InnerTube API key
3. Call InnerTube API (as Android client) to get caption tracks
4. If captions exist: fetch caption XML, parse, and return transcript (instant)
5. If no captions: fall back to full pipeline below

### Other Videos / Local Files

1. Download video via yt-dlp (or use local file directly)
2. Extract audio via ffmpeg
3. Transcribe audio with whisper.cpp
4. Save output as TXT, JSON, and Markdown

### Output

Transcripts are saved to `~/Downloads/video-transcripts/` in three formats:

```
video-id-title.txt   # Plain text transcript
video-id-title.json  # JSON with metadata and transcript source
video-id-title.md    # Markdown with video info and transcript
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `transcribe_video` | Transcribe a video from URL or local file |
| `check_dependencies` | Check if yt-dlp, ffmpeg, and models are installed |
| `list_supported_sites` | Show supported video platforms |
| `list_transcripts` | List saved transcripts |
| `get_latest_transcript` | Get the most recent transcript |
| `delete_transcript` | Delete a transcript by video ID |
| `cleanup_old_transcripts` | Delete transcripts older than N days |
| `delete_all_transcripts` | Delete all transcripts |

## Transport Modes

### Stdio (Default)

For local use with Claude Code:

```bash
video-transcriber-mcp
```

### Streamable HTTP

For remote/team access:

```bash
video-transcriber-mcp --transport http --host 0.0.0.0 --port 8080
```

Configure clients with:

```json
{
  "mcpServers": {
    "video-transcriber-mcp": {
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

## Development

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) - Fast C++ implementation of Whisper
- [whisper-rs](https://codeberg.org/tazz4843/whisper-rs) - Rust bindings for whisper.cpp
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - Video downloader
- [Model Context Protocol](https://modelcontextprotocol.io) - MCP specification
