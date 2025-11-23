use std::path::PathBuf;
use tempfile::TempDir;

/// Captures tracing output for tests.
#[allow(dead_code)]
pub struct TestTracing {
    buffer: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

#[allow(dead_code)]
impl TestTracing {
    pub fn new() -> Self {
        Self {
            buffer: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn install(&self) -> tracing::subscriber::DefaultGuard {
        let writer = self.buffer.clone();
        let make_writer = move || TestWriter(writer.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(make_writer)
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    pub fn output(&self) -> String {
        let buf = self.buffer.lock().unwrap();
        String::from_utf8_lossy(&buf).to_string()
    }
}

#[allow(dead_code)]
pub struct EnvGuard {
    key: String,
    prev: Option<String>,
}

#[allow(dead_code)]
impl EnvGuard {
    pub fn set(key: &str, val: impl AsRef<str>) -> Self {
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, val.as_ref()) };
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe { std::env::set_var(&self.key, v) },
            None => unsafe { std::env::remove_var(&self.key) },
        }
    }
}

struct TestWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut guard = self.0.lock().unwrap();
        guard.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[allow(dead_code)]
pub struct TempFixtureDir {
    pub dir: TempDir,
}

#[allow(dead_code)]
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
