//! Logical scene graph for Avatar Studio.
//!
//! This crate is intentionally renderer-free. It gives commands, tests, and
//! future agent integrations stable object IDs and JSON-friendly scene data.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SceneError {
    #[error("invalid scene id '{id}': {reason}")]
    InvalidId { id: String, reason: &'static str },
    #[error("scene object already exists: {0}")]
    DuplicateObject(SceneId),
    #[error("scene object not found: {0}")]
    ObjectNotFound(SceneId),
    #[error("parent object not found: {0}")]
    ParentNotFound(SceneId),
    #[error("cannot parent an object to itself")]
    SelfParent,
    #[error("reparent would create a graph cycle")]
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SceneId(String);

impl SceneId {
    pub fn new(id: impl Into<String>) -> Result<Self, SceneError> {
        let id = id.into();
        validate_id(&id)?;
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SceneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for SceneId {
    type Error = SceneError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for SceneId {
    type Error = SceneError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

fn validate_id(id: &str) -> Result<(), SceneError> {
    if id.trim().is_empty() {
        return Err(SceneError::InvalidId {
            id: id.to_string(),
            reason: "id is empty",
        });
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(SceneError::InvalidId {
            id: id.to_string(),
            reason: "only ASCII letters, digits, '_', '-', and '.' are allowed",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneObjectKind {
    Avatar,
    MeshInstance,
    SkinnedMeshInstance,
    Skeleton,
    Bone,
    Material,
    AnimationClip,
    Camera,
    Light,
    Accessory,
    AttachmentPoint,
    Empty,
    Constraint,
    Pose,
    BlendshapeSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneObject {
    pub id: SceneId,
    pub name: String,
    pub kind: SceneObjectKind,
    pub parent: Option<SceneId>,
    pub asset_id: Option<String>,
    pub transform: SceneTransform,
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl SceneObject {
    pub fn new(
        id: impl TryInto<SceneId, Error = SceneError>,
        name: impl Into<String>,
        kind: SceneObjectKind,
    ) -> Result<Self, SceneError> {
        Ok(Self {
            id: id.try_into()?,
            name: name.into(),
            kind,
            parent: None,
            asset_id: None,
            transform: SceneTransform::default(),
            visible: true,
            locked: false,
            metadata: BTreeMap::new(),
        })
    }

    pub fn with_parent(
        mut self,
        parent: impl TryInto<SceneId, Error = SceneError>,
    ) -> Result<Self, SceneError> {
        self.parent = Some(parent.try_into()?);
        Ok(self)
    }

    pub fn with_asset_id(mut self, asset_id: impl Into<String>) -> Self {
        self.asset_id = Some(asset_id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSelection {
    pub active_object: Option<SceneId>,
    pub selected_objects: Vec<SceneId>,
    pub active_bone: Option<SceneId>,
    pub selected_bones: Vec<SceneId>,
    pub active_clip: Option<SceneId>,
    pub selected_keyframes: Vec<String>,
}

impl SceneSelection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn set_objects<I>(&mut self, ids: I)
    where
        I: IntoIterator<Item = SceneId>,
    {
        let mut seen = BTreeSet::new();
        self.selected_objects = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        self.active_object = self.selected_objects.first().cloned();
        self.active_bone = None;
        self.selected_bones.clear();
        self.active_clip = None;
        self.selected_keyframes.clear();
    }

    pub fn list_selected(&self) -> Vec<SceneId> {
        let mut out = self.selected_objects.clone();
        out.extend(self.selected_bones.clone());
        if let Some(clip) = self.active_clip.clone() {
            if !out.contains(&clip) {
                out.push(clip);
            }
        }
        out
    }

    pub fn validate_against(&self, graph: &SceneGraph) -> Result<(), SceneError> {
        for id in self.list_selected() {
            if !graph.contains(&id) {
                return Err(SceneError::ObjectNotFound(id));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneGraph {
    objects: BTreeMap<SceneId, SceneObject>,
    children: BTreeMap<SceneId, Vec<SceneId>>,
    pub selection: SceneSelection,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.children.clear();
        self.selection.clear();
    }

    pub fn insert(&mut self, object: SceneObject) -> Result<(), SceneError> {
        if self.objects.contains_key(&object.id) {
            return Err(SceneError::DuplicateObject(object.id));
        }
        if let Some(parent) = &object.parent {
            if !self.objects.contains_key(parent) {
                return Err(SceneError::ParentNotFound(parent.clone()));
            }
            self.children
                .entry(parent.clone())
                .or_default()
                .push(object.id.clone());
        }
        self.children.entry(object.id.clone()).or_default();
        self.objects.insert(object.id.clone(), object);
        Ok(())
    }

    pub fn remove(&mut self, id: &SceneId) -> Result<SceneObject, SceneError> {
        if !self.objects.contains_key(id) {
            return Err(SceneError::ObjectNotFound(id.clone()));
        }
        let descendants = self.list_descendants(id);
        for child in descendants.iter().rev() {
            self.remove_single(child);
        }
        self.remove_single(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))
    }

    fn remove_single(&mut self, id: &SceneId) -> Option<SceneObject> {
        let removed = self.objects.remove(id)?;
        if let Some(parent) = &removed.parent {
            if let Some(children) = self.children.get_mut(parent) {
                children.retain(|child| child != id);
            }
        }
        self.children.remove(id);
        self.selection
            .selected_objects
            .retain(|selected| selected != id);
        self.selection
            .selected_bones
            .retain(|selected| selected != id);
        if self.selection.active_object.as_ref() == Some(id) {
            self.selection.active_object = self.selection.selected_objects.first().cloned();
        }
        if self.selection.active_bone.as_ref() == Some(id) {
            self.selection.active_bone = self.selection.selected_bones.first().cloned();
        }
        if self.selection.active_clip.as_ref() == Some(id) {
            self.selection.active_clip = None;
        }
        Some(removed)
    }

    pub fn reparent(
        &mut self,
        id: &SceneId,
        new_parent: Option<SceneId>,
    ) -> Result<(), SceneError> {
        if !self.objects.contains_key(id) {
            return Err(SceneError::ObjectNotFound(id.clone()));
        }
        if new_parent.as_ref() == Some(id) {
            return Err(SceneError::SelfParent);
        }
        if let Some(parent) = &new_parent {
            if !self.objects.contains_key(parent) {
                return Err(SceneError::ParentNotFound(parent.clone()));
            }
            if self.list_descendants(id).contains(parent) {
                return Err(SceneError::Cycle);
            }
        }
        let old_parent = self.objects.get(id).and_then(|obj| obj.parent.clone());
        if let Some(old_parent) = old_parent {
            if let Some(children) = self.children.get_mut(&old_parent) {
                children.retain(|child| child != id);
            }
        }
        if let Some(parent) = &new_parent {
            self.children
                .entry(parent.clone())
                .or_default()
                .push(id.clone());
        }
        self.objects.get_mut(id).unwrap().parent = new_parent;
        Ok(())
    }

    pub fn contains(&self, id: &SceneId) -> bool {
        self.objects.contains_key(id)
    }

    pub fn get_object(&self, id: &SceneId) -> Option<&SceneObject> {
        self.objects.get(id)
    }

    /// Flip the `visible` flag on an existing object. Returns the previous
    /// value so the command router can capture it for undo.
    pub fn set_visible(&mut self, id: &SceneId, visible: bool) -> Result<bool, SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.visible;
        obj.visible = visible;
        Ok(prev)
    }

    /// Flip the `locked` flag on an existing object. Returns the previous
    /// value so the command router can capture it for undo.
    pub fn set_locked(&mut self, id: &SceneId, locked: bool) -> Result<bool, SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.locked;
        obj.locked = locked;
        Ok(prev)
    }

    pub fn set_translation(
        &mut self,
        id: &SceneId,
        translation: [f32; 3],
    ) -> Result<[f32; 3], SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.transform.translation;
        obj.transform.translation = translation;
        Ok(prev)
    }

    pub fn set_rotation(
        &mut self,
        id: &SceneId,
        rotation: [f32; 4],
    ) -> Result<[f32; 4], SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.transform.rotation;
        obj.transform.rotation = rotation;
        Ok(prev)
    }

    pub fn set_scale(&mut self, id: &SceneId, scale: [f32; 3]) -> Result<[f32; 3], SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.transform.scale;
        obj.transform.scale = scale;
        Ok(prev)
    }

    pub fn reset_transform(&mut self, id: &SceneId) -> Result<SceneTransform, SceneError> {
        let obj = self
            .objects
            .get_mut(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
        let prev = obj.transform;
        obj.transform = SceneTransform::default();
        Ok(prev)
    }

    pub fn list_children(&self, id: &SceneId) -> Vec<&SceneObject> {
        self.children
            .get(id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.objects.get(id))
            .collect()
    }

    pub fn list_descendants(&self, id: &SceneId) -> Vec<SceneId> {
        let mut out = Vec::new();
        let mut queue: VecDeque<SceneId> =
            self.children.get(id).cloned().unwrap_or_default().into();
        while let Some(next) = queue.pop_front() {
            out.push(next.clone());
            if let Some(children) = self.children.get(&next) {
                queue.extend(children.iter().cloned());
            }
        }
        out
    }

    pub fn list_objects(&self) -> Vec<&SceneObject> {
        self.objects.values().collect()
    }

    pub fn list_avatars(&self) -> Vec<&SceneObject> {
        self.find_by_type(SceneObjectKind::Avatar)
    }

    pub fn list_meshes(&self) -> Vec<&SceneObject> {
        self.objects
            .values()
            .filter(|obj| {
                matches!(
                    obj.kind,
                    SceneObjectKind::MeshInstance | SceneObjectKind::SkinnedMeshInstance
                )
            })
            .collect()
    }

    pub fn list_skeletons(&self) -> Vec<&SceneObject> {
        self.find_by_type(SceneObjectKind::Skeleton)
    }

    pub fn list_bones(&self) -> Vec<&SceneObject> {
        self.find_by_type(SceneObjectKind::Bone)
    }

    pub fn list_animation_clips(&self) -> Vec<&SceneObject> {
        self.find_by_type(SceneObjectKind::AnimationClip)
    }

    pub fn list_materials(&self) -> Vec<&SceneObject> {
        self.find_by_type(SceneObjectKind::Material)
    }

    pub fn list_selected(&self) -> Vec<&SceneObject> {
        self.selection
            .list_selected()
            .iter()
            .filter_map(|id| self.objects.get(id))
            .collect()
    }

    pub fn find_by_name(&self, needle: &str) -> Vec<&SceneObject> {
        let needle = needle.to_ascii_lowercase();
        self.objects
            .values()
            .filter(|obj| obj.name.to_ascii_lowercase().contains(&needle))
            .collect()
    }

    pub fn find_by_type(&self, kind: SceneObjectKind) -> Vec<&SceneObject> {
        self.objects
            .values()
            .filter(|obj| obj.kind == kind)
            .collect()
    }

    pub fn find_by_asset(&self, asset_id: &str) -> Vec<&SceneObject> {
        self.objects
            .values()
            .filter(|obj| obj.asset_id.as_deref() == Some(asset_id))
            .collect()
    }

    pub fn get_scene_summary(&self) -> SceneSummary {
        SceneSummary {
            object_count: self.objects.len(),
            selected_count: self.selection.list_selected().len(),
            avatars: self.list_avatars().len(),
            meshes: self.list_meshes().len(),
            skeletons: self.list_skeletons().len(),
            bones: self.list_bones().len(),
            materials: self.list_materials().len(),
            animation_clips: self.list_animation_clips().len(),
        }
    }

    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSummary {
    pub object_count: usize,
    pub selected_count: usize,
    pub avatars: usize,
    pub meshes: usize,
    pub skeletons: usize,
    pub bones: usize,
    pub materials: usize,
    pub animation_clips: usize,
}

pub trait SceneQuery {
    fn get_scene_summary(&self) -> SceneSummary;
    fn get_object(&self, id: &SceneId) -> Option<&SceneObject>;
    fn list_objects(&self) -> Vec<&SceneObject>;
    fn list_avatars(&self) -> Vec<&SceneObject>;
    fn list_meshes(&self) -> Vec<&SceneObject>;
    fn list_skeletons(&self) -> Vec<&SceneObject>;
    fn list_bones(&self) -> Vec<&SceneObject>;
    fn list_animation_clips(&self) -> Vec<&SceneObject>;
    fn list_materials(&self) -> Vec<&SceneObject>;
    fn list_selected(&self) -> Vec<&SceneObject>;
    fn find_by_name(&self, needle: &str) -> Vec<&SceneObject>;
    fn find_by_type(&self, kind: SceneObjectKind) -> Vec<&SceneObject>;
    fn find_by_asset(&self, asset_id: &str) -> Vec<&SceneObject>;
}

impl SceneQuery for SceneGraph {
    fn get_scene_summary(&self) -> SceneSummary {
        SceneGraph::get_scene_summary(self)
    }

    fn get_object(&self, id: &SceneId) -> Option<&SceneObject> {
        SceneGraph::get_object(self, id)
    }

    fn list_objects(&self) -> Vec<&SceneObject> {
        SceneGraph::list_objects(self)
    }

    fn list_avatars(&self) -> Vec<&SceneObject> {
        SceneGraph::list_avatars(self)
    }

    fn list_meshes(&self) -> Vec<&SceneObject> {
        SceneGraph::list_meshes(self)
    }

    fn list_skeletons(&self) -> Vec<&SceneObject> {
        SceneGraph::list_skeletons(self)
    }

    fn list_bones(&self) -> Vec<&SceneObject> {
        SceneGraph::list_bones(self)
    }

    fn list_animation_clips(&self) -> Vec<&SceneObject> {
        SceneGraph::list_animation_clips(self)
    }

    fn list_materials(&self) -> Vec<&SceneObject> {
        SceneGraph::list_materials(self)
    }

    fn list_selected(&self) -> Vec<&SceneObject> {
        SceneGraph::list_selected(self)
    }

    fn find_by_name(&self, needle: &str) -> Vec<&SceneObject> {
        SceneGraph::find_by_name(self, needle)
    }

    fn find_by_type(&self, kind: SceneObjectKind) -> Vec<&SceneObject> {
        SceneGraph::find_by_type(self, kind)
    }

    fn find_by_asset(&self, asset_id: &str) -> Vec<&SceneObject> {
        SceneGraph::find_by_asset(self, asset_id)
    }
}

pub fn metadata_slot(
    slot: &str,
    supports_color: bool,
    color_srgb: [f32; 3],
) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("slot".to_string(), json!(slot)),
        ("supports_color".to_string(), json!(supports_color)),
        ("color_srgb".to_string(), json!(color_srgb)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> SceneId {
        SceneId::new(value).unwrap()
    }

    fn object(value: &str, kind: SceneObjectKind) -> SceneObject {
        SceneObject::new(value, value, kind).unwrap()
    }

    #[test]
    fn stable_ids_validate() {
        assert!(SceneId::new("avatar_001").is_ok());
        assert!(SceneId::new("mesh_top_001").is_ok());
        assert!(SceneId::new("").is_err());
        assert!(SceneId::new("bad id").is_err());
    }

    #[test]
    fn objects_can_be_created_queried_reparented_and_deleted() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        graph
            .insert(
                SceneObject::new(
                    "mesh_body_001",
                    "Body",
                    SceneObjectKind::SkinnedMeshInstance,
                )
                .unwrap()
                .with_parent("avatar_001")
                .unwrap(),
            )
            .unwrap();
        graph
            .insert(object("empty_001", SceneObjectKind::Empty))
            .unwrap();
        graph
            .reparent(&id("mesh_body_001"), Some(id("empty_001")))
            .unwrap();

        assert_eq!(
            graph.get_object(&id("mesh_body_001")).unwrap().parent,
            Some(id("empty_001"))
        );
        assert_eq!(graph.list_children(&id("empty_001")).len(), 1);

        graph.remove(&id("empty_001")).unwrap();
        assert!(graph.get_object(&id("mesh_body_001")).is_none());
    }

    #[test]
    fn delete_removes_descendants_and_selection() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("root", SceneObjectKind::Empty))
            .unwrap();
        graph
            .insert(
                object("child", SceneObjectKind::MeshInstance)
                    .with_parent("root")
                    .unwrap(),
            )
            .unwrap();
        graph.selection.set_objects([id("child")]);
        graph.remove(&id("root")).unwrap();
        assert!(graph.selection.selected_objects.is_empty());
        assert!(graph.get_object(&id("child")).is_none());
    }

    #[test]
    fn selection_validates_missing_objects() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        let mut selection = SceneSelection::default();
        selection.set_objects([id("avatar_001")]);
        assert!(selection.validate_against(&graph).is_ok());
        selection.set_objects([id("missing")]);
        assert!(selection.validate_against(&graph).is_err());
    }

    #[test]
    fn query_api_filters_by_type_name_asset_and_children() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        graph
            .insert(
                SceneObject::new(
                    "mesh_top_001",
                    "Phase 7 Top",
                    SceneObjectKind::SkinnedMeshInstance,
                )
                .unwrap()
                .with_parent("avatar_001")
                .unwrap()
                .with_asset_id("top_phase7_basic_001"),
            )
            .unwrap();
        graph
            .insert(
                SceneObject::new("mat_top_primary", "Top Material", SceneObjectKind::Material)
                    .unwrap()
                    .with_parent("mesh_top_001")
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(graph.list_avatars().len(), 1);
        assert_eq!(graph.list_meshes().len(), 1);
        assert_eq!(graph.list_materials().len(), 1);
        assert_eq!(graph.find_by_name("top").len(), 2);
        assert_eq!(graph.find_by_asset("top_phase7_basic_001").len(), 1);
        assert_eq!(graph.list_children(&id("avatar_001")).len(), 1);
    }

    #[test]
    fn scene_graph_serializes_and_deserializes() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("camera_main", SceneObjectKind::Camera))
            .unwrap();
        let json = graph.to_json_pretty().unwrap();
        assert!(json.contains("camera_main"));
        let round_trip: SceneGraph = serde_json::from_str(&json).unwrap();
        assert!(round_trip.contains(&id("camera_main")));
    }

    #[test]
    fn summary_counts_core_objects() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        graph
            .insert(object("skeleton_avatar_001", SceneObjectKind::Skeleton))
            .unwrap();
        graph
            .insert(object("bone_head", SceneObjectKind::Bone))
            .unwrap();
        graph
            .insert(object("clip_idle", SceneObjectKind::AnimationClip))
            .unwrap();
        let summary = graph.get_scene_summary();
        assert_eq!(summary.object_count, 4);
        assert_eq!(summary.avatars, 1);
        assert_eq!(summary.skeletons, 1);
        assert_eq!(summary.bones, 1);
        assert_eq!(summary.animation_clips, 1);
    }

    #[test]
    fn set_visible_returns_previous_value() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        let avatar_id = id("avatar_001");
        let prev = graph.set_visible(&avatar_id, false).unwrap();
        assert!(prev, "default visibility should be true");
        assert!(!graph.get_object(&avatar_id).unwrap().visible);
        let prev = graph.set_visible(&avatar_id, true).unwrap();
        assert!(!prev);
        assert!(graph.get_object(&avatar_id).unwrap().visible);
    }

    #[test]
    fn set_visible_rejects_unknown_id() {
        let mut graph = SceneGraph::new();
        let missing = id("avatar_nope");
        let err = graph.set_visible(&missing, false).unwrap_err();
        assert!(matches!(err, SceneError::ObjectNotFound(_)));
    }

    #[test]
    fn set_locked_round_trip() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("mesh_top_001", SceneObjectKind::MeshInstance))
            .unwrap();
        let mesh_id = id("mesh_top_001");
        let prev = graph.set_locked(&mesh_id, true).unwrap();
        assert!(!prev, "default locked should be false");
        assert!(graph.get_object(&mesh_id).unwrap().locked);
        let prev = graph.set_locked(&mesh_id, false).unwrap();
        assert!(prev);
        assert!(!graph.get_object(&mesh_id).unwrap().locked);
    }

    #[test]
    fn set_translation_returns_previous_value() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("mesh_top_001", SceneObjectKind::MeshInstance))
            .unwrap();
        let mesh_id = id("mesh_top_001");
        let prev = graph.set_translation(&mesh_id, [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(prev, [0.0, 0.0, 0.0]);
        assert_eq!(
            graph.get_object(&mesh_id).unwrap().transform.translation,
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn set_rotation_returns_previous_value() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("bone_head", SceneObjectKind::Bone))
            .unwrap();
        let bone_id = id("bone_head");
        let prev = graph
            .set_rotation(&bone_id, [0.0, 0.5, 0.0, 0.8660254])
            .unwrap();
        assert_eq!(prev, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            graph.get_object(&bone_id).unwrap().transform.rotation,
            [0.0, 0.5, 0.0, 0.8660254]
        );
    }

    #[test]
    fn set_scale_and_reset_transform_round_trip() {
        let mut graph = SceneGraph::new();
        graph
            .insert(object("avatar_001", SceneObjectKind::Avatar))
            .unwrap();
        let avatar_id = id("avatar_001");
        let prev = graph.set_scale(&avatar_id, [1.25, 1.0, 0.75]).unwrap();
        assert_eq!(prev, [1.0, 1.0, 1.0]);
        let prev = graph.reset_transform(&avatar_id).unwrap();
        assert_eq!(prev.scale, [1.25, 1.0, 0.75]);
        assert_eq!(
            graph.get_object(&avatar_id).unwrap().transform,
            SceneTransform::default()
        );
    }
}
