use std::{
    collections::HashMap,
    fmt, fs, io, mem,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub trait FileSystem: Send + Sync + fmt::Debug {
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

/// 由于 read_progress bench 中也是用到了这个 mock，我们直接暴露该结构体
#[derive(Default)]
pub struct MockFileSystem {
    state: Mutex<FsState>,
}

impl fmt::Debug for MockFileSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mutex<FsState>")
    }
}

struct FsState {
    created_dirs: Vec<PathBuf>,
    create_dir_result: io::Result<()>, // 默认 Ok
    metadata: HashMap<PathBuf, io::Result<FileType>>,
    dir_entries: HashMap<PathBuf, io::Result<Vec<PathBuf>>>,
}

impl Default for FsState {
    fn default() -> Self {
        Self {
            created_dirs: vec![],
            create_dir_result: Ok(()),
            metadata: HashMap::new(),
            dir_entries: HashMap::new(),
        }
    }
}

impl MockFileSystem {
    pub fn set_metadata(&self, path: impl Into<PathBuf>, result: io::Result<FileType>) {
        self.state
            .lock()
            .unwrap()
            .metadata
            .insert(path.into(), result);
    }

    pub fn set_dir_entries(&self, dir: impl Into<PathBuf>, result: io::Result<Vec<PathBuf>>) {
        self.state
            .lock()
            .unwrap()
            .dir_entries
            .insert(dir.into(), result);
    }

    /// 设置 `create_dir_all` 的失败结果（只生效一次，之后恢复 Ok）
    pub fn set_create_dir_err(&self, kind: io::ErrorKind, msg: &'static str) {
        let mut s = self.state.lock().unwrap();
        s.create_dir_result = Err(io::Error::new(kind, msg));
    }

    /// 暴露已创建的目录列表（测试断言用）
    pub fn created_dirs(&self) -> Vec<PathBuf> {
        self.state.lock().unwrap().created_dirs.clone()
    }
}

impl FileSystem for MockFileSystem {
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileType> {
        let s = self.state.lock().unwrap();
        match s.metadata.get(path) {
            Some(Ok(ft)) => Ok(ft.clone()), // FileType 可 Copy
            Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Path not found: {}", path.display()),
            )),
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let s = self.state.lock().unwrap();
        match s.dir_entries.get(path) {
            Some(Ok(entries)) => Ok(entries.clone()),
            Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Directory not found: {}", path.display()),
            )),
        }
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let mut s = self.state.lock().unwrap();
        s.created_dirs.push(path.to_path_buf());
        // 取出当前结果，如果为 Err，则消费一次并恢复为 Ok
        mem::replace(&mut s.create_dir_result, Ok(()))
    }

    fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Ok(())
    }
}
