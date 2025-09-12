use std::collections::HashSet;
use std::fmt::Write;
use std::io::Cursor;
use std::path::Path;

use anstream::eprintln;
use anstyle::{Color, Style};
use anyhow::{Context as _, Result};
use rabex::files::bundlefile::{BundleFileReader, ExtractionConfig};
use rabex::objects::{ClassId, TypedPPtr};
use rabex::serde_typetree;
use rabex::typetree::TypeTreeProvider;
use rabex_env::handle::ObjectRefHandle;
use rabex_env::resolver::BasedirEnvResolver;
use rabex_env::unity::types::GameObject;
use rabex_env::{handle::SerializedFileHandle, rabex::files::SerializedFile};
use serde::Deserialize;

use crate::old_new::OldNew;

use super::Context;

pub struct Filter {
    pub ignore_classes: HashSet<ClassId>,
}
impl Filter {
    fn matches<T, R, P>(&self, object: &ObjectRefHandle<'_, T, R, P>) -> bool {
        !self.ignore_classes.contains(&object.class_id())
    }
}

pub fn diff_serializedfile(cx: &Context, path: &Path, data: OldNew<&[u8]>) -> Result<String> {
    let env = cx
        .unity_game
        .as_ref()
        .context("cannot diff bundlefile outside unity game")?;

    let old_reader = &mut Cursor::new(data.old);
    let new_reader = &mut Cursor::new(data.new);
    let mut old = SerializedFile::from_reader(old_reader)?;
    let mut new = SerializedFile::from_reader(new_reader)?;
    old.m_UnityVersion.get_or_insert(env.old.unity_version()?);
    new.m_UnityVersion.get_or_insert(env.new.unity_version()?);

    let old = SerializedFileHandle::new(&env.old, &old, data.old);
    let new = SerializedFileHandle::new(&env.new, &new, data.new);

    let file = OldNew::new(old, new);
    let file = file.as_ref();

    let mut text = super::diff_text(
        cx,
        OldNew::new(
            &format!("{:#?}", format::SerializedFile::from(file.old.file)),
            &format!("{:#?}", format::SerializedFile::from(file.new.file)),
        ),
    );
    if !text.is_empty() {
        text.push('\n');
    }
    let object_count = file.map(|file| file.file.objects().len());
    if object_count.changed() {
        writeln!(
            &mut text,
            "{} -> {} objects",
            object_count.old, object_count.new,
        )?;
    }

    let object_changes = file
        .as_ref()
        .changes(|file| file.file.objects().map(|x| x.m_PathID));

    if !object_changes.removed.is_empty() {
        writeln!(
            &mut text,
            "Removed {} objects",
            object_changes.removed.len()
        )?;
    }
    if !object_changes.added.is_empty() {
        writeln!(&mut text, "Added {} objects", object_changes.added.len())?;
    }

    for added in object_changes.added {
        let object = file.new.object_at::<serde_json::Value>(added)?;

        if cx.unity_filter.matches(&object) {
            let val = object.read()?;
            let script = object.mono_script()?;
            let name = name(&object, &val)?;

            write!(&mut text, "--- added {:?}", object.class_id())?;
            if let Some(script) = &script {
                write!(&mut text, " {}", script.full_name())?;
            }
            write!(&mut text, " {name}")?;
            writeln!(&mut text, " ---")?;

            writeln!(&mut text, "{}", serde_json::to_string_pretty(&val)?)?;
        } else {
            let val = object.read()?;
            let name = name(&object, &val)?;
            writeln!(
                &mut text,
                "--- added {:?} {} ---",
                object.object.info.m_ClassID, name
            )?;
        }
    }

    for path_id in object_changes.same {
        let object = file
            .as_ref()
            .try_map(|file| file.object_at::<serde_json::Value>(path_id))?;
        let object = object.as_ref();
        let class_id = object.map(|val| val.class_id());

        let data = object.try_map_zip(&file, |obj, file| {
            obj.object.get_raw_data(&mut file.reader())
        })?;

        if data.changed() {
            if let Err(e) = (|| -> Result<()> {
                let old_value = serde_typetree::from_reader_endianed::<serde_json::Value>(
                    &mut Cursor::new(data.old),
                    &object.old.object.tt,
                    object.old.file.file.m_Header.m_Endianess,
                )?;
                let new_value = serde_typetree::from_reader_endianed::<serde_json::Value>(
                    &mut Cursor::new(data.new),
                    &object.new.object.tt,
                    object.new.file.file.m_Header.m_Endianess,
                )?;
                let value = OldNew::new(old_value, new_value);
                let value = value.as_ref();

                let name = object.try_map_zip(&value, |object, val| name(object, val))?;
                let script = object.try_map(|obj| obj.mono_script())?;

                let matches_filter = cx.unity_filter.matches(&object.new);
                let diff = matches_filter
                    .then(|| super::diff_json(cx, value))
                    .transpose()?
                    .filter(|diff| !diff.is_empty());

                if let Some(diff) = diff {
                    let mut major_change = false;

                    write!(&mut text, "--- changed {:?}", class_id.new)?;
                    if class_id.changed() {
                        write!(&mut text, " (previously {:?})", class_id.old)?;
                        major_change = true;
                    }
                    if let Some(script) = &script.new {
                        write!(&mut text, " {}", script.full_name())?;
                    }
                    if script.changed()
                        && let Some(script) = &script.old
                    {
                        major_change = true;
                        write!(&mut text, " (previously {})", script.full_name())?;
                    }
                    write!(&mut text, " {}", name.new)?;
                    if name.changed() {
                        write!(&mut text, " (previously {})", name.old)?;
                        major_change = true;
                    }
                    writeln!(&mut text, " ---")?;

                    if !major_change {
                        writeln!(&mut text, "{}", diff)?;
                    }
                }

                Ok(())
            })() {
                writeln!(
                    &mut text,
                    "--- change object {:?} at path id {path_id} ---",
                    object.new.class_id()
                )?;

                let style = Style::new().fg_color(Some(Color::Ansi(anstyle::AnsiColor::Red)));
                eprintln!(
                    "{style}Skipping {:?} object in {} (Path ID {}): {e:?}{style:#}",
                    object.new.class_id(),
                    path.display(),
                    path_id,
                );
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
    let bundle = bundle.as_ref();

    let changes = bundle.changes(|bundle| bundle.files().iter().map(|file| file.path.as_str()));

    let mut text = String::new();
    if !changes.removed.is_empty() {
        write!(&mut text, "Removed: {:?}", changes.removed)?;
    }
    if !changes.removed.is_empty() {
        write!(&mut text, "Added: {:?}", changes.added)?;
    }
    for &bundle_path in &changes.same {
        let contains_serialized_file =
            !bundle_path.ends_with(".resS") && !bundle_path.ends_with("resource");

        let size_changed = bundle
            .map(|bundle| bundle.file(bundle_path).unwrap().size)
            .changed();

        if contains_serialized_file {
            // writeln!(&mut text, "=== {bundle_path} ===")?;
            let data = bundle.try_map(|bundle| bundle.read_at(bundle_path).transpose().unwrap())?;
            let diff = diff_serializedfile(cx, &path.join(bundle_path), data.as_deref())?;
            write!(&mut text, "{diff}")?;
        } else if size_changed {
            writeln!(&mut text, "=== {bundle_path} ===")?;
            writeln!(&mut text, "changed")?;
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
            }
        }
    }
}

fn name<R: BasedirEnvResolver, P: TypeTreeProvider>(
    object: &ObjectRefHandle<serde_json::Value, R, P>,
    val: &serde_json::Value,
) -> Result<String> {
    let name = val
        .get("m_Name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut result = String::with_capacity(name.len() + 2);
    if !name.is_empty() {
        result.push('\'');
        result.push_str(name);
        result.push('\'');
    }

    if let Some(go) = val.get("m_GameObject") {
        let go = TypedPPtr::<GameObject>::deserialize(go)?;
        if let Some(go) = object.file.deref_optional(go)? {
            let path = go.path()?;
            if !result.is_empty() {
                result.push(' ');
            }
            write!(&mut result, "on '{path}'")?;
        }
    }

    Ok(result)
}
