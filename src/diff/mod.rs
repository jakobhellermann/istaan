mod unity;
use std::path::Path;

use anstream::eprintln;
use anyhow::{Context as _, Result};
use diffy::{DiffOptions, PatchFormatter};
use json_diff_ng::DiffType;
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
        return Ok(DiffResult::diff_ext(diff_json(
            data.try_map(serde_json::from_slice::<serde_json::Value>)?,
        )?));
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
    let len = data.map(str::len).max();
    let diff = len < 1024 * 32;

    if diff {
        // let context_len = usize::MAX;
        let context_len = 10;

        let patch = DiffOptions::new()
            .set_context_len(context_len)
            .create_patch(data.old, data.new);
        let text = PatchFormatter::new()
            .missing_newline_message(false)
            .fmt_patch(&patch)
            .to_string();
        text.lines().skip(2).collect::<Vec<_>>().join("\n")
    } else {
        format!("old: {}\nnew: {}", data.old, data.new)
    }
}

fn diff_json(data: OldNew<serde_json::Value>) -> Result<String> {
    use std::fmt::Write;

    let diffs = json_diff_ng::compare_serde_values(&data.old, &data.new, false, &[])?;
    let mut f = String::new();
    for (diff_type, diff_path) in diffs.all_diffs() {
        write!(&mut f, "{}: ", diff_type)?;

        for element in &diff_path.path {
            write!(&mut f, ".{element}")?;
        }
        if let Some((left, right)) = &diff_path.values {
            if left != right {
                write!(f, " {left} -> {right}")?;
            } else {
                write!(f, " {left}")?;
            }
        } else {
            let val = match diff_type {
                DiffType::LeftExtra => diff_path.resolve(&data.old),
                DiffType::RightExtra => diff_path.resolve(&data.new),
                _ => None,
            };
            if let Some(val) = val {
                write!(f, " {val}")?;
            }
        }
        f.push('\n');
    }

    Ok(f)
    // .consume(|data| format!("old: {}\nnew: {}", data.old, data.new)))
}
