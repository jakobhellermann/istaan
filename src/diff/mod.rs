mod unity;
use std::path::Path;

use anstream::eprintln;
use anyhow::{Context as _, Result};
use diffy::{DiffOptions, PatchFormatter};
use rabex_env::{
    Environment,
    game_files::GameFiles,
    rabex::{tpk::TpkTypeTreeBlob, typetree::typetree_cache::sync::TypeTreeCache},
};

use crate::old_new::OldNew;

pub struct Context<'a> {
    pub unity_game: Option<OldNew<Environment<GameFiles, &'a TypeTreeCache<TpkTypeTreeBlob>>>>,
}

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

pub fn diff(cx: &Context, path: &Path, data: OldNew<&[u8]>) -> Result<DiffResult> {
    let file_name = path
        .file_name()
        .context("file has no filename")?
        .to_str()
        .context("non-utf8 filename")?;
    let extension = path
        .extension()
        .map(|e| e.to_str().context("non-utf8 extension"))
        .transpose()?;

    if extension == Some("json") {
        return Ok(DiffResult::diff_ext(diff_json(data)?));
    }

    if extension == Some("assets") || file_name == "globalgamemanagers" {
        return unity::diff_serializedfile(cx, path, data)
            .map(DiffResult::diff_ext)
            .context("failed to diff unity serializedfile");
    }

    if extension == Some("bundle") {
        return unity::diff_bundlefile(cx, path, data)
            .map(DiffResult::diff_ext)
            .context("failed to diff unity bundlefile");
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
    let context_len = 3;
    let patch = DiffOptions::new()
        .set_context_len(context_len)
        .create_patch(data.old, data.new);
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
