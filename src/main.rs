#![feature(str_split_whitespace_remainder, path_add_extension)]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, ensure};
use clap::Parser;
use rabex_env::Environment;
use rabex_env::rabex::tpk::TpkTypeTreeBlob;
use rabex_env::rabex::typetree::typetree_cache::sync::TypeTreeCache;

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
    app_id: u32,
    manifests: Vec<ManifestFiles>,
}
struct ManifestFiles {
    path: PathBuf,
    manifest: Manifest,
}

fn load() -> Result<App> {
    let mut app = None;
    for item in std::fs::read_dir("data/depots")? {
        let item = item?;
        if !item.file_type()?.is_dir() {
            continue;
        }

        let item = item.path();
        let Some(app_id) = item
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };

        ensure!(!app.is_some(), "Multiple app ids found in data folder");
        app = Some((app_id, item));
    }
    let (app_id, app_path) = app.context("no app data found in data folder")?;

    let mut manifests = Vec::new();
    for manifest in std::fs::read_dir(&app_path)? {
        let manifest_dir = manifest?.path();
        let manifest_path = find_single_file_of_extension(&manifest_dir, "txt")?;
        let manifest = std::fs::read_to_string(manifest_path)?;
        let manifest = Manifest::parse(&manifest)?;
        manifests.push(ManifestFiles {
            path: manifest_dir,
            manifest,
        });
    }

    Ok(App { app_id, manifests })
}

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Parser)]
enum Command {
    Diff {
        manifest_old: String,
        manifest_new: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let app = load()?;

    match cli.command {
        None => {
            println!("AppId: {}", app.app_id);
            for manifest in &app.manifests {
                println!("- {} ({})", manifest.manifest.id, manifest.manifest.date);
            }
        }
        Some(Command::Diff {
            manifest_old,
            manifest_new,
        }) => {
            let manifest = OldNew::new(manifest_old, manifest_new);
            let files = manifest.try_map(|id| {
                app.manifests
                    .iter()
                    .find(|m| m.manifest.id == id)
                    .context(format!("manifest {} does not exist", id))
            })?;
            diff(files, Path::new("diff"))?;
        }
    }

    Ok(())
}

fn diff(manifest_files: OldNew<&ManifestFiles>, diff_out_dir: &Path) -> Result<()> {
    // std::fs::remove_dir_all(diff_out_dir)?;

    let tpk = TypeTreeCache::new(TpkTypeTreeBlob::embedded());
    let unity_game = manifest_files
        .try_map(|files| Environment::new_in(&files.path, &tpk))
        .ok();

    let cx = Context { unity_game };

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

    for path in file_changes.same {
        let manifest_file = manifest_files.map(|x| &x.manifest.files[path]);

        if manifest_file.map(|file| file.flags).changed() {
            println!(
                "Changed '{path}' flags from {:b} to {:b}",
                manifest_file.old.flags, manifest_file.new.flags
            );
        }
        if manifest_file.map(|file| &file.sha).changed() {
            println!("Changed '{path}'",);

            let mut diff_out_file = diff_out_dir.join(path);
            std::fs::create_dir_all(diff_out_file.parent().unwrap())?;

            let data = manifest_files.try_map(|f| std::fs::read(f.path.join(path)))?;
            let diff = diff::diff(&cx, Path::new(path), data.as_deref())?;

            if let Some(extension) = diff.extension {
                diff_out_file.add_extension(extension);
            }
            std::fs::write(diff_out_file, diff.content)?;
        }
    }

    Ok(())
}
