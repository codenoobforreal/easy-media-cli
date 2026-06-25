use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use easy_media_cli::{
    common::collect_videos,
    infra::{FileType, MockFileSystem},
};
use std::{
    hint::black_box,
    path::{Path, PathBuf},
};

/// 构建一个扁平目录场景：
/// - `file_count` 个文件，其中 `video_ratio` 比例是视频（.mp4）
#[allow(clippy::cast_precision_loss)]
fn build_flat_scenario(fs: &MockFileSystem, file_count: usize, video_ratio: f64) {
    let root = PathBuf::from("/root");
    let mut entries: Vec<PathBuf> = Vec::new();
    for i in 0..file_count {
        let is_video = (i as f64) < (file_count as f64) * video_ratio;
        let ext = if is_video { "mp4" } else { "txt" };
        let name = format!("file_{i}.{ext}");
        let path = root.join(&name);
        entries.push(path.clone());
        fs.set_metadata(&path, Ok(FileType::File));
    }
    fs.set_metadata(&root, Ok(FileType::Dir));
    fs.set_dir_entries(&root, Ok(entries));
}

/// 构建深层嵌套场景：
/// - `depth` 层目录，每层有 `files_per_dir` 个文件（50% 视频）
/// - 最后没有子目录
fn build_deep_scenario(fs: &MockFileSystem, depth: u8, files_per_dir: usize) {
    let root = PathBuf::from("/root");
    fs.set_metadata(&root, Ok(FileType::Dir));

    let mut current = root.clone();
    for d in 0..=depth {
        let mut entries: Vec<PathBuf> = Vec::new();
        // 添加文件
        for i in 0..files_per_dir {
            let is_video = i % 2 == 0;
            let ext = if is_video { "avi" } else { "log" };
            let name = format!("f_{i}.{ext}");
            let path = current.join(&name);
            entries.push(path.clone());
            fs.set_metadata(&path, Ok(FileType::File));
        }
        // 如果不是最深层，加一个子目录
        if d < depth {
            let sub = current.join("sub");
            entries.push(sub.clone());
            fs.set_metadata(&sub, Ok(FileType::Dir));
            // 注册当前目录的条目
            fs.set_dir_entries(&current, Ok(entries));
            current = sub;
        } else {
            // 最深层，仅有文件
            fs.set_dir_entries(&current, Ok(entries));
        }
    }
}

fn bench_collect_videos(c: &mut Criterion) {
    let mut group = c.benchmark_group("collect_videos");

    // 场景1：大扁平目录，1000个文件，50% 视频
    let fs1 = MockFileSystem::default();
    build_flat_scenario(&fs1, 1000, 0.5);
    group.bench_with_input(
        BenchmarkId::new("flat_1000_50pct", "None"),
        &fs1,
        |b, fs| {
            b.iter(|| {
                let videos = collect_videos(fs, black_box(Path::new("/root")), None).unwrap();
                black_box(videos);
            });
        },
    );

    // 场景2：无视频文件，500个文件
    let fs2 = MockFileSystem::default();
    build_flat_scenario(&fs2, 500, 0.0);
    group.bench_with_input(
        BenchmarkId::new("flat_500_no_video", "None"),
        &fs2,
        |b, fs| {
            b.iter(|| {
                let videos = collect_videos(fs, black_box(Path::new("/root")), None).unwrap();
                black_box(videos);
            });
        },
    );

    // 场景3：深层嵌套，深度5，每层20文件
    let fs3 = MockFileSystem::default();
    build_deep_scenario(&fs3, 5, 20);
    group.bench_with_input(
        BenchmarkId::new("deep_5x20_50pct", "None"),
        &fs3,
        |b, fs| {
            b.iter(|| {
                let videos = collect_videos(fs, black_box(Path::new("/root")), None).unwrap();
                black_box(videos);
            });
        },
    );

    // 场景4：深度限制（只扫描顶层）
    let fs4 = MockFileSystem::default();
    build_flat_scenario(&fs4, 200, 0.3);
    group.bench_with_input(
        BenchmarkId::new("flat_200_depth_0", "Some(0)"),
        &fs4,
        |b, fs| {
            b.iter(|| {
                let videos = collect_videos(fs, black_box(Path::new("/root")), Some(0)).unwrap();
                black_box(videos);
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_collect_videos);
criterion_main!(benches);
