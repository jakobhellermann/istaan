use std::path::PathBuf;

pub struct DiffResult {
    pub content: String,
    pub extension: Option<&'static str>,
    pub children: Vec<(PathBuf, DiffResult)>,
}
impl DiffResult {
    pub fn new_with_ext(content: String, extension: &'static str) -> Self {
        DiffResult {
            content,
            extension: Some(extension),
            children: Vec::new(),
        }
    }
    pub fn diff_ext(content: String) -> Self {
        DiffResult::new_with_ext(content, "diff")
    }
    pub fn with_children(mut self, children: Vec<(PathBuf, DiffResult)>) -> Self {
        self.children = children;
        self
    }
}
impl From<String> for DiffResult {
    fn from(content: String) -> Self {
        DiffResult {
            content,
            extension: None,
            children: Vec::new(),
        }
    }
}
