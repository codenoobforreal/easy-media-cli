use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

pub fn collect_videos<P: AsRef<Path>>(path: P, max_depth: u8) -> Vec<PathBuf> {
    let metadata = fs::symlink_metadata(&path).unwrap();

    if metadata.is_file() {
        return if is_video_file(&path) {
            vec![path.as_ref().to_path_buf()]
        } else {
            vec![]
        };
    }

    if metadata.is_dir() {
        let remaining_depth = max_depth;
        return traverse_videos(path, remaining_depth);
    }

    vec![]
}

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

fn traverse_videos<P: AsRef<Path>>(dir: P, remaining_depth: u8) -> Vec<PathBuf> {
    let entries = fs::read_dir(dir).unwrap();

    let videos = entries
        .map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = entry.metadata().unwrap();

            if metadata.is_file() && is_video_file(&path) {
                return vec![path];
            }

            if metadata.is_dir() && remaining_depth > 0 {
                return traverse_videos(&path, remaining_depth - 1);
            }

            vec![]
        })
        .flatten()
        .collect();

    videos
}
