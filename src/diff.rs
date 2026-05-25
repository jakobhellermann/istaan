use std::path::Path;

use anstream::eprintln;
use anyhow::{Context as _, Result};
use istaan_diff_core::{DiffResult, OldNew};
use istaan_diff_json::JsonDiffOptions;
use rabex_env::Environment;
use rabex_env::game_files::GameFiles;
use rabex_env::rabex::tpk::TpkTypeTreeBlob;
use rabex_env::rabex::typetree::typetree_cache::sync::TypeTreeCache;

pub struct Context<'a> {
    pub file_filter: String,

    pub text_diff_context_size: usize,

    pub json: JsonDiffOptions,

    pub cs_decompile_assembly: bool,

    pub unity_game: Option<OldNew<Environment<GameFiles, &'a TypeTreeCache<TpkTypeTreeBlob>>>>,
    pub unity_filter: istaan_diff_unity::Filter,
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
        let cs_cx = istaan_diff_cs::Context {
            text_diff_context_size: cx.text_diff_context_size,
        };
        return istaan_diff_cs::diff_assembly(&cs_cx, data);
    }

    if extension == Some("json") {
        return Ok(DiffResult::diff_ext(istaan_diff_json::diff_json(
            &cx.json,
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
        let du_cx = build_unity_context(cx)?;
        return istaan_diff_unity::diff_serializedfile(&du_cx, path, data)
            .map(DiffResult::diff_ext)
            .context("failed to diff unity serializedfile");
    }

    if extension == Some("bundle") {
        let du_cx = build_unity_context(cx)?;
        return istaan_diff_unity::diff_bundlefile(&du_cx, path, data)
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

    Ok(DiffResult::from("binary file differs".to_string()))
}

fn build_unity_context<'a>(cx: &'a Context<'a>) -> Result<istaan_diff_unity::Context<'a>> {
    let env = cx
        .unity_game
        .as_ref()
        .context("cannot diff unity file outside unity game")?;
    Ok(istaan_diff_unity::Context {
        text_diff_context_size: cx.text_diff_context_size,
        json: cx.json.clone(),
        unity_game: OldNew::new(&env.old, &env.new),
        unity_filter: istaan_diff_unity::Filter {
            ignore_classes: cx.unity_filter.ignore_classes.clone(),
        },
    })
}

fn warn_style() -> anstyle::Style {
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)))
}

fn try_diff_text(cx: &Context, data: OldNew<&[u8]>) -> Option<String> {
    data.try_map(str::from_utf8)
        .ok()
        .map(|data| istaan_diff_core::diff_text(data, cx.text_diff_context_size))
}
