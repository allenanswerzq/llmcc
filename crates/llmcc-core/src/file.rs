use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug, Clone, Default)]
pub struct FileId {
    pub path: Option<String>,
    pub content: Option<Vec<u8>>,
    pub content_hash: u64,
}

impl FileId {
    pub fn new_path(path: String) -> std::io::Result<Self> {
        let content = std::fs::read(&path)?;

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = hasher.finish();

        Ok(FileId {
            path: Some(path),
            content: Some(content),
            content_hash,
        })
    }

    pub fn new_content(content: Vec<u8>) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(&content);
        let content_hash = hasher.finish();

        FileId {
            path: None,
            content: Some(content),
            content_hash,
        }
    }

    pub fn get_text(&self, start_byte: usize, end_byte: usize) -> Option<String> {
        let content_bytes = self.content.as_ref()?;

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
        let content_bytes = self.content.as_ref()?;
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

    pub fn content(&self) -> Vec<u8> {
        self.file.content.as_ref().unwrap().to_vec()
    }

    pub fn get_text(&self, start: usize, end: usize) -> String {
        self.file.get_text(start, end).unwrap()
    }

    pub fn opt_get_text(&self, start: usize, end: usize) -> Option<String> {
        self.file.get_text(start, end)
    }
}
