pub mod cs;
pub mod unity;

use std::path::{Path, PathBuf};

use anstream::eprintln;
use anyhow::{Context as _, Result};
use diffy::{DiffOptions, PatchFormatter};
use json_diff_ng::DiffType;
use rabex_env::Environment;
use rabex_env::game_files::GameFiles;
use rabex_env::rabex::tpk::TpkTypeTreeBlob;
use rabex_env::rabex::typetree::typetree_cache::sync::TypeTreeCache;
use regex::Regex;

use crate::old_new::OldNew;

pub struct Context<'a> {
    pub file_filter: String,

    pub text_diff_context_size: usize,

    pub json_ignore_regex: Option<Regex>,
    /// Ignore new values of `0`, `[]`, etc.
    pub json_ignore_new_default: bool,
    pub json_sort: bool,

    pub cs_decompile_assembly: bool,

    pub unity_game: Option<OldNew<Environment<GameFiles, &'a TypeTreeCache<TpkTypeTreeBlob>>>>,
    pub unity_filter: unity::Filter,
}

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

    if extension == Some("dll") && cx.cs_decompile_assembly {
        let diff = cs::diff_assembly(cx, data)?;
        return Ok(diff);
    }

    if extension == Some("json") {
        return Ok(DiffResult::diff_ext(diff_json(
            cx,
            data.try_map(serde_json::from_slice::<serde_json::Value>)?
                .as_ref(),
        )?));
    }

    if extension == Some("assets")
        || ["globalgamemanagers", "unity_default_resources"].contains(&file_name)
        || file_name
            .strip_prefix("level")
            .is_some_and(|i| i.parse::<usize>().is_ok())
    {
        return unity::diff_serializedfile(cx, path, data)
            .map(DiffResult::diff_ext)
            .context("failed to diff unity serializedfile");
    }

    if extension == Some("bundle") {
        return unity::diff_bundlefile(cx, path, data)
            .map(DiffResult::diff_ext)
            .context("failed to diff unity bundlefile");
    }

    if let Some(content) = try_diff_text(cx, data) {
        return Ok(DiffResult::diff_ext(content));
    }

    let style = warn_style();
    eprintln!(
        "{style}Unrecognized binary format: {}{style:#}",
        path.display()
    );

    Ok(DiffResult {
        content: "binary file differs".into(),
        extension: None,
        children: Vec::new(),
    })
}

fn warn_style() -> anstyle::Style {
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)))
}

fn try_diff_text(cx: &Context, data: OldNew<&[u8]>) -> Option<String> {
    data.try_map(str::from_utf8)
        .ok()
        .map(|data| diff_text(cx, data))
}

fn diff_text(cx: &Context, data: OldNew<&str>) -> String {
    diff_text_context(data, cx.text_diff_context_size)
}

fn diff_text_context(data: OldNew<&str>, context_len: usize) -> String {
    let len = data.map(str::len).max();
    let threshold = 1024 * 1024;
    let diff = len < threshold;

    if diff {
        // let context_len = usize::MAX;

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

fn diff_json(cx: &Context, data: OldNew<&serde_json::Value>) -> Result<String> {
    use std::fmt::Write;

    let diffs = json_diff_ng::compare_serde_values(
        data.old,
        data.new,
        cx.json_sort,
        cx.json_ignore_regex.clone().as_slice(),
    )?;
    let mut f = String::new();

    let all_diffs = diffs.all_diffs();
    let all_mismatch = all_diffs
        .iter()
        .all(|(diff_type, _)| matches!(diff_type, DiffType::Mismatch));
    for (diff_type, diff_path) in all_diffs {
        if cx.json_ignore_new_default
            && let DiffType::RightExtra = diff_type
        {
            match diff_path.resolve(data.new) {
                Some(new_value) => {
                    if is_json_default(new_value) {
                        continue;
                    }
                }
                None => {
                    /*let style = warn_style();
                    eprintln!(
                        "{style}Couldn't look up path '{}' {style:#}",
                        diff_path
                            .path
                            .iter()
                            .map(|x| format!(".{x}"))
                            .collect::<String>()
                    );*/
                }
            }
        }

        if !f.is_empty() {
            f.push('\n');
        }

        let diff_type_msg = match diff_type {
            DiffType::RootMismatch => "Mismatch at root.",
            DiffType::LeftExtra => "< ",
            DiffType::RightExtra => "> ",
            DiffType::Mismatch if all_mismatch => "",
            DiffType::Mismatch => "  ",
        };
        write!(&mut f, "{}", diff_type_msg)?;

        for element in &diff_path.path {
            write!(&mut f, ".{element}")?;
        }
        if let Some((left, right)) = diff_path.values {
            if left != right {
                write!(f, " {left} -> {right}")?;
            } else {
                write!(f, " {left}")?;
            }
        } else {
            let val = match diff_type {
                DiffType::LeftExtra => diff_path.resolve(data.old),
                DiffType::RightExtra => diff_path.resolve(data.new),
                _ => None,
            };
            if let Some(val) = val {
                write!(f, " {}", serde_json::to_string_pretty(val)?)?;
            }
        }
    }

    Ok(f)
    // .consume(|data| format!("old: {}\nnew: {}", data.old, data.new)))
}

fn is_json_default(new_value: &serde_json::Value) -> bool {
    match new_value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(bool) => !*bool,
        serde_json::Value::Number(number) => number.as_u64() == Some(0),
        serde_json::Value::String(str) => str.is_empty(),
        serde_json::Value::Array(arr) => arr.is_empty(),
        serde_json::Value::Object(map) => map.is_empty() || map.values().all(is_json_default),
    }
}
