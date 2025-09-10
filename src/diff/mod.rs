use std::path::Path;

use anstream::eprintln;
use anyhow::Result;
use diffy::{DiffOptions, PatchFormatter};

use crate::old_new::OldNew;

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

pub fn diff(path: &Path, data: OldNew<&[u8]>) -> Result<DiffResult> {
    if path.extension().is_some_and(|ext| ext == "json") {
        return Ok(DiffResult::diff_ext(diff_json(data)?));
    }

    if let Some(content) = try_diff_text(data) {
        return Ok(DiffResult::diff_ext(content));
    }

    let style =
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)));
    eprintln!(
        "{style}Unrecognized binary format: {}{style:#}",
        path.display()
    );

    Ok(DiffResult {
        content: "not yet implemented".into(),
        extension: None,
    })
}

fn try_diff_text(data: OldNew<&[u8]>) -> Option<String> {
    data.try_map(str::from_utf8).ok().map(diff_text)
}

fn diff_text(data: OldNew<&str>) -> String {
    let patch = DiffOptions::new().create_patch(data.old, data.new);
    let text = PatchFormatter::new()
        .missing_newline_message(false)
        .fmt_patch(&patch)
        .to_string();
    text.lines().skip(2).collect::<Vec<_>>().join("\n")
}

fn diff_json(data: OldNew<&[u8]>) -> Result<String> {
    Ok(data
        .try_map(serde_json::from_slice::<serde_json::Value>)?
        .try_map(|value| serde_json::to_string_pretty(&value))?
        .as_deref()
        .consume(diff_text))
}
