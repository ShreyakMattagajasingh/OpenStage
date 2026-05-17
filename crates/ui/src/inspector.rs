//! Inspector data model. The desktop app builds an [`InspectorStatus`]
//! every frame from the live `scene::SceneGraph` + `scene::SceneSelection`
//! and the UI renders it via [`super::layout::draw_inspector_panel`].
//!
//! Keeping the data model in this crate (instead of `scene`) keeps egui-side
//! ergonomics (string IDs for serde-friendly UI actions, depth pre-computed
//! for outliner indent) out of the otherwise rendering-free scene crate.

use scene::{SceneGraph, SceneObject, SceneObjectKind, SceneSelection};
use serde::{Deserialize, Serialize};

/// Snapshot of the inspector contents the side-panel renders each frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorStatus {
    pub objects: Vec<InspectorObjectRow>,
    pub active: Option<String>,
    pub selected_count: usize,
    pub detail: Option<InspectorDetail>,
    pub filter: String,
}

/// One row in the outliner list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectorObjectRow {
    pub id: String,
    pub name: String,
    pub kind: SceneObjectKind,
    pub parent: Option<String>,
    pub depth: u8,
    pub visible: bool,
    pub locked: bool,
}

/// Per-`SceneObjectKind` detail block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum InspectorDetail {
    Avatar(InspectorAvatar),
    MeshInstance(InspectorMesh),
    Skeleton(InspectorSkeleton),
    Bone(InspectorBone),
    Material(InspectorMaterial),
    AnimationClip(InspectorClip),
    Camera(InspectorBasic),
    Light(InspectorBasic),
    Other(InspectorBasic),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorAvatar {
    pub id: String,
    pub name: String,
    pub body: Option<String>,
    pub slot_count: usize,
    pub current_clip: Option<String>,
    pub skeleton: Option<String>,
    pub expression: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub locked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorMesh {
    pub id: String,
    pub name: String,
    pub asset_id: Option<String>,
    pub parent: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub visible: bool,
    pub locked: bool,
    pub skinned: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorSkeleton {
    pub id: String,
    pub name: String,
    pub bone_count: usize,
    pub root_bone: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub locked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorBone {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub locked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorMaterial {
    pub id: String,
    pub name: String,
    pub base_color: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorClip {
    pub id: String,
    pub name: String,
    pub fps: Option<f32>,
    pub duration_frames: Option<u32>,
    pub looping: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InspectorBasic {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub visible: bool,
    pub locked: bool,
}

/// Build the per-frame inspector state from a scene graph + selection +
/// the current filter string. `filter` matches the object id or name
/// case-insensitively; an empty filter shows every object.
pub fn build_inspector_status(
    graph: &SceneGraph,
    selection: &SceneSelection,
    filter: &str,
) -> InspectorStatus {
    let objects = build_object_rows(graph, filter);
    let active = selection
        .active_object
        .as_ref()
        .map(|id| id.as_str().to_string());
    let selected_count = selection.selected_objects.len();
    let detail = active
        .as_ref()
        .and_then(|id| graph.get_object(&id.as_str().try_into().ok()?))
        .map(|obj| build_detail(graph, obj));
    InspectorStatus {
        objects,
        active,
        selected_count,
        detail,
        filter: filter.to_string(),
    }
}

fn build_object_rows(graph: &SceneGraph, filter: &str) -> Vec<InspectorObjectRow> {
    let mut rows = Vec::new();
    let filter_lc = filter.trim().to_lowercase();
    for obj in graph.list_objects() {
        if !filter_lc.is_empty() {
            let id_match = obj.id.as_str().to_lowercase().contains(&filter_lc);
            let name_match = obj.name.to_lowercase().contains(&filter_lc);
            if !id_match && !name_match {
                continue;
            }
        }
        let depth = compute_depth(graph, obj);
        rows.push(InspectorObjectRow {
            id: obj.id.as_str().to_string(),
            name: obj.name.clone(),
            kind: obj.kind,
            parent: obj.parent.as_ref().map(|p| p.as_str().to_string()),
            depth,
            visible: obj.visible,
            locked: obj.locked,
        });
    }
    rows
}

fn compute_depth(graph: &SceneGraph, obj: &SceneObject) -> u8 {
    let mut depth: u8 = 0;
    let mut cursor = obj.parent.clone();
    while let Some(parent_id) = cursor {
        depth = depth.saturating_add(1);
        cursor = graph.get_object(&parent_id).and_then(|p| p.parent.clone());
        if depth >= 16 {
            break;
        }
    }
    depth
}

fn build_detail(graph: &SceneGraph, obj: &SceneObject) -> InspectorDetail {
    match obj.kind {
        SceneObjectKind::Avatar => InspectorDetail::Avatar(InspectorAvatar {
            id: obj.id.as_str().to_string(),
            name: obj.name.clone(),
            body: obj.asset_id.clone(),
            slot_count: graph.list_children(&obj.id).len(),
            current_clip: obj
                .metadata
                .get("current_clip")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            skeleton: obj
                .metadata
                .get("skeleton")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            expression: obj
                .metadata
                .get("expression")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            translation: obj.transform.translation,
            rotation: obj.transform.rotation,
            scale: obj.transform.scale,
            locked: obj.locked,
        }),
        SceneObjectKind::MeshInstance | SceneObjectKind::SkinnedMeshInstance => {
            InspectorDetail::MeshInstance(InspectorMesh {
                id: obj.id.as_str().to_string(),
                name: obj.name.clone(),
                asset_id: obj.asset_id.clone(),
                parent: obj.parent.as_ref().map(|p| p.as_str().to_string()),
                translation: obj.transform.translation,
                rotation: obj.transform.rotation,
                scale: obj.transform.scale,
                visible: obj.visible,
                locked: obj.locked,
                skinned: matches!(obj.kind, SceneObjectKind::SkinnedMeshInstance),
            })
        }
        SceneObjectKind::Skeleton => {
            let bones: Vec<&SceneObject> = graph
                .list_children(&obj.id)
                .into_iter()
                .filter(|c| matches!(c.kind, SceneObjectKind::Bone))
                .collect();
            let root_bone = bones
                .iter()
                .find(|b| b.parent.as_ref() == Some(&obj.id))
                .map(|b| b.id.as_str().to_string());
            InspectorDetail::Skeleton(InspectorSkeleton {
                id: obj.id.as_str().to_string(),
                name: obj.name.clone(),
                bone_count: bones.len(),
                root_bone,
                translation: obj.transform.translation,
                rotation: obj.transform.rotation,
                scale: obj.transform.scale,
                locked: obj.locked,
            })
        }
        SceneObjectKind::Bone => InspectorDetail::Bone(InspectorBone {
            id: obj.id.as_str().to_string(),
            name: obj.name.clone(),
            parent: obj.parent.as_ref().map(|p| p.as_str().to_string()),
            translation: obj.transform.translation,
            rotation: obj.transform.rotation,
            scale: obj.transform.scale,
            locked: obj.locked,
        }),
        SceneObjectKind::Material => InspectorDetail::Material(InspectorMaterial {
            id: obj.id.as_str().to_string(),
            name: obj.name.clone(),
            base_color: obj
                .metadata
                .get("base_color")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    if arr.len() == 3 {
                        let r = arr[0].as_f64()? as f32;
                        let g = arr[1].as_f64()? as f32;
                        let b = arr[2].as_f64()? as f32;
                        Some([r, g, b])
                    } else {
                        None
                    }
                }),
        }),
        SceneObjectKind::AnimationClip => InspectorDetail::AnimationClip(InspectorClip {
            id: obj.id.as_str().to_string(),
            name: obj.name.clone(),
            fps: obj
                .metadata
                .get("fps")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32),
            duration_frames: obj
                .metadata
                .get("duration_frames")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            looping: obj.metadata.get("looping").and_then(|v| v.as_bool()),
        }),
        SceneObjectKind::Camera => InspectorDetail::Camera(basic(obj, "camera")),
        SceneObjectKind::Light => InspectorDetail::Light(basic(obj, "light")),
        _ => InspectorDetail::Other(basic(obj, kind_label(obj.kind))),
    }
}

fn basic(obj: &SceneObject, kind: &str) -> InspectorBasic {
    InspectorBasic {
        id: obj.id.as_str().to_string(),
        name: obj.name.clone(),
        kind: kind.to_string(),
        translation: obj.transform.translation,
        rotation: obj.transform.rotation,
        scale: obj.transform.scale,
        visible: obj.visible,
        locked: obj.locked,
    }
}

/// Human label for a `SceneObjectKind` shown in the outliner / detail panel.
pub fn kind_label(kind: SceneObjectKind) -> &'static str {
    match kind {
        SceneObjectKind::Avatar => "avatar",
        SceneObjectKind::MeshInstance => "mesh",
        SceneObjectKind::SkinnedMeshInstance => "skinned mesh",
        SceneObjectKind::Skeleton => "skeleton",
        SceneObjectKind::Bone => "bone",
        SceneObjectKind::Material => "material",
        SceneObjectKind::AnimationClip => "clip",
        SceneObjectKind::Camera => "camera",
        SceneObjectKind::Light => "light",
        SceneObjectKind::Accessory => "accessory",
        SceneObjectKind::AttachmentPoint => "attachment",
        SceneObjectKind::Empty => "empty",
        SceneObjectKind::Constraint => "constraint",
        SceneObjectKind::Pose => "pose",
        SceneObjectKind::BlendshapeSet => "blendshapes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene::{SceneId, SceneObject, SceneObjectKind};

    fn obj(id: &str, name: &str, kind: SceneObjectKind, parent: Option<&str>) -> SceneObject {
        let mut o = SceneObject::new(id, name, kind).unwrap();
        if let Some(p) = parent {
            o.parent = Some(SceneId::new(p).unwrap());
        }
        o
    }

    #[test]
    fn outliner_rows_indent_by_depth() {
        let mut graph = SceneGraph::new();
        graph
            .insert(obj("avatar_001", "Avatar", SceneObjectKind::Avatar, None))
            .unwrap();
        graph
            .insert(obj(
                "skeleton_avatar_001",
                "Skeleton",
                SceneObjectKind::Skeleton,
                Some("avatar_001"),
            ))
            .unwrap();
        graph
            .insert(obj(
                "bone_root",
                "Root Bone",
                SceneObjectKind::Bone,
                Some("skeleton_avatar_001"),
            ))
            .unwrap();
        let status = build_inspector_status(&graph, &SceneSelection::default(), "");
        let depths: std::collections::HashMap<_, _> = status
            .objects
            .iter()
            .map(|r| (r.id.clone(), r.depth))
            .collect();
        assert_eq!(depths["avatar_001"], 0);
        assert_eq!(depths["skeleton_avatar_001"], 1);
        assert_eq!(depths["bone_root"], 2);
    }

    #[test]
    fn filter_matches_case_insensitive() {
        let mut graph = SceneGraph::new();
        graph
            .insert(obj(
                "avatar_001",
                "Phase 4 Rig",
                SceneObjectKind::Avatar,
                None,
            ))
            .unwrap();
        graph
            .insert(obj(
                "camera_main",
                "Main Camera",
                SceneObjectKind::Camera,
                None,
            ))
            .unwrap();
        let status = build_inspector_status(&graph, &SceneSelection::default(), "PHASE");
        assert_eq!(status.objects.len(), 1);
        assert_eq!(status.objects[0].id, "avatar_001");
    }

    #[test]
    fn empty_filter_lists_everything() {
        let mut graph = SceneGraph::new();
        graph
            .insert(obj("avatar_001", "Avatar", SceneObjectKind::Avatar, None))
            .unwrap();
        graph
            .insert(obj("camera_main", "Camera", SceneObjectKind::Camera, None))
            .unwrap();
        let status = build_inspector_status(&graph, &SceneSelection::default(), "");
        assert_eq!(status.objects.len(), 2);
    }

    #[test]
    fn active_selection_drives_detail() {
        let mut graph = SceneGraph::new();
        graph
            .insert(obj(
                "avatar_001",
                "Phase 4 Rig",
                SceneObjectKind::Avatar,
                None,
            ))
            .unwrap();
        let mut sel = SceneSelection::default();
        sel.set_objects([SceneId::new("avatar_001").unwrap()]);
        let status = build_inspector_status(&graph, &sel, "");
        assert_eq!(status.active.as_deref(), Some("avatar_001"));
        assert_eq!(status.selected_count, 1);
        assert!(matches!(status.detail, Some(InspectorDetail::Avatar(_))));
    }
}
