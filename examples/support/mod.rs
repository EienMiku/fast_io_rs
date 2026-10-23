use std::{
    fs::{File, OpenOptions},
    io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

// Own only a newly created temporary file; never truncate a pre-existing path.
pub struct ScratchFile {
    file: Option<File>,
    path: PathBuf,
}
impl ScratchFile {
    pub fn new(label: &str) -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fast-io-bench-{}-{stamp}-{label}.tmp",
            std::process::id()
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            file: Some(file),
            path,
        })
    }
    pub fn file(&mut self) -> &mut File {
        self.file.as_mut().unwrap()
    }
}
impl Drop for ScratchFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
