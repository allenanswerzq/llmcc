//! Source file handling.
//! Source file handling.
use std::fs::File as StdFile;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct FileId {
    pub path: Option<String>,
    content: Arc<[u8]>,
    pub content_hash: u64,
}

impl FileId {
    pub fn new_path(path: String) -> std::io::Result<Self> {
        let mut file = StdFile::open(&path)?;
        let capacity = file.metadata().map(|meta| meta.len() as usize).unwrap_or(0);
        let mut content = Vec::with_capacity(capacity);
        file.read_to_end(&mut content)?;

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = hasher.finish();

        Ok(FileId {
            path: Some(path),
            content: Arc::from(content),
            content_hash,
        })
    }

    /// Create a FileId by reading from `physical_path` but storing `logical_path`.
    /// This is useful when files have prefixes (like "000_") that should be stripped
    /// for downstream processing while still reading the actual file from disk.
    pub fn new_path_with_logical(
        physical_path: &str,
        logical_path: String,
    ) -> std::io::Result<Self> {
        let mut file = StdFile::open(physical_path)?;
        let capacity = file.metadata().map(|meta| meta.len() as usize).unwrap_or(0);
        let mut content = Vec::with_capacity(capacity);
        file.read_to_end(&mut content)?;

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = hasher.finish();

        Ok(FileId {
            path: Some(logical_path),
            content: Arc::from(content),
            content_hash,
        })
    }

    pub fn new_content(content: Vec<u8>) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(&content);
        let content_hash = hasher.finish();

        FileId {
            path: None,
            content: Arc::from(content),
            content_hash,
        }
    }

    pub fn content(&self) -> &[u8] {
        self.content.as_ref()
    }

    pub fn get_text(&self, start_byte: usize, end_byte: usize) -> Option<String> {
        let content_bytes = self.content();

        if start_byte > end_byte
            || start_byte > content_bytes.len()
            || end_byte > content_bytes.len()
        {
            return None;
        }

        let slice = &content_bytes[start_byte..end_byte];
        Some(String::from_utf8_lossy(slice).into_owned())
    }

    pub fn get_full_text(&self) -> Option<String> {
        let content_bytes = self.content();
        Some(String::from_utf8_lossy(content_bytes).into_owned())
    }
}

#[derive(Debug, Clone, Default)]
pub struct File {
    // TODO: add cache and all other stuff
    pub file: FileId,
}

impl File {
    pub fn new_source(source: Vec<u8>) -> Self {
        File {
            file: FileId::new_content(source),
        }
    }

    pub fn new_file(file: String) -> std::io::Result<Self> {
        Ok(File {
            file: FileId::new_path(file)?,
        })
    }

    /// Create a File by reading from `physical_path` but storing `logical_path`.
    pub fn new_file_with_logical(
        physical_path: &str,
        logical_path: String,
    ) -> std::io::Result<Self> {
        Ok(File {
            file: FileId::new_path_with_logical(physical_path, logical_path)?,
        })
    }

    pub fn content(&self) -> &[u8] {
        self.file.content()
    }

    pub fn get_text(&self, start: usize, end: usize) -> String {
        self.file.get_text(start, end).unwrap()
    }

    pub fn opt_get_text(&self, start: usize, end: usize) -> Option<String> {
        self.file.get_text(start, end)
    }

    pub fn path(&self) -> Option<&str> {
        self.file.path.as_deref()
    }
}
