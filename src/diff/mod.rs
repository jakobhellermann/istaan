use std::path::Path;

use anyhow::{Context, Result};
use diffy::{DiffOptions, PatchFormatter};

pub struct DiffResult {
    pub content: String,
    pub extension: Option<&'static str>,
}
impl DiffResult {
    pub fn new(content: String, extension: &'static str) -> Self {
        DiffResult {
            content,
            extension: Some(extension),
        }
    }
    pub fn diff_ext(content: String) -> Self {
        DiffResult::new(content, "diff")
    }
}
impl From<String> for DiffResult {
    fn from(content: String) -> Self {
        DiffResult {
            content,
            extension: None,
        }
    }
}

pub fn diff(path: &Path, old: &[u8], new: &[u8]) -> Result<DiffResult> {
    let extension = path
        .extension()
        .map(|e| e.to_str().context("non-utf8 extension"))
        .transpose()?;

    if extension == Some("json") {
        return Ok(DiffResult::diff_ext(diff_json(old, new)?));
    }

    if let Some(content) = try_diff_text(&old, &new)? {
        return Ok(DiffResult::diff_ext(content));
    }

    Ok(DiffResult {
        content: "not yet implemented".into(),
        extension: None,
    })
}

fn try_diff_text(old: &[u8], new: &[u8]) -> Result<Option<String>> {
    let Ok(old) = str::from_utf8(old) else {
        return Ok(None);
    };
    let Ok(new) = str::from_utf8(new) else {
        return Ok(None);
    };

    Ok(Some(diff_text(old, new)))
}

fn diff_text(old: &str, new: &str) -> String {
    let patch = DiffOptions::new().create_patch(old, new);
    let text = PatchFormatter::new()
        .missing_newline_message(false)
        .fmt_patch(&patch)
        .to_string();
    text.lines().skip(2).collect::<Vec<_>>().join("\n")
}

fn diff_json(old: &[u8], new: &[u8]) -> Result<String> {
    let old: serde_json::Value = serde_json::from_slice(old)?;
    let new: serde_json::Value = serde_json::from_slice(new)?;

    let old_pretty = serde_json::to_string_pretty(&old)?;
    let new_pretty = serde_json::to_string_pretty(&new)?;

    Ok(diff_text(&old_pretty, &new_pretty))
}
