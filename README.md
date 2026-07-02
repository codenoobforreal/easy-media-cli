# easy-media-cli

[English README](README_en.md)

> 一款基于 FFmpeg 的轻量级命令行工具，用于批量视频处理与场景检测缩略图生成

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
<!--[![Crates.io](https://img.shields.io/crates/v/easy-media-cli.svg)](https://crates.io/crates/easy-media-cli)-->

## 核心特性

- **批量场景检测缩略图**：基于视频画面变化自动提取关键帧，无需手动挑选时间点
- **视频编码**：通过 SVT-AV1 进行 AV1 编码，自动处理分辨率缩放、帧率上限和 CRF 质量控制 — 仅服务于个人归档流程，非通用编码器
- **终端实时 UI**：内置终端进度交互界面，实时展示任务状态与执行结果汇总

## 快速开始

### 前置依赖

本工具依赖 FFmpeg 的 `libsvtav1` 编码器来实现 SVT‑AV1 视频编码。**标准 FFmpeg 构建通常不包含该编码器**，因此您必须安装 **包含 `libsvtav1` 的 FFmpeg 完整版（full build）**，并确保 ffmpeg 命令可在系统环境变量 PATH 中调用。

安装后，建议运行以下命令验证 `libsvtav1` 是否可用，若输出包含 `libsvtav1`，则表示编码器已就绪。
```bash
ffmpeg -h encoder=libsvtav1
```

### 安装

<!--#### Install via Cargo | 通过 Cargo 安装
```bash
cargo install easy-media-cli
```-->

#### 下载预编译版本

预编译的二进制文件可在 [Releases 页面](https://github.com/codenoobforreal/easy-media-cli/releases) 获取，支持 Windows、macOS 和 Linux。

下载对应平台的资产文件，重命名为 `easy-media-cli`（可选，便于直接沿用后续示例命令），并将其放入系统 `PATH` 中的目录，例如 macOS/Linux 的 `/usr/local/bin`。

在 macOS 和 Linux 上，可能需要赋予可执行权限：
```bash
chmod +x /path/to/easy-media-cli
```

<!--#### Install via Cargo | 通过 Cargo 安装
```bash
cargo install easy-media-cli
```-->

#### 源码编译

```bash
git clone https://github.com/codenoobforreal/easy-media-cli.git
cd easy-media-cli
cargo build --release
```

## 使用说明

目前提供两个子命令：`scs`（场景检测缩略图生成）与 `ev`（个人 SVT‑AV1 视频编码）。

### 基础语法

```bash
easy-media-cli -h
easy-media-cli scs -h
easy-media-cli ev -h

easy-media-cli scs [OPTIONS] --input <INPUT>
easy-media-cli ev [OPTIONS] --input <INPUT>
```

### 使用示例

#### 场景缩略图

1. 为单个视频生成缩略图
```bash
easy-media-cli scs -i demo.mp4
```

2. 批量处理目录，自定义场景敏感度
```bash
easy-media-cli scs -i ./videos -t 0.5 -o ./thumbnails
```

3. 递归扫描目录，生成指定宽度的缩略图
```bash
easy-media-cli scs -i ./media -w 480 -d 3
```

#### 视频编码 (SVTAV1)

1. 编码单个视频，限制分辨率至 720p 且帧率不超过 24
```bash
easy-media-cli encode -i demo.mp4 -r 1280x720 -f 24
```

2. 批量编码目录，输出到指定文件夹
```bash
easy-media-cli encode -i ./raw_videos -o ./encoded -r 1920x1080
```

## 开发指南

### 构建项目

```bash
cargo build
```

### 运行测试

```bash
cargo test
```

## 许可证

本项目基于 MIT 许可证开源。
