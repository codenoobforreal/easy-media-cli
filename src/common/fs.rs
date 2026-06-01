use std::{
    fs::{self, DirEntry, Metadata},
    io::{self},
    path::Path,
};

pub trait FileSystem {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>>;
}

#[derive(Default)]
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::symlink_metadata(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        fs::read_dir(path)?.collect()
    }
}
