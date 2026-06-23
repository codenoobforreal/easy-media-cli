use crate::infra::{FileSystem, FileType};
use std::{
    io,
    path::{Path, PathBuf},
};

/// 递归收集指定路径下的所有视频文件
/// # 参数
/// - `path`：起始路径（文件/目录）
/// - `max_depth`：最大递归深度
///   - `Some(0)`：仅遍历当前目录，不进入子目录
///   - `Some(n)`：最多递归 n 层子目录
///   - `None`：不限制递归深度，完整遍历所有层级
/// # 返回值
/// 成功时返回视频文件路径列表；失败时返回 IO 错误
pub fn collect_videos(
    fs: &dyn FileSystem,
    path: impl AsRef<Path>,
    max_depth: Option<u8>,
) -> io::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let meta = fs.symlink_metadata(path)?;
    match meta {
        FileType::File | FileType::Symlink => Ok(if is_video_file(path) {
            vec![path.to_path_buf()]
        } else {
            vec![]
        }),
        FileType::Dir => traverse_videos(fs, path, max_depth),
    }
}

/// 检查文件是否为视频文件（根据其文件扩展名）
fn is_video_file<P: AsRef<Path>>(path: P) -> bool {
    const EXTS: &[&str] = &[
        "3gp", "avi", "flv", "m4v", "mov", "mpg", "mts", "ogv", "ts", "f4v", "m2v", "mp4", "webm",
        "wmv", "rmvb", "mkv",
    ];

    path.as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|s| EXTS.contains(&s.to_lowercase().as_str()))
}

fn traverse_videos(
    fs: &dyn FileSystem,
    dir: &Path,
    remaining_depth: Option<u8>,
) -> io::Result<Vec<PathBuf>> {
    let entries = fs.read_dir(dir)?;
    let mut videos = vec![];

    for entry in entries {
        let meta = fs.symlink_metadata(&entry)?;
        match meta {
            FileType::File | FileType::Symlink => {
                if is_video_file(&entry) {
                    videos.push(entry);
                }
            }
            FileType::Dir if remaining_depth != Some(0) => {
                let next_depth = remaining_depth.map(|d| d - 1);
                videos.extend(traverse_videos(fs, &entry, next_depth)?);
            }
            FileType::Dir => {}
        }
    }

    Ok(videos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::test_utils::MockFileSystem;
    use insta::assert_debug_snapshot;
    use std::io::ErrorKind;

    #[test]
    fn collect_videos_propagates_io_error_for_nonexistent_path() {
        let fs = MockFileSystem::default();
        fs.set_metadata(
            "non_exist",
            Err(std::io::Error::new(ErrorKind::NotFound, "Not found")),
        );
        let result = collect_videos(&fs, Path::new("non_exist"), Some(0));
        assert_debug_snapshot!(result, @r#"
            Err(
                Custom {
                    kind: NotFound,
                    error: "Not found",
                },
            )
            "#);
    }

    #[test]
    fn collect_videos_returns_single_file_when_input_is_video() {
        let fs = MockFileSystem::default();
        fs.set_metadata("video.mp4", Ok(FileType::File));
        let result = collect_videos(&fs, "video.mp4", Some(0)).unwrap();
        assert_eq!(result, vec![PathBuf::from("video.mp4")]);
    }

    #[test]
    fn collect_videos_returns_empty_for_non_video_file() {
        let fs = MockFileSystem::default();
        fs.set_metadata("note.txt", Ok(FileType::File));
        let result = collect_videos(&fs, "note.txt", Some(0)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn collect_videos_respects_max_depth_zero_boundary() {
        let fs = MockFileSystem::default();
        fs.set_metadata(".", Ok(FileType::Dir));
        fs.set_dir_entries(
            ".",
            Ok(vec![
                PathBuf::from("video1.mp4"),
                PathBuf::from("subdir"),
                PathBuf::from("doc.pdf"),
            ]),
        );
        fs.set_metadata("video1.mp4", Ok(FileType::File));
        fs.set_metadata("subdir", Ok(FileType::Dir));
        fs.set_metadata("doc.pdf", Ok(FileType::File));
        let result = collect_videos(&fs, ".", Some(0)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], PathBuf::from("video1.mp4"));
    }

    #[test]
    fn collect_videos_recurses_correctly_within_max_depth() {
        let fs = MockFileSystem::default();
        fs.set_metadata(".", Ok(FileType::Dir));
        fs.set_metadata("video1.mp4", Ok(FileType::File));
        fs.set_metadata("sub", Ok(FileType::Dir));
        fs.set_metadata("sub/sub_video.webm", Ok(FileType::File));
        fs.set_metadata("sub/deep", Ok(FileType::Dir));
        fs.set_metadata("sub/deep/deep_video.mkv", Ok(FileType::File));
        fs.set_dir_entries(
            ".",
            Ok(vec![PathBuf::from("video1.mp4"), PathBuf::from("sub")]),
        );
        fs.set_dir_entries(
            "sub",
            Ok(vec![
                PathBuf::from("sub/sub_video.webm"),
                PathBuf::from("sub/deep"),
            ]),
        );
        fs.set_dir_entries(
            "sub/deep",
            Ok(vec![PathBuf::from("sub/deep/deep_video.mkv")]),
        );
        let depth1 = collect_videos(&fs, ".", Some(1)).unwrap();
        assert_eq!(depth1.len(), 2);
        let depth2 = collect_videos(&fs, ".", Some(2)).unwrap();
        assert_eq!(depth2.len(), 3);
    }

    #[test]
    fn collect_videos_unlimited_depth_when_none() {
        let fs = MockFileSystem::default();
        fs.set_metadata(".", Ok(FileType::Dir));
        fs.set_metadata("video1.mp4", Ok(FileType::File));
        fs.set_metadata("sub", Ok(FileType::Dir));
        fs.set_metadata("sub/sub_video.webm", Ok(FileType::File));
        fs.set_metadata("sub/deep", Ok(FileType::Dir));
        fs.set_metadata("sub/deep/deep_video.mkv", Ok(FileType::File));
        fs.set_metadata("sub/deep/deeper", Ok(FileType::Dir));
        fs.set_metadata("sub/deep/deeper/ultra_video.mkv", Ok(FileType::File));
        fs.set_dir_entries(
            ".",
            Ok(vec![PathBuf::from("video1.mp4"), PathBuf::from("sub")]),
        );
        fs.set_dir_entries(
            "sub",
            Ok(vec![
                PathBuf::from("sub/sub_video.webm"),
                PathBuf::from("sub/deep"),
            ]),
        );
        fs.set_dir_entries(
            "sub/deep",
            Ok(vec![
                PathBuf::from("sub/deep/deep_video.mkv"),
                PathBuf::from("sub/deep/deeper"),
            ]),
        );
        fs.set_dir_entries(
            "sub/deep/deeper",
            Ok(vec![PathBuf::from("sub/deep/deeper/ultra_video.mkv")]),
        );
        let unlimited = collect_videos(&fs, ".", None).unwrap();
        assert_eq!(unlimited.len(), 4);
        let limited = collect_videos(&fs, ".", Some(2)).unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[test]
    fn collect_videos_handles_symlink_video_file() {
        let fs = MockFileSystem::default();
        fs.set_metadata("link_to_video.mp4", Ok(FileType::Symlink));
        let result = collect_videos(&fs, "link_to_video.mp4", Some(0)).unwrap();
        assert_eq!(result, vec![PathBuf::from("link_to_video.mp4")]);
    }

    #[test]
    fn collect_videos_empty_directory_returns_empty() {
        let fs = MockFileSystem::default();
        fs.set_metadata("empty_dir", Ok(FileType::Dir));
        fs.set_dir_entries("empty_dir", Ok(vec![]));
        let result = collect_videos(&fs, "empty_dir", Some(0)).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn is_video_file_case_insensitive_extension() {
        assert!(is_video_file("video.MP4"));
        assert!(is_video_file("video.MKV"));
        assert!(is_video_file("video.WebM"));
    }
}
