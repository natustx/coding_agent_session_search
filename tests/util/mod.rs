use std::path::PathBuf;
use tempfile::TempDir;

pub struct TempFixtureDir {
    pub dir: TempDir,
}

impl TempFixtureDir {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().expect("tempdir"),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }
}
