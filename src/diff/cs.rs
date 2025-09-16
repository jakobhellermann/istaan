use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::Command;

use crate::diff::{Context, DiffResult};
use crate::old_new::OldNew;
use anyhow::{Context as _, Result, ensure};
use tempfile::{NamedTempFile, TempDir};
use walkdir::WalkDir;

pub fn diff_assembly(cx: &Context, data: OldNew<&[u8]>) -> Result<DiffResult> {
    let decomp = data.try_map(|data| -> Result<_> {
        let mut file = NamedTempFile::new()?;
        file.write_all(data)?;

        let dir = TempDir::new()?;
        let output = Command::new("ilspycmd")
            .arg(file.path())
            .arg("-p")
            .arg("-o")
            .arg(dir.path())
            .output()
            .context("failed to run ilspycmd")?;
        ensure!(output.status.success(), "failed to run ilspycmd");

        let mut all_files = BTreeSet::default();

        for item in WalkDir::new(&dir) {
            let item = item?;
            if item.file_type().is_dir() {
                continue;
            }
            if item
                .path()
                .file_name()
                .is_some_and(|name| name == ".csproj")
            {
                continue;
            }
            all_files.insert(item.path().strip_prefix(&dir).unwrap().to_owned());
        }

        Ok((dir, all_files))
    })?;
    let decomp = decomp.as_ref();

    let changes = decomp.changes(|(_, all_files)| all_files.iter());
    let mut text = String::new();
    for added in &changes.added {
        writeln!(&mut text, "Added {}", added.display())?;
    }
    for added in &changes.removed {
        writeln!(&mut text, "Removed {}", added.display())?;
    }

    let mut children = Vec::new();

    for file in changes.same {
        let source = decomp.try_map(|(dir, _)| std::fs::read_to_string(dir.as_ref().join(file)))?;
        let diff = super::diff_text(cx, source.as_deref());
        if !diff.is_empty() {
            writeln!(&mut text, "Changed: {}", file.display())?;
            children.push((file.clone(), DiffResult::new_with_ext(diff, "diff")));
        }
    }

    Ok(DiffResult::diff_ext(text).with_children(children))
}
