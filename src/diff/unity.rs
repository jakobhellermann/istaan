use std::fmt::Write;
use std::io::Cursor;
use std::path::Path;

use anstream::eprintln;
use anstyle::{Color, Style};
use anyhow::{Context as _, Result};
use rabex::files::bundlefile::{BundleFileReader, ExtractionConfig};
use rabex_env::{handle::SerializedFileHandle, rabex::files::SerializedFile};

use crate::old_new::OldNew;

use super::Context;

pub fn diff_serializedfile(cx: &Context, path: &Path, data: OldNew<&[u8]>) -> Result<String> {
    let env = cx
        .unity_game
        .as_ref()
        .context("cannot diff bundlefile outside unity game")?;

    let old_reader = &mut Cursor::new(data.old);
    let new_reader = &mut Cursor::new(data.new);
    let old = SerializedFile::from_reader(old_reader)?;
    let new = SerializedFile::from_reader(new_reader)?;

    let old = SerializedFileHandle::new(&env.old, &old, data.old);
    let new = SerializedFileHandle::new(&env.new, &new, data.new);

    let file = OldNew::new(old, new);
    let old = &file.old;
    let new = &file.new;

    let mut text = super::diff_text(OldNew::new(
        &format!("{:#?}", format::SerializedFile::from(old.file)),
        &format!("{:#?}", format::SerializedFile::from(new.file)),
    ));

    let object_changes = file
        .as_ref()
        .changes(|file| file.file.objects().map(|x| x.m_PathID));

    text.push_str("\n\n");
    writeln!(
        &mut text,
        "Removed {} objects",
        object_changes.removed.len()
    )?;
    writeln!(&mut text, "Added {} objects", object_changes.added.len())?;

    for added in object_changes.added {
        let new_object = new.object_at::<serde_json::Value>(added)?;
        let new_value = new_object.read()?;
        writeln!(
            &mut text,
            "--- added object {:?} {added} ---\n{}",
            new_object.object.info.m_ClassID,
            serde_json::to_string_pretty(&new_value)?
        )?;
    }

    for path_id in object_changes.same {
        let old_object = old.object_at::<serde_json::Value>(path_id)?;
        let new_object = new.object_at::<serde_json::Value>(path_id)?;

        let old_data = old_object.object.get_raw_data(&mut old.reader())?;
        let new_data = new_object.object.get_raw_data(&mut new.reader())?;

        if old_data != new_data {
            writeln!(&mut text, "--- change object at path id {path_id} ---",)?;

            if let Err(e) = (|| -> Result<()> {
                // let old_value = old_object.read()?;
                // let new_value = new_object.read()?;
                let old_value = old_object.object.read(&mut old_object.file.reader())?;
                let new_value = new_object.object.read(&mut new_object.file.reader())?;

                let name = new_value
                    .get("m_Name")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned);

                let value = OldNew::new(old_value, new_value);

                let diff = super::diff_json(value)?;
                write!(&mut text, "> {:?}", new_object.class_id())?;
                let script = old_object.mono_script()?;
                if let Some(script) = script {
                    write!(&mut text, " {}", script.full_name())?;
                }
                if let Some(name) = name {
                    write!(&mut text, " ({})", name)?;
                }

                if old_object.class_id() != new_object.class_id() {
                    writeln!(&mut text, " (previously {:?})", old_object.class_id())?;
                    return Ok(());
                }

                writeln!(&mut text)?;
                writeln!(&mut text, "{}", diff)?;

                /*writeln!(
                    &mut text,
                    "> old {:?} {}",
                    old_object.info.m_ClassID, formatted.old
                )?;
                writeln!(
                    &mut text,
                    "> new {:?} {}",
                    new_object.info.m_ClassID, formatted.new
                )?;*/
                Ok(())
            })() {
                let style = Style::new().fg_color(Some(Color::Ansi(anstyle::AnsiColor::Red)));
                eprintln!(
                    "{style}Skipping {:?} object in {} (Path ID {}): {e:?}{style:#}",
                    new_object.class_id(),
                    path.display(),
                    path_id,
                );
                writeln!(&mut text, "{}", e)?;
            }
        }
    }

    Ok(text)
}

pub fn diff_bundlefile(cx: &Context, path: &Path, data: OldNew<&[u8]>) -> Result<String> {
    let env = cx
        .unity_game
        .as_ref()
        .context("cannot diff bundlefile outside unity game")?;

    let bundle = data.try_map_zip(env, |data, env| -> Result<_> {
        let config = ExtractionConfig::new(None, Some(env.unity_version()?));
        let bundle = BundleFileReader::from_reader(Cursor::new(data), &config)?;
        Ok(bundle)
    })?;

    let changes = bundle
        .as_ref()
        .changes(|bundle| bundle.files().iter().map(|file| file.path.as_str()));

    let mut text = format!(
        "Removed: {:?}\nAdded: {:?}\nPossibly Modified: {:?}\n",
        changes.removed, changes.added, changes.same,
    );
    for &bundle_path in &changes.same {
        let contains_serialized_file =
            !bundle_path.ends_with(".resS") && !bundle_path.ends_with("resource");
        writeln!(&mut text, "--- {bundle_path} ---")?;

        if contains_serialized_file {
            let data = bundle
                .as_ref()
                .try_map(|bundle| bundle.read_at(bundle_path).transpose().unwrap())?;

            let diff = diff_serializedfile(cx, &path.join(bundle_path), data.as_deref())?;
            writeln!(&mut text, "  {diff}")?;
        }
    }

    Ok(text)
}

pub mod format {
    #![allow(non_snake_case, dead_code)]
    use rabex_env::rabex::{UnityVersion, files::SerializedFile as SerializedFileRabex};

    #[derive(Debug, Copy, Clone)]
    struct SerializedFileHeader {
        m_Version: u32,
    }

    #[derive(Debug, Clone)]
    pub struct SerializedFile<'a> {
        m_Header: SerializedFileHeader,
        m_UnityVersion: Option<UnityVersion>,
        m_TargetPlatform: Option<i32>,
        m_EnableTypeTree: bool,
        m_bigIDEnabled: Option<i32>,
        // pub m_Types: Vec<SerializedType>,
        // m_Objects: Vec<ObjectInfo>,
        // pub m_ScriptTypes: Option<Vec<LocalSerializedObjectIdentifier>>,
        m_Externals: Vec<&'a str>,
        // pub m_RefTypes: Option<Vec<SerializedType>>,
        m_UserInformation: Option<&'a str>,
        object_count: usize,
    }

    impl<'a> From<&'a SerializedFileRabex> for SerializedFile<'a> {
        fn from(value: &'a SerializedFileRabex) -> Self {
            SerializedFile {
                m_Header: SerializedFileHeader {
                    m_Version: value.m_Header.m_Version,
                },
                m_UnityVersion: value.m_UnityVersion,
                m_TargetPlatform: value.m_TargetPlatform,
                m_EnableTypeTree: value.m_EnableTypeTree,
                m_bigIDEnabled: value.m_bigIDEnabled,
                m_Externals: value
                    .m_Externals
                    .iter()
                    .map(|external| external.pathName.as_str())
                    .collect(),
                m_UserInformation: value.m_UserInformation.as_deref(),
                object_count: value.objects().len(),
            }
        }
    }
}
