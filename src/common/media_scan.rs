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
    let mut videos = Vec::new();

    match meta {
        FileType::File | FileType::Symlink => {
            if is_video_file(path) {
                videos.push(path.to_path_buf());
            }
        }
        FileType::Dir => {
            traverse_videos(fs, path, max_depth, &mut videos)?;
        }
    }

    Ok(videos)
}

fn traverse_videos(
    fs: &dyn FileSystem,
    root: &Path,
    max_depth: Option<u8>,
    videos: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let mut stack = vec![(root.to_path_buf(), max_depth)];

    while let Some((dir, depth)) = stack.pop() {
        for entry in fs.read_dir(&dir)? {
            let meta = fs.symlink_metadata(&entry)?;
            match meta {
                FileType::File | FileType::Symlink => {
                    if is_video_file(&entry) {
                        videos.push(entry);
                    }
                }
                FileType::Dir if depth != Some(0) => {
                    let next_depth = depth.map(|d| d - 1);
                    stack.push((entry, next_depth));
                }
                FileType::Dir => {}
            }
        }
    }
    Ok(())
}

// const EXTS: &[&str] = &[
//     "dv", "ts", "qt", "rm", "3gp", "3g2", "avi", "asf", "dvd", "dat", "f4v", "flv", "m4v", "mov",
//     "mpg", "mod", "mts", "m2v", "mp4", "wmv", "mkv", "mts", "ogv", "ogg", "oga", "vob", "m2ts",
//     "mpeg", "webm", "rmvb",
// ];

/// 检查文件是否为视频文件（根据其文件扩展名）
pub fn is_video_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().extension().is_some_and(|ext| {
        let bytes = ext.as_encoded_bytes();
        let len = bytes.len();
        if len > 32 {
            return false;
        }
        let mut lower = [0u8; 32]; // 合理的扩展名上限长度
        for (i, &b) in bytes.iter().enumerate() {
            lower[i] = b.to_ascii_lowercase();
        }
        // 按字母顺序排序，后续新增可直接添加
        matches!(&lower[..len], |b"3gp"| b"3g2"
            | b"asf"
            | b"avi"
            | b"dv"
            | b"f4v"
            | b"flv"
            | b"m2ts"
            | b"m2v"
            | b"m4v"
            | b"mkv"
            | b"mod"
            | b"mov"
            | b"mp4"
            | b"mpeg"
            | b"mpg"
            | b"mts"
            | b"ogv"
            | b"qt"
            | b"rm"
            | b"rmvb"
            | b"ts"
            | b"vob"
            | b"webm"
            | b"wmv")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::MockFileSystem;

    #[test]
    fn test_is_video_file() {
        assert!(is_video_file("video.mp4"));
        assert!(is_video_file("video.MKV"));
        assert!(is_video_file("video.mov"));
        assert!(is_video_file("video.avi"));
        assert!(!is_video_file("video.txt"));
        assert!(!is_video_file("video.jpg"));
        assert!(!is_video_file("video.mp4.extra"));
        assert!(!is_video_file("no_extension"));
        assert!(!is_video_file("video.mp4.bak"));
    }

    #[test]
    fn test_collect_videos_file() {
        let fs = MockFileSystem::default();
        let path = PathBuf::from("video.mp4");
        fs.set_metadata(&path, Ok(FileType::File));
        let result = collect_videos(&fs, &path, None).unwrap();
        assert_eq!(result, vec![path]);
    }

    #[test]
    fn test_collect_videos_dir_no_recursion() {
        let fs = MockFileSystem::default();
        let root = PathBuf::from("/root");
        let file1 = root.join("a.mp4");
        let file2 = root.join("b.mkv");
        let subdir = root.join("sub");

        fs.set_metadata(&root, Ok(FileType::Dir));
        fs.set_dir_entries(
            &root,
            Ok(vec![file1.clone(), file2.clone(), subdir.clone()]),
        );
        fs.set_metadata(&file1, Ok(FileType::File));
        fs.set_metadata(&file2, Ok(FileType::File));
        fs.set_metadata(&subdir, Ok(FileType::Dir));
        let result = collect_videos(&fs, &root, Some(0)).unwrap();
        assert_eq!(result, vec![file1, file2]);
    }

    #[test]
    fn test_collect_videos_dir_with_depth() {
        let fs = MockFileSystem::default();
        let root = PathBuf::from("/root");
        let sub1 = root.join("sub1");
        let sub2 = sub1.join("sub2");
        let file1 = root.join("a.mp4");
        let file2 = sub1.join("b.mkv");
        let file3 = sub2.join("c.mp4");

        fs.set_metadata(&root, Ok(FileType::Dir));
        fs.set_dir_entries(&root, Ok(vec![file1.clone(), sub1.clone()]));
        fs.set_metadata(&file1, Ok(FileType::File));
        fs.set_metadata(&sub1, Ok(FileType::Dir));

        fs.set_dir_entries(&sub1, Ok(vec![file2.clone(), sub2.clone()]));
        fs.set_metadata(&file2, Ok(FileType::File));
        fs.set_metadata(&sub2, Ok(FileType::Dir));

        fs.set_dir_entries(&sub2, Ok(vec![file3.clone()]));
        fs.set_metadata(&file3, Ok(FileType::File));

        let result = collect_videos(&fs, &root, Some(1)).unwrap();
        assert_eq!(result, vec![file1.clone(), file2.clone()]);

        let result = collect_videos(&fs, &root, Some(2)).unwrap();
        assert_eq!(result, vec![file1.clone(), file2.clone(), file3]);
    }

    #[test]
    fn test_collect_videos_no_video() {
        let fs = MockFileSystem::default();
        let root = PathBuf::from("/root");
        fs.set_dir_entries(&root, Ok(vec![]));
        fs.set_metadata(&root, Ok(FileType::Dir));
        let result = collect_videos(&fs, &root, Some(0));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
