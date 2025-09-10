#![feature(str_split_whitespace_remainder, path_add_extension)]
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;

use crate::depotdownloader_manifest::Manifest;

mod depotdownloader_manifest;
mod diff;

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
            let manifest_old = app
                .manifests
                .iter()
                .find(|manifest| manifest.manifest.id == manifest_old)
                .context(format!("manifest {manifest_old} does not exist"))?;
            let manifest_new = app
                .manifests
                .iter()
                .find(|manifest| manifest.manifest.id == manifest_new)
                .context(format!("manifest {manifest_new} does not exist"))?;
            diff(manifest_old, manifest_new, Path::new("diff"))?;
        }
    }

    Ok(())
}

fn diff(old: &ManifestFiles, new: &ManifestFiles, diff_out_dir: &Path) -> Result<()> {
    // std::fs::remove_dir_all(diff_out_dir)?;

    let old_files: HashSet<_> = old.manifest.files.keys().collect();
    let new_files: HashSet<_> = new.manifest.files.keys().collect();

    let removed_files: BTreeSet<_> = old_files
        .difference(&new_files)
        .map(|x| x.as_str())
        .collect();
    let added_files: BTreeSet<_> = old_files
        .difference(&new_files)
        .map(|x| x.as_str())
        .collect();
    let kept_files: BTreeSet<_> = old_files
        .intersection(&new_files)
        .map(|x| x.as_str())
        .collect();

    if !removed_files.is_empty() {
        println!("Removed {} files:", removed_files.len());
        for file in &removed_files {
            println!("- {}", file);
        }
    }
    if !added_files.is_empty() {
        println!("Added {} files:", added_files.len());
        for file in &added_files {
            println!("- {}", file);
        }
    }

    for file in kept_files {
        let file_old = &old.manifest.files[file];
        let file_new = &new.manifest.files[file];

        if file_old.flags != file_new.flags {
            println!(
                "Changed '{file}' flags from {:b} to {:b}",
                file_old.flags, file_new.flags
            );
        }
        if file_old.sha != file_new.sha {
            println!("Changed '{file}'",);

            let mut diff_out_file = diff_out_dir.join(file);
            std::fs::create_dir_all(diff_out_file.parent().unwrap())?;

            let old_data = std::fs::read(old.path.join(file))?;
            let new_data = std::fs::read(new.path.join(file))?;
            let diff = diff::diff(Path::new(file), &old_data, &new_data)?;

            if let Some(extension) = diff.extension {
                diff_out_file.add_extension(extension);
            }
            std::fs::write(diff_out_file, diff.content)?;
        }
    }

    Ok(())
}
