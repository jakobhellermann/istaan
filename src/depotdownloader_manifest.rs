use anyhow::{Context, Result, ensure};
use jiff::civil::DateTime;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Manifest {
    pub id: String,
    pub date: DateTime,
    pub files: BTreeMap<String, ManifestFile>,
}
#[derive(Debug)]
pub struct ManifestFile {
    #[allow(dead_code)]
    pub size: usize,
    pub flags: u32,
    pub sha: String,
}

impl Manifest {
    pub fn parse(manifest: &str) -> Result<Manifest> {
        let mut lines = manifest.lines();
        lines.next();
        lines.next();
        let date = lines.next().context("invalid manifest header")?;
        let (key, value) = date.split_once(": ").context("invalid manifest header")?;
        ensure!(key.trim() == "Manifest ID / date");
        let (id, date) = value
            .trim()
            .split_once(" / ")
            .context("invalid manifest header")?;

        while let Some(line) = lines.next()
            && !line.trim_start().starts_with("Size Chunks File SHA")
        {}

        let mut files = BTreeMap::new();
        for line in lines {
            let mut items = line.split_ascii_whitespace();
            let size = items.next().unwrap().parse()?;
            let _chunks = items.next().unwrap();
            let sha = items.next().unwrap().to_owned();
            let flags: u32 = items.next().unwrap().parse()?;
            let path = items.remainder().unwrap();
            files.insert(path.to_owned(), ManifestFile { size, flags, sha });
        }

        let date = DateTime::strptime("%m/%d/%Y %H:%M:%S", date)?;

        Ok(Manifest {
            id: id.to_owned(),
            date,
            files,
        })
    }
}
