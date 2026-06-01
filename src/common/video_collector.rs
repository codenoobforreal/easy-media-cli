use crate::common::fs::{FileSystem, RealFileSystem};
use anyhow::Result;
use std::path::{Path, PathBuf};

fn is_video_file<P: AsRef<Path>>(path: P) -> bool {
    let ext = path
        .as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "3gp" | "avi" | "flv" | "m4v" | "mov" | "mpg" | "mts" | "ogv" | "ts" | "f4v" | "m2v"
        | "mp4" | "webm" | "wmv" | "rmvb" => true,
        _ => false,
    }
}

pub fn collect_videos(path: &Path, max_depth: u8) -> Result<Vec<PathBuf>> {
    collect_videos_with_fs(&RealFileSystem::default(), path, max_depth)
}

fn collect_videos_with_fs<FS: FileSystem>(
    fs: &FS,
    path: &Path,
    max_depth: u8,
) -> Result<Vec<PathBuf>> {
    let metadata = fs.symlink_metadata(path)?;

    if metadata.is_file() {
        return if is_video_file(path) {
            Ok(vec![path.to_path_buf()])
        } else {
            Ok(vec![])
        };
    }

    if metadata.is_dir() {
        let remaining_depth = max_depth;
        return traverse_videos_with_fs(fs, path, remaining_depth);
    }

    Ok(vec![])
}

fn traverse_videos_with_fs<FS: FileSystem>(
    fs: &FS,
    dir: &Path,
    remaining_depth: u8,
) -> Result<Vec<PathBuf>> {
    let entries = fs.read_dir(dir)?;

    let videos = entries
        .into_iter()
        .map(|entry| -> Result<Vec<PathBuf>> {
            let path = entry.path();
            let metadata = entry.metadata()?;

            if metadata.is_file() && is_video_file(&path) {
                return Ok(vec![path]);
            }

            if metadata.is_dir() && remaining_depth > 0 {
                return traverse_videos_with_fs(fs, &path, remaining_depth - 1);
            }

            Ok(vec![])
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(videos)
}
