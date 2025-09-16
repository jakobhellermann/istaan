use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Write};
use std::io::Cursor;
use std::path::Path;

use anstream::eprintln;
use anstyle::{Color, Style};
use anyhow::{Context as _, Result};
use rabex::files::bundlefile::{BundleFileReader, ExtractionConfig};
use rabex::objects::pptr::PathId;
use rabex::objects::{ClassId, TypedPPtr};
use rabex::serde_typetree;
use rabex::typetree::TypeTreeProvider;
use rabex_env::game_files::GameFiles;
use rabex_env::handle::{ObjectRefHandle, SerializedFileHandle};
use rabex_env::rabex::files::SerializedFile;
use rabex_env::resolver::BasedirEnvResolver;
use rabex_env::unity::types::{GameObject, MonoBehaviour, Transform};
use rustc_hash::{FxHashMap, FxHashSet};
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
    diff_serializedfile_smart(cx, path, data)
}

fn diff_serializedfile_smart(cx: &Context, _: &Path, data: OldNew<&[u8]>) -> Result<String> {
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

    /*if path.extension().is_some_and(|x| x == "sharedAssets") {
        return Ok(text);
    }*/

    let transforms = file.as_ref().try_map(|file| {
        file.transforms()?
            .map(|transform| {
                let path_id = transform.path_id();
                let transform = transform.read()?;
                let go = file.deref(transform.m_GameObject)?.read()?;
                Ok((path_id, (transform, go)))
            })
            .collect::<Result<FxHashMap<PathId, _>>>()
    })?;

    let mut cx = SceneMatcher {
        cx,
        transforms: &transforms,
        file,
        current_path: Vec::new(),
        current_old: PathId::default(),
        old_seen: HashSet::default(),
        out: &mut text,
    };
    cx.visit_roots()?;

    for (path_id, (t, go)) in &cx.transforms.old {
        if !cx.old_seen.contains(path_id) {
            let mut components: Vec<_> = std::iter::successors(Some((t, go)), |(t, _)| {
                let parent = cx.transforms.old.get(&t.m_Father.m_PathID)?;
                Some((&parent.0, &parent.1))
            })
            .map(|(_, go)| go.m_Name.as_str())
            .collect();
            components.reverse();
            let path = components.join("/");

            writeln!(&mut cx.out, "--- Removed object '{}' ---", path)?;
        }
    }

    Ok(text)
}

struct SceneMatcher<'a, P> {
    cx: &'a Context<'a>,
    transforms: &'a OldNew<FxHashMap<PathId, (Transform, GameObject)>>,
    file: OldNew<SerializedFileHandle<'a, GameFiles, P>>,

    current_old: PathId,
    current_path: Vec<String>,

    out: &'a mut String,

    old_seen: FxHashSet<PathId>,
}
impl<'a, P: TypeTreeProvider> SceneMatcher<'a, P> {
    fn added_object(&mut self, path: String) -> Result<()> {
        writeln!(self.out, "--- Added Object '{}' ---", path)?;
        Ok(())
    }
    fn compare(&mut self, path: String, data: OldNew<(&Transform, &GameObject)>) -> Result<()> {
        let components =
            data.as_ref()
                .map(|x| &x.1)
                .try_map_zip(&self.file, |go, file| -> Result<_> {
                    let mut components = BTreeMap::new();
                    for component in go.components(file.file, &file.env.tpk) {
                        let component = ObjectRefHandle::new(component?, file.reborrow());

                        let component_key = match component.class_id() {
                            ClassId::MonoBehaviour => {
                                match component.cast::<MonoBehaviour>().mono_script()? {
                                    Some(script) => {
                                        ComponentKey::Script(script.full_name().into_owned())
                                    }
                                    None => ComponentKey::ClassId(ClassId::MonoBehaviour),
                                }
                            }
                            class_id => ComponentKey::ClassId(class_id),
                        };

                        components.insert(component_key, component);
                    }
                    Ok(components)
                })?;
        let component_changes = components.as_ref().changes(|x| x.keys());

        for new_component in component_changes.added {
            writeln!(self.out, "--- Added {} @ '{}' ---\n", new_component, path)?;
        }
        for removed_component in component_changes.removed {
            writeln!(
                self.out,
                "--- Removed {} @ '{}' ---\n",
                removed_component, path
            )?;
        }
        for component in component_changes.same {
            let comp = components.as_ref().map(|x| &x[component]);

            if !self.cx.unity_filter.matches(comp.new) {
                continue;
            }

            let data = comp.map(|comp| {
                let start = comp.object.info.m_Offset as usize;
                &comp.file.data[start..start + comp.object.info.m_Size as usize]
            });
            if data.changed() {
                let value = data.try_map_zip(&comp, |data, comp| {
                    serde_typetree::from_reader_endianed::<serde_json::Value>(
                        &mut Cursor::new(data),
                        &comp.object.tt,
                        comp.file.file.m_Header.m_Endianess,
                    )
                })?;

                let diff = super::diff_json(self.cx, value.as_ref())?;
                if !diff.is_empty() {
                    writeln!(self.out, "--- Changed {} @ '{}' ---", component, path)?;
                    writeln!(self.out, "{}", diff)?;
                }
            }
        }

        Ok(())
    }

    fn visit(&mut self, transform: &Transform, go: &GameObject) -> Result<()> {
        assert!(!go.m_Name.is_empty());
        self.current_path.push(go.m_Name.clone());

        let current_old = self.current_old;
        let old = &self.transforms.old[&current_old];

        self.old_seen.insert(current_old);

        self.compare(
            self.current_path.join("/"),
            OldNew::new((&old.0, &old.1), (transform, go)),
        )?;

        let mut current_siblings_seen = FxHashMap::<String, usize>::default();
        for &child in &transform.m_Children {
            let child = self.file.new.deref(child)?;
            let child = child.read()?;
            let child_go = self.file.new.deref(child.m_GameObject)?.read()?;

            let sibling_index = {
                let entry = current_siblings_seen
                    .entry(child_go.m_Name.clone())
                    .or_default();
                let index = *entry;
                *entry += 1;
                index
            };

            let old_child = old
                .0
                .m_Children
                .iter()
                .filter_map(|&old_child| {
                    assert!(old_child.is_local());
                    let path_id = old_child.m_PathID;
                    let old_child = &self.transforms.old[&path_id];
                    (old_child.1.m_Name == child_go.m_Name).then_some((
                        path_id,
                        &old_child.0,
                        &old_child.1,
                    ))
                }).nth(sibling_index);

            let old_child = match old_child {
                Some(val) => val,
                None => {
                    let mut added_path = self.current_path.join("/");
                    added_path.push('/');
                    added_path.push_str(&child_go.m_Name);
                    self.added_object(added_path)?;
                    continue;
                }
            };

            self.current_old = old_child.0;
            self.visit(&child, &child_go)?;
        }

        self.current_old = current_old;
        self.current_path.pop();

        Ok(())
    }

    fn visit_roots(&mut self) -> Result<()> {
        let mut roots_seen = FxHashMap::<&str, usize>::default();
        for (&root, (_, root_go)) in self
            .transforms
            .new
            .iter()
            .filter(|(_, (t, _))| t.m_Father.is_null())
        {
            let root_seen_count = {
                let entry = roots_seen.entry(&root_go.m_Name).or_default();
                let seen = *entry;
                *entry += 1;
                seen
            };

            let matching_old = self
                .transforms
                .old
                .iter()
                .filter(|(_, (t, go))| t.m_Father.is_null() && go.m_Name == root_go.m_Name).nth(root_seen_count);

            let matching_old = match matching_old {
                Some(val) => val,
                None => {
                    writeln!(
                        &mut self.out,
                        "--- Removed Object '{}'@{}\n",
                        root_go.m_Name, root_seen_count
                    )?;
                    continue;
                }
            };
            self.current_old = *matching_old.0;

            let root = &self.transforms.new[&root];
            self.visit(&root.0, &root.1)?;
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ComponentKey {
    Script(String),
    ClassId(ClassId),
}
impl Display for ComponentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentKey::Script(name) => f.write_str(name),
            ComponentKey::ClassId(class_id) => std::fmt::Debug::fmt(class_id, f),
        }
    }
}

#[allow(dead_code)]
fn diff_serializedfile_old(cx: &Context, path: &Path, data: OldNew<&[u8]>) -> Result<String> {
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

        if data.changed()
            && let Err(e) = (|| -> Result<()> {
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

                let matches_filter = cx.unity_filter.matches(object.new);
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
                    if let Some(script_new) = &script.new {
                        let script_new_name = script_new.full_name();
                        write!(&mut text, " {}", script_new_name)?;
                        if let Some(script_old) = &script.old {
                            let script_old_name = script_old.full_name();
                            if script_new_name != script_old_name {
                                major_change = true;
                                write!(&mut text, " (previously {})", script_new.full_name())?;
                            }
                        }
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
            })()
        {
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
    use rabex_env::rabex::UnityVersion;
    use rabex_env::rabex::files::SerializedFile as SerializedFileRabex;

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
