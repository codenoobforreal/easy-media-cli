# easy-media-cli

[中文文档](README.md)

> A lightweight FFmpeg-powered CLI for batch video processing and scene-based thumbnail generation

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
<!--[![Crates.io](https://img.shields.io/crates/v/easy-media-cli.svg)](https://crates.io/crates/easy-media-cli)-->

## Features

- **Batch scene detection thumbnails**: Automatically extract key frames from videos based on scene changes, no manual frame selection required
- **Opinionated video encoding**: Transcode videos to AV1 using SVT-AV1 with automatic resolution scaling, framerate capping, and quality‑based CRF tuning – designed for personal archival workflows, not a general‑purpose encoder
- **Terminal real-time UI**: Built-in terminal progress interface with live task status and final execution summary

## Quick Start

### Prerequisites

This tool relies on FFmpeg's `libsvtav1` encoder for SVT‑AV1 video encoding. **Standard FFmpeg builds usually do not include this encoder**, so you must install a **full FFmpeg build that includes `libsvtav1`** and ensure the ffmpeg command is available in your system PATH.

After installation, it is recommended to verify that `libsvtav1` is available by running. If the output lists `libsvtav1`, the encoder is ready.
```bash
ffmpeg -h encoder=libsvtav1
```

### Installation

<!--#### Install via Cargo | 通过 Cargo 安装
```bash
cargo install easy-media-cli
```-->

#### Download prebuilt binaries

Prebuilt binaries for Windows, macOS, and Linux are available on the [Releases page](https://github.com/codenoobforreal/easy-media-cli/releases).  

Download the asset matching your platform, rename it to `easy-media-cli` (optional but convenient for following the examples), and place it in a directory that is on your system `PATH` (e.g. `/usr/local/bin` on macOS/Linux).  

On macOS and Linux, you may need to make the file executable:  
 ```bash
 chmod +x /path/to/easy-media-cli
 ```

<!--#### Install via Cargo | 通过 Cargo 安装
```bash
cargo install easy-media-cli
```-->

#### Build from source

```bash
git clone https://github.com/codenoobforreal/easy-media-cli.git
cd easy-media-cli
cargo build --release
```

## Usage

The current release includes two subcommands: `scs` (scene‑snap thumbnail generator) and `ve` (personal SVT‑AV1 video encoder).

### Basic syntax

```bash
easy-media-cli -h
easy-media-cli scs -h
easy-media-cli ev -h

easy-media-cli scs [OPTIONS] --input <INPUT>
easy-media-cli ev [OPTIONS] --input <INPUT>
```

### Examples

#### Scene‑snap thumbnails

1. Generate thumbnails for a single video
```bash
easy-media-cli scs -i demo.mp4
```

2. Batch process a directory with custom sensitivity
```bash
easy-media-cli scs -i ./videos -t 0.5 -o ./thumbnails
```

3. Recursive scan with fixed output width
```bash
easy-media-cli scs -i ./media -w 480 -d 3
```

#### Video encoding (SVTAV1)

1. Encode a single video, cap resolution to 720p and framerate to 24
```bash
easy-media-cli encode -i demo.mp4 -r 1280x720 -f 24
```

2. Batch encode a directory, output to a custom folder
```bash
easy-media-cli encode -i ./raw_videos -o ./encoded -r 1920x1080
```

## Development

### Build
```bash
cargo build
```

### Run tests
```bash
cargo test
```

## License

This project is licensed under the MIT License.
