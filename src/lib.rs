#![feature(str_split_whitespace_remainder)]
mod depotdownloader_manifest;
mod diff;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result, ensure};
use istaan_diff_core::OldNew;
use rabex::objects::ClassId;
use rabex::tpk::TpkTypeTreeBlob;
use rabex::typetree::typetree_cache::sync::TypeTreeCache;
use rabex_env::Environment;
use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};
use regex::Regex;

pub use crate::depotdownloader_manifest::Manifest;
pub use crate::diff::Context;

pub struct ManifestFiles {
    pub path: PathBuf,
    pub manifest: Manifest,
}

pub fn diff(manifest_files: OldNew<&ManifestFiles>, diff_out_dir: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(diff_out_dir);
    std::fs::create_dir_all(diff_out_dir)?;

    // Separate TypeTreeCache per side
    let tpk = OldNew::new(
        TypeTreeCache::new(TpkTypeTreeBlob::embedded()),
        TypeTreeCache::new(TpkTypeTreeBlob::embedded()),
    );
    let unity_game = manifest_files
        .try_map_zip(&tpk, |files, tpk| Environment::new_in(&files.path, tpk))
        .ok();

    let cx = Context {
        // file_filter: "dataassets".into(),
        // file_filter: ".dll".into(),
        file_filter: "".into(),
        text_diff_context_size: 6,

        json: istaan_diff_json::JsonDiffOptions {
            ignore_regex: Some(Regex::new("m_PreloadTable|preloadIndex|m_glyphInfoList").unwrap()),
            ignore_new_default: true,
            sort: false,
        },

        cs_decompile_assembly: true,

        unity_game,
        unity_filter: istaan_diff_unity::Filter {
            ignore_classes: HashSet::from_iter([
                ClassId::Texture2D,
                ClassId::Sprite,
                ClassId::SpriteAtlas,
                ClassId::SpriteRenderer,
                ClassId::AudioSource,
                ClassId::CircleCollider2D,
                ClassId::HingeJoint2D,
            ]),
        },
    };

    let file_changes = manifest_files.changes(|files| files.manifest.files.keys());

    if !file_changes.removed.is_empty() {
        println!("Removed {} files:", file_changes.removed.len());
        for file in &file_changes.removed {
            println!("- {}", file);
        }
    }
    if !file_changes.added.is_empty() {
        println!("Added {} files:", file_changes.added.len());
        for file in &file_changes.added {
            println!("- {}", file);
        }
    }

    file_changes
        .same
        .into_par_iter()
        .map(|path| {
            let manifest_file = manifest_files.map(|x| &x.manifest.files[path]);

            if !path.contains(&cx.file_filter) {
                return Ok(());
            }

            if manifest_file.map(|file| file.flags).changed() {
                println!(
                    "Changed '{path}' flags from {:b} to {:b}",
                    manifest_file.old.flags, manifest_file.new.flags
                );
            }
            if manifest_file.map(|file| &file.sha).changed() {
                let start = Instant::now();

                let diff_out_file = diff_out_dir.join(path);

                let data = manifest_files.try_map(|f| std::fs::read(f.path.join(path)))?;
                let diff = diff::diff(&cx, Path::new(path), data.as_deref())?;

                if !diff.content.is_empty() {
                    let mut out_file = diff_out_file.clone();
                    if let Some(extension) = diff.extension {
                        out_file.add_extension(extension);
                    }
                    std::fs::create_dir_all(diff_out_file.parent().unwrap())?;
                    std::fs::write(&out_file, &diff.content)?;
                    println!("Changed '{path}' ({}ms)", start.elapsed().as_millis());
                }
                for (child_path, child) in &diff.children {
                    ensure!(
                        diff.extension.is_some(),
                        "Internal error: Can't have diff with children and no extension"
                    );
                    let mut out_file = diff_out_file.join(child_path);
                    if let Some(extension) = child.extension {
                        out_file.add_extension(extension);
                    }
                    std::fs::create_dir_all(out_file.parent().unwrap())?;
                    std::fs::write(&out_file, &child.content)
                        .with_context(|| format!("Failed to save diff {}", out_file.display()))?;
                }
            }

            Ok(())
        })
        .collect::<Result<()>>()?;

    Ok(())
}
