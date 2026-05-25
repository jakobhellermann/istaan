use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use clap::Parser;

use istaan::{Manifest, ManifestFiles};
use istaan_diff_core::OldNew;

pub fn find_single_file_of_extension(folder: &Path, extension: &str) -> Result<PathBuf> {
    let entries = std::fs::read_dir(folder)
        .with_context(|| format!("Failed to read directory {}", folder.display()))?;

    let mut manifest_files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();

    match manifest_files.len() {
        1 => Ok(manifest_files.pop().unwrap()),
        0 => Err(anyhow!(
            "No .{extension} file found in {}",
            folder.display()
        )),
        _ => Err(anyhow!(
            "Multiple .{extension} files found in {}",
            folder.display()
        )),
    }
}

struct App {
    manifests: Vec<ManifestFiles>,
}

fn load() -> Result<App> {
    let manifests_root = Path::new("data/manifests");
    let mut manifests = Vec::new();
    let entries = std::fs::read_dir(manifests_root).with_context(|| {
        format!(
            "Failed to read manifests directory {}",
            manifests_root.display()
        )
    })?;
    for manifest in entries {
        let manifest_dir = manifest
            .with_context(|| format!("Failed to read entry in {}", manifests_root.display()))?
            .path();
        let manifest_path =
            find_single_file_of_extension(&manifest_dir, "txt").with_context(|| {
                format!(
                    "Failed to locate manifest .txt in {}",
                    manifest_dir.display()
                )
            })?;
        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest file {}", manifest_path.display()))?;
        let manifest = Manifest::parse(&manifest_content)
            .with_context(|| format!("Failed to parse manifest {}", manifest_path.display()))?;
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
            istaan::diff(files, &out_dir).context("Failed to generate diff")?;
            println!("Diffed all files in {:?}", start.elapsed())
        }
    }

    Ok(())
}
