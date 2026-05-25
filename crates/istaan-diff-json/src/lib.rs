use std::fmt::Write;

use anyhow::Result;
use istaan_diff_core::OldNew;
use json_diff_ng::DiffType;
use regex::Regex;

#[derive(Debug, Default, Clone)]
pub struct JsonDiffOptions {
    pub ignore_regex: Option<Regex>,
    /// Ignore new values of `0`, `[]`, etc.
    pub ignore_new_default: bool,
    pub sort: bool,
}

pub fn diff_json(options: &JsonDiffOptions, data: OldNew<&serde_json::Value>) -> Result<String> {
    let diffs = json_diff_ng::compare_serde_values(
        data.old,
        data.new,
        options.sort,
        options.ignore_regex.clone().as_slice(),
    )?;
    let mut f = String::new();

    let all_diffs = diffs.all_diffs();
    let all_mismatch = all_diffs
        .iter()
        .all(|(diff_type, _)| matches!(diff_type, DiffType::Mismatch));
    for (diff_type, diff_path) in all_diffs {
        if options.ignore_new_default
            && let DiffType::RightExtra = diff_type
        {
            match diff_path.resolve(data.new) {
                Some(new_value) if is_json_default(new_value) => continue,
                Some(_) | None => {
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

pub fn is_json_default(new_value: &serde_json::Value) -> bool {
    match new_value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(bool) => !*bool,
        serde_json::Value::Number(number) => number.as_u64() == Some(0),
        serde_json::Value::String(str) => str.is_empty(),
        serde_json::Value::Array(arr) => arr.is_empty(),
        serde_json::Value::Object(map) => map.is_empty() || map.values().all(is_json_default),
    }
}
