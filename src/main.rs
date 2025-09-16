#![feature(str_split_whitespace_remainder, path_add_extension)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use clap::Parser;
use rabex::objects::ClassId;
use rabex_env::Environment;
use rabex_env::rabex::tpk::TpkTypeTreeBlob;
use rabex_env::rabex::typetree::typetree_cache::sync::TypeTreeCache;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use regex::Regex;

use crate::depotdownloader_manifest::Manifest;
use crate::diff::Context;
use crate::old_new::OldNew;

mod depotdownloader_manifest;
mod diff;
mod old_new;

pub fn find_single_file_of_extension(folder: &Path, extension: &str) -> Result<PathBuf> {
    let entries = std::fs::read_dir(folder)?;

    let mut manifest_files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();

    match manifest_files.len() {
        1 => Ok(manifest_files.pop().unwrap()),
        0 => Err(anyhow!("No .{extension} file found in {:?}", folder)),
        _ => Err(anyhow!("Multiple .{extension} files found in {:?}", folder)),
    }
}

struct App {
    manifests: Vec<ManifestFiles>,
}
struct ManifestFiles {
    path: PathBuf,
    manifest: Manifest,
}

fn load() -> Result<App> {
    let mut manifests = Vec::new();
    for manifest in std::fs::read_dir("data/manifests")? {
        let manifest_dir = manifest?.path();
        let manifest_path = find_single_file_of_extension(&manifest_dir, "txt")?;
        let manifest = std::fs::read_to_string(manifest_path)?;
        let manifest = Manifest::parse(&manifest)?;
        manifests.push(ManifestFiles {
            path: manifest_dir,
            manifest,
        });
    }
    manifests.sort_by_key(|a| a.manifest.date);

    Ok(App { manifests })
}

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Parser)]
enum Command {
    Diff {
        #[clap(long, short, default_value = "diff")]
        out_dir: PathBuf,
        manifest_old: Option<String>,
        manifest_new: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let app = load()?;

    match cli.command {
        None => {
            for manifest in &app.manifests {
                println!(
                    "- {} ({})",
                    manifest.manifest.id,
                    manifest.manifest.date.date(),
                );
            }
        }
        Some(Command::Diff {
            out_dir,
            manifest_old,
            manifest_new,
        }) => {
            let manifest = match (manifest_old, manifest_new) {
                (Some(old), Some(new)) => OldNew::new(old, new),
                (Some(new), None) => {
                    let new_index = app
                        .manifests
                        .iter()
                        .position(|m| m.manifest.id == new)
                        .context(format!("manifest {} does not exist", new))?;
                    ensure!(
                        new_index >= 1,
                        "Cannot compute diff for first manifest version"
                    );
                    let old_index = new_index - 1;
                    let old = app.manifests[old_index].manifest.id.clone();
                    OldNew::new(old, new)
                }
                (None, None) => match app.manifests.as_slice() {
                    [] => bail!("No downloaded manifests found"),
                    [_] => bail!("Only one downloaded manifest found"),
                    [.., old, new] => OldNew::new(old, new).map(|m| m.manifest.id.clone()),
                },
                (None, Some(_)) => unreachable!(),
            };

            let files = manifest.try_map(|id| {
                app.manifests
                    .iter()
                    .find(|m| m.manifest.id == id)
                    .context(format!("manifest {} does not exist", id))
            })?;
            let out_dir = out_dir.join(format!(
                "{} to {}",
                files.old.manifest.date.date(),
                files.new.manifest.date.date()
            ));

            println!("Diffing {} -> {}", files.old.manifest, files.new.manifest);

            let start = Instant::now();
            diff(files, &out_dir).context("Failed to generate diff")?;
            println!("Diffed all files in {:?}", start.elapsed())
        }
    }

    Ok(())
}

fn diff(manifest_files: OldNew<&ManifestFiles>, diff_out_dir: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(diff_out_dir);
    std::fs::create_dir_all(diff_out_dir)?;

    let tpk = TypeTreeCache::new(TpkTypeTreeBlob::embedded());
    let unity_game = manifest_files
        .try_map(|files| Environment::new_in(&files.path, &tpk))
        .ok();

    let cx = Context {
        // file_filter: "dataassets".into(),
        file_filter: "".into(),

        text_diff_context_size: 6,

        json_ignore_regex: Some(Regex::new("m_PreloadTable").unwrap()),
        json_ignore_new_default: true,
        json_sort: false,

        cs_decompile_assembly: true,

        unity_game,
        unity_filter: diff::unity::Filter {
            ignore_classes: HashSet::from_iter([
                ClassId::Texture2D,
                ClassId::Sprite,
                ClassId::SpriteAtlas,
                ClassId::SpriteRenderer,
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

                let mut diff_out_file = diff_out_dir.join(path);
                std::fs::create_dir_all(diff_out_file.parent().unwrap())?;

                let data = manifest_files.try_map(|f| std::fs::read(f.path.join(path)))?;
                let diff = diff::diff(&cx, Path::new(path), data.as_deref())?;

                if !diff.content.is_empty() {
                    if let Some(extension) = diff.extension {
                        diff_out_file.add_extension(extension);
                    }
                    std::fs::write(&diff_out_file, &diff.content)?;
                    println!("Changed '{path}' ({}ms)", start.elapsed().as_millis());
                }
            }

            Ok(())
        })
        .collect::<Result<()>>()?;

    Ok(())
}
