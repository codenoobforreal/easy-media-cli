# easy-media-cli

> A lightweight FFmpeg-powered CLI for batch video processing and scene-based thumbnail generation
>
> 一款基于 FFmpeg 的轻量级命令行工具，用于批量视频处理与场景检测缩略图生成

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
<!--[![Crates.io](https://img.shields.io/crates/v/easy-media-cli.svg)](https://crates.io/crates/easy-media-cli)-->

---

## ✨ Features | 核心特性
- **Batch scene detection thumbnails**: Automatically extract key frames from videos based on scene changes, no manual frame selection required
- 批量场景检测缩略图：基于视频画面变化自动提取关键帧，无需手动挑选时间点
- **Terminal real-time UI**: Built-in terminal progress interface with live task status and final execution summary
- 终端实时 UI：内置终端进度交互界面，实时展示任务状态与执行结果汇总

---

## 🚀 Quick Start | 快速开始

### Prerequisites | 前置依赖
FFmpeg must be installed and available in your system `PATH`.
> 需提前安装 FFmpeg 并确保可在系统环境变量 PATH 中调用。

### Installation | 安装

<!--#### Install via Cargo | 通过 Cargo 安装
```bash
cargo install easy-media-cli
```-->

#### Build from source | 源码编译
```bash
git clone https://github.com/codenoobforreal/easy-media-cli.git
cd easy-media-cli
cargo build --release
```

---

## 📖 Usage | 使用说明
The core subcommand currently provided is `scs` (Scene detection thumbnail generator).
> 当前核心提供 `scs` 子命令（场景检测缩略图生成器）。

### Basic syntax | 基础语法
```bash
easy-media-cli -h
easy-media-cli scs -h
easy-media-cli scs [OPTIONS] --input <INPUT>
```

### Examples | 使用示例
1. **Generate thumbnails for a single video**
   > 为单个视频生成缩略图
   ```bash
   easy-media scs -i demo.mp4
   ```

2. **Batch process a directory with custom sensitivity**
   > 批量处理目录，自定义场景敏感度
   ```bash
   easy-media scs -i ./videos -t 5 -o ./thumbnails
   ```
3. **Recursive scan with fixed output width**
   > 递归扫描目录，生成指定宽度的缩略图
   ```bash
   easy-media scs -i ./media -w 480 -d 3
   ```

---

## 🛠️ Development | 开发指南

### Build | 构建项目
```bash
cargo build
```

### Run tests | 运行测试
```bash
cargo test
```

---

## 📄 License | 许可证

This project is licensed under the MIT License.
> 本项目基于 MIT 许可证开源。
