use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub trait FileSystem: Send + Sync {
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileType>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    #[allow(dead_code)]
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// `std::fs::Metadata` 的一个简约业务抽象，可解决测试场景中的“可构造性”问题：
/// - 通过将业务逻辑所需的“文件类型”信息提取到自定义枚举中，FileSystem 的模拟版本可以灵活地返回任何类型的元数据。这使得对 `collect_videos` 等上级逻辑进行纯粹的单元测试成为可能，且完全独立于真实的文件系统
/// - 这将业务逻辑与操作系统的实现解耦：如果将来需要扩展元数据字段（例如文件大小、修改时间），只需扩展枚举类型即可，无需对上层的业务逻辑进行任何修改
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
    Symlink,
}

impl FileType {
    // pub fn is_file(&self) -> bool {
    //     matches!(self, FileType::File)
    // }

    // pub fn is_dir(&self) -> bool {
    //     matches!(self, FileType::Dir)
    // }

    // pub fn is_symlink(&self) -> bool {
    //     matches!(self, FileType::Symlink)
    // }
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultFileSystem;

impl FileSystem for DefaultFileSystem {
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileType> {
        let meta = fs::symlink_metadata(path)?;
        if meta.is_file() {
            Ok(FileType::File)
        } else if meta.is_dir() {
            Ok(FileType::Dir)
        } else {
            Ok(FileType::Symlink)
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        Ok(entries)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};
    use tempfile::tempdir;

    #[derive(Debug, Default)]
    pub struct MockFileSystem {
        pub created_dirs: Mutex<Vec<PathBuf>>,
        create_dir_result: Mutex<Option<io::Result<()>>>,
        metadata: Mutex<HashMap<PathBuf, io::Result<FileType>>>,
        dir_entries: Mutex<HashMap<PathBuf, io::Result<Vec<PathBuf>>>>,
    }

    impl MockFileSystem {
        /// 设置指定路径的元数据返回值
        pub fn set_metadata(&self, path: impl Into<PathBuf>, result: io::Result<FileType>) {
            self.metadata.lock().unwrap().insert(path.into(), result);
        }

        /// 设置指定目录的 `read_dir` 返回条目
        pub fn set_dir_entries(&self, dir: impl Into<PathBuf>, result: io::Result<Vec<PathBuf>>) {
            self.dir_entries.lock().unwrap().insert(dir.into(), result);
        }

        /// 设置 `create_dir_all` 的错误返回
        pub fn set_create_dir_err(&self, kind: io::ErrorKind, msg: &'static str) {
            *self.create_dir_result.lock().unwrap() = Some(Err(io::Error::new(kind, msg)));
        }
    }

    impl FileSystem for MockFileSystem {
        fn symlink_metadata(&self, path: &Path) -> io::Result<FileType> {
            let map = self.metadata.lock().unwrap();
            match map.get(path) {
                Some(Ok(ft)) => Ok(ft.clone()),
                Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Path not found: {}", path.display()),
                )),
            }
        }

        fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
            let map = self.dir_entries.lock().unwrap();
            match map.get(path) {
                Some(Ok(entries)) => Ok(entries.clone()),
                Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Directory not found: {}", path.display()),
                )),
            }
        }

        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            self.created_dirs.lock().unwrap().push(path.to_path_buf());
            self.create_dir_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(()))
        }

        fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn create_dir_all_creates_nested_dirs() {
        let base = tempdir().unwrap();
        let target = base.path().join("a/b/c");
        let fs = DefaultFileSystem;
        fs.create_dir_all(&target).unwrap();
        assert!(target.is_dir());
    }

    #[test]
    fn symlink_metadata_distinguishes_file_and_dir() {
        let base = tempdir().unwrap();
        let file_path = base.path().join("test.txt");
        fs::write(&file_path, b"hello").unwrap();
        let fs = DefaultFileSystem;
        assert_eq!(fs.symlink_metadata(&file_path).unwrap(), FileType::File);
        assert_eq!(fs.symlink_metadata(base.path()).unwrap(), FileType::Dir);
    }

    #[test]
    fn read_dir_returns_all_entries() {
        let base = tempdir().unwrap();
        fs::write(base.path().join("a.txt"), b"").unwrap();
        fs::write(base.path().join("b.txt"), b"").unwrap();
        fs::create_dir_all(base.path().join("subdir")).unwrap();
        let fs = DefaultFileSystem;
        let entries = fs.read_dir(base.path()).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn rename_moves_file_successfully() {
        let base = tempdir().unwrap();
        let from = base.path().join("old.txt");
        let to = base.path().join("new.txt");
        fs::write(&from, b"content").unwrap();
        let fs = DefaultFileSystem;
        fs.rename(&from, &to).unwrap();
        assert!(!from.exists());
        assert!(to.exists());
    }

    #[test]
    fn metadata_missing_path_returns_error() {
        let fs = DefaultFileSystem;
        let err = fs
            .symlink_metadata(Path::new("/nonexistent/path/12345"))
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn mock_fs_tracks_created_dirs() {
        let fs = MockFileSystem::default();
        let path = PathBuf::from("/test/output");
        fs.create_dir_all(&path).unwrap();
        assert_eq!(fs.created_dirs.lock().unwrap().len(), 1);
        assert_eq!(fs.created_dirs.lock().unwrap()[0], path);
    }

    #[test]
    fn mock_fs_returns_configured_error() {
        let fs = MockFileSystem::default();
        fs.set_create_dir_err(io::ErrorKind::PermissionDenied, "Permission denied");
        let err = fs.create_dir_all(Path::new("/test")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
