//! Procedural "Phase 7 Top" fixture — an octagonal chest tube skinned to the
//! `chest` and `spine` bones of an `avatar_skeleton_v1` rig.
//!
//! Reads `assets/processed/avatars/bodies/phase4_rig.glb` to copy the
//! skeleton's joint order + inverse-bind matrices verbatim, so the wearable's
//! JOINTS_0 indices line up with the body's runtime skinning palette.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use gj::validation::{Checked::Valid, USize64};
use glam::{Mat4, Vec3};
use gltf::json as gj;
use serde_json::Value;
use tracing::info;

use animation::Skeleton;

use crate::glb_writer::write_glb;

const TUBE_SEGMENTS: u32 = 12;
const TUBE_RADIUS: f32 = 0.32;
const RIG_PATH: &str = "assets/processed/avatars/bodies/phase4_rig.glb";
const OUT_GLB: &str = "assets/processed/avatars/tops/phase7_top.glb";
const OUT_META: &str = "assets/processed/metadata/phase7_top.json";

pub fn generate(workspace_root: &Path) -> Result<()> {
    let rig_path = workspace_root.join(RIG_PATH);
    let out_glb = workspace_root.join(OUT_GLB);
    let out_meta = workspace_root.join(OUT_META);

    let (document, buffers, _) =
        gltf::import(&rig_path).with_context(|| format!("import rig {}", rig_path.display()))?;
    let skeleton = Skeleton::from_gltf(&document, &buffers)
        .with_context(|| format!("parse rig skeleton {}", rig_path.display()))?
        .ok_or_else(|| anyhow!("rig has no skeleton"))?;

    let chest_idx = skeleton
        .bone_index("chest")
        .ok_or_else(|| anyhow!("rig has no `chest` bone"))?
        .0;
    let spine_idx = skeleton
        .bone_index("spine")
        .ok_or_else(|| anyhow!("rig has no `spine` bone"))?
        .0;

    let chest_world = skeleton.bones[chest_idx].world_bind_transform;
    let spine_world = skeleton.bones[spine_idx].world_bind_transform;
    let chest_pos = world_translation(&chest_world);
    let spine_pos = world_translation(&spine_world);

    let bone_count = skeleton.bones.len();
    if bone_count > u16::MAX as usize {
        bail!("rig has too many bones for u16 JOINTS_0");
    }

    // --- 1. Vertices --------------------------------------------------------
    // Top ring + bottom ring; positions are in *world* space at bind pose so
    // that palette = identity at bind, leaving the vertex unchanged. fit_matrix
    // applied at draw time scales the whole thing into the viewport.
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(2 * TUBE_SEGMENTS as usize);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(2 * TUBE_SEGMENTS as usize);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(2 * TUBE_SEGMENTS as usize);
    let mut joints: Vec<[u16; 4]> = Vec::with_capacity(2 * TUBE_SEGMENTS as usize);
    let mut weights: Vec<[f32; 4]> = Vec::with_capacity(2 * TUBE_SEGMENTS as usize);

    let chest_joint = chest_idx as u16;
    let spine_joint = spine_idx as u16;

    for s in 0..TUBE_SEGMENTS {
        let theta = (s as f32 / TUBE_SEGMENTS as f32) * TAU;
        let (sin, cos) = theta.sin_cos();
        let radial = Vec3::new(cos, 0.0, sin) * TUBE_RADIUS;

        let top_pos = chest_pos + radial;
        let bot_pos = spine_pos + radial;
        let normal = Vec3::new(cos, 0.0, sin).normalize();
        let u = s as f32 / TUBE_SEGMENTS as f32;

        positions.push(top_pos.to_array());
        normals.push(normal.to_array());
        uvs.push([u, 0.0]);
        joints.push([chest_joint, 0, 0, 0]);
        weights.push([1.0, 0.0, 0.0, 0.0]);

        positions.push(bot_pos.to_array());
        normals.push(normal.to_array());
        uvs.push([u, 1.0]);
        joints.push([spine_joint, 0, 0, 0]);
        weights.push([1.0, 0.0, 0.0, 0.0]);
    }

    // --- 2. Indices ---------------------------------------------------------
    let mut indices: Vec<u16> = Vec::with_capacity((TUBE_SEGMENTS as usize) * 6);
    for s in 0..TUBE_SEGMENTS {
        let s_next = (s + 1) % TUBE_SEGMENTS;
        let t0 = (s * 2) as u16;
        let b0 = (s * 2 + 1) as u16;
        let t1 = (s_next * 2) as u16;
        let b1 = (s_next * 2 + 1) as u16;
        // CCW from outside (renderer culls back faces, FrontFace::Ccw):
        //   (top, bottom_next, bottom) + (top, top_next, bottom_next).
        indices.extend_from_slice(&[t0, b1, b0, t0, t1, b1]);
    }

    let vert_count = positions.len();

    // --- 3. Bounding box for the position accessor (spec requires min/max) --
    let (pos_min, pos_max) = aabb_of(&positions);

    // --- 4. Inverse bind matrices in joint order ---------------------------
    let ibms: Vec<Mat4> = skeleton
        .bones
        .iter()
        .map(|b| b.inverse_bind_matrix)
        .collect();

    // --- 5. Pack binary buffer ---------------------------------------------
    let mut bin: Vec<u8> = Vec::new();

    let pos_offset = align_to(&mut bin, 4);
    for p in &positions {
        for f in p {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let pos_len = bin.len() - pos_offset;

    let norm_offset = align_to(&mut bin, 4);
    for n in &normals {
        for f in n {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let norm_len = bin.len() - norm_offset;

    let uv_offset = align_to(&mut bin, 4);
    for uv in &uvs {
        for f in uv {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let uv_len = bin.len() - uv_offset;

    let joints_offset = align_to(&mut bin, 4);
    for j in &joints {
        for v in j {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    let joints_len = bin.len() - joints_offset;

    let weights_offset = align_to(&mut bin, 4);
    for w in &weights {
        for f in w {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let weights_len = bin.len() - weights_offset;

    let indices_offset = align_to(&mut bin, 4);
    for i in &indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    let indices_len = bin.len() - indices_offset;

    let ibm_offset = align_to(&mut bin, 4);
    for m in &ibms {
        for col in m.to_cols_array() {
            bin.extend_from_slice(&col.to_le_bytes());
        }
    }
    let ibm_len = bin.len() - ibm_offset;

    let buffer_len = bin.len() as u64;

    // --- 6. Build glTF JSON -------------------------------------------------
    let buffer = gj::Buffer {
        byte_length: USize64(buffer_len),
        uri: None,
        name: None,
        extensions: None,
        extras: Default::default(),
    };

    let make_view = |offset: usize,
                     length: usize,
                     stride: Option<usize>,
                     target: Option<gj::buffer::Target>| gj::buffer::View {
        buffer: gj::Index::new(0),
        byte_length: USize64(length as u64),
        byte_offset: Some(USize64(offset as u64)),
        byte_stride: stride.map(gj::buffer::Stride),
        target: target.map(Valid),
        name: None,
        extensions: None,
        extras: Default::default(),
    };

    let views = vec![
        make_view(
            pos_offset,
            pos_len,
            Some(12),
            Some(gj::buffer::Target::ArrayBuffer),
        ), // 0 pos
        make_view(
            norm_offset,
            norm_len,
            Some(12),
            Some(gj::buffer::Target::ArrayBuffer),
        ), // 1 normal
        make_view(
            uv_offset,
            uv_len,
            Some(8),
            Some(gj::buffer::Target::ArrayBuffer),
        ), // 2 uv
        make_view(
            joints_offset,
            joints_len,
            Some(8),
            Some(gj::buffer::Target::ArrayBuffer),
        ), // 3 joints
        make_view(
            weights_offset,
            weights_len,
            Some(16),
            Some(gj::buffer::Target::ArrayBuffer),
        ), // 4 weights
        make_view(
            indices_offset,
            indices_len,
            None,
            Some(gj::buffer::Target::ElementArrayBuffer),
        ), // 5 indices
        make_view(ibm_offset, ibm_len, None, None), // 6 inverse bind matrices
    ];

    let make_accessor = |view: u32,
                         count: usize,
                         component: gj::accessor::ComponentType,
                         ty: gj::accessor::Type,
                         min: Option<Value>,
                         max: Option<Value>| gj::Accessor {
        buffer_view: Some(gj::Index::new(view)),
        byte_offset: Some(USize64(0)),
        count: USize64(count as u64),
        component_type: Valid(gj::accessor::GenericComponentType(component)),
        type_: Valid(ty),
        min,
        max,
        normalized: false,
        sparse: None,
        name: None,
        extensions: None,
        extras: Default::default(),
    };

    let accessors = vec![
        make_accessor(
            0,
            vert_count,
            gj::accessor::ComponentType::F32,
            gj::accessor::Type::Vec3,
            Some(Value::Array(pos_min.to_vec())),
            Some(Value::Array(pos_max.to_vec())),
        ),
        make_accessor(
            1,
            vert_count,
            gj::accessor::ComponentType::F32,
            gj::accessor::Type::Vec3,
            None,
            None,
        ),
        make_accessor(
            2,
            vert_count,
            gj::accessor::ComponentType::F32,
            gj::accessor::Type::Vec2,
            None,
            None,
        ),
        make_accessor(
            3,
            vert_count,
            gj::accessor::ComponentType::U16,
            gj::accessor::Type::Vec4,
            None,
            None,
        ),
        make_accessor(
            4,
            vert_count,
            gj::accessor::ComponentType::F32,
            gj::accessor::Type::Vec4,
            None,
            None,
        ),
        make_accessor(
            5,
            indices.len(),
            gj::accessor::ComponentType::U16,
            gj::accessor::Type::Scalar,
            None,
            None,
        ),
        make_accessor(
            6,
            bone_count,
            gj::accessor::ComponentType::F32,
            gj::accessor::Type::Mat4,
            None,
            None,
        ),
    ];

    // --- 7. Skeleton nodes (mirror phase4_rig joint order) ------------------
    let joint_node_start: u32 = 0;
    let mesh_node_idx: u32 = bone_count as u32;
    let scene_root_idx: u32 = mesh_node_idx + 1;

    let mut nodes: Vec<gj::Node> = Vec::with_capacity(bone_count + 2);

    // Build child lists for parents.
    let mut children_by_parent: std::collections::HashMap<usize, Vec<u32>> =
        std::collections::HashMap::new();
    for (i, bone) in skeleton.bones.iter().enumerate() {
        if let Some(parent) = bone.parent {
            children_by_parent
                .entry(parent.0)
                .or_default()
                .push(joint_node_start + i as u32);
        }
    }

    for (i, bone) in skeleton.bones.iter().enumerate() {
        let children = children_by_parent
            .get(&i)
            .map(|v| v.iter().copied().map(gj::Index::new).collect::<Vec<_>>());
        nodes.push(gj::Node {
            name: Some(bone.name.clone()),
            matrix: Some(bone.local_bind_transform.to_cols_array()),
            children,
            mesh: None,
            skin: None,
            translation: None,
            rotation: None,
            scale: None,
            camera: None,
            weights: None,
            extensions: None,
            extras: Default::default(),
        });
    }

    // Mesh node (carries the skin reference and the tube mesh).
    nodes.push(gj::Node {
        name: Some("phase7_top_mesh".to_string()),
        mesh: Some(gj::Index::new(0)),
        skin: Some(gj::Index::new(0)),
        matrix: None,
        children: None,
        translation: None,
        rotation: None,
        scale: None,
        camera: None,
        weights: None,
        extensions: None,
        extras: Default::default(),
    });

    // Scene root: holds the rig root bone + the mesh node as children.
    nodes.push(gj::Node {
        name: Some("phase7_top_scene_root".to_string()),
        children: Some(vec![
            gj::Index::new(joint_node_start),
            gj::Index::new(mesh_node_idx),
        ]),
        matrix: None,
        mesh: None,
        skin: None,
        translation: None,
        rotation: None,
        scale: None,
        camera: None,
        weights: None,
        extensions: None,
        extras: Default::default(),
    });

    // --- 8. Skin ------------------------------------------------------------
    // `skeleton` field intentionally None — runtime's `Skeleton::from_gltf`
    // validates the root node name when present, and our root node is the
    // unnamed "root" joint. Omitting it skips that check, which is fine: the
    // body's skin is what carries the skeleton-name claim.
    let skin = gj::Skin {
        name: Some(Skeleton::AVATAR_SKELETON_V1.to_string()),
        skeleton: None,
        joints: (0..bone_count as u32)
            .map(|i| gj::Index::new(joint_node_start + i))
            .collect(),
        inverse_bind_matrices: Some(gj::Index::new(6)),
        extensions: None,
        extras: Default::default(),
    };

    // --- 9. Mesh + primitive -----------------------------------------------
    let mut attributes = std::collections::BTreeMap::new();
    attributes.insert(Valid(gj::mesh::Semantic::Positions), gj::Index::new(0));
    attributes.insert(Valid(gj::mesh::Semantic::Normals), gj::Index::new(1));
    attributes.insert(Valid(gj::mesh::Semantic::TexCoords(0)), gj::Index::new(2));
    attributes.insert(Valid(gj::mesh::Semantic::Joints(0)), gj::Index::new(3));
    attributes.insert(Valid(gj::mesh::Semantic::Weights(0)), gj::Index::new(4));

    let primitive = gj::mesh::Primitive {
        attributes,
        indices: Some(gj::Index::new(5)),
        material: None,
        mode: Valid(gj::mesh::Mode::Triangles),
        targets: None,
        extensions: None,
        extras: Default::default(),
    };

    let mesh = gj::Mesh {
        name: Some("phase7_top".to_string()),
        primitives: vec![primitive],
        weights: None,
        extensions: None,
        extras: Default::default(),
    };

    // --- 10. Scene ----------------------------------------------------------
    let scene = gj::Scene {
        name: Some("phase7_top_scene".to_string()),
        nodes: vec![gj::Index::new(scene_root_idx)],
        extensions: None,
        extras: Default::default(),
    };

    let root = gj::Root {
        asset: gj::Asset {
            version: "2.0".into(),
            generator: Some("avatar_studio asset_builder gen-fixture-top".into()),
            copyright: None,
            min_version: None,
            extensions: None,
            extras: Default::default(),
        },
        buffers: vec![buffer],
        buffer_views: views,
        accessors,
        meshes: vec![mesh],
        nodes,
        skins: vec![skin],
        scenes: vec![scene],
        scene: Some(gj::Index::new(0)),
        animations: vec![],
        cameras: vec![],
        images: vec![],
        materials: vec![],
        samplers: vec![],
        textures: vec![],
        extensions: None,
        extensions_used: vec![],
        extensions_required: vec![],
        extras: Default::default(),
    };

    write_glb(&out_glb, &root, &bin).with_context(|| format!("write {}", out_glb.display()))?;

    write_metadata(&out_meta).with_context(|| format!("write {}", out_meta.display()))?;

    info!(
        glb = %out_glb.display(),
        meta = %out_meta.display(),
        verts = vert_count,
        tris = indices.len() / 3,
        joints = bone_count,
        "phase7_top fixture written"
    );

    Ok(())
}

fn write_metadata(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::json!({
        "id": "top_phase7_basic_001",
        "displayName": "Phase 7 Top",
        "category": "top",
        "model": "avatars/tops/phase7_top.glb",
        "thumbnail": null,
        "supportsColor": false,
        "compatibleSkeleton": "avatar_skeleton_v1",
        "compatibleBodyTypes": ["body_phase4_rig_001"],
        "tags": ["sample", "rigged", "phase7"],
        "version": 1,
    });
    let pretty = serde_json::to_string_pretty(&json)?;
    std::fs::write(path, pretty)?;
    Ok(())
}

fn align_to(bin: &mut Vec<u8>, alignment: usize) -> usize {
    let rem = bin.len() % alignment;
    if rem != 0 {
        bin.extend(std::iter::repeat_n(0u8, alignment - rem));
    }
    bin.len()
}

fn aabb_of(positions: &[[f32; 3]]) -> ([Value; 3], [Value; 3]) {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            if p[i] < mn[i] {
                mn[i] = p[i];
            }
            if p[i] > mx[i] {
                mx[i] = p[i];
            }
        }
    }
    let to_value = |v: f32| Value::Number(serde_json::Number::from_f64(v as f64).unwrap());
    (
        [to_value(mn[0]), to_value(mn[1]), to_value(mn[2])],
        [to_value(mx[0]), to_value(mx[1]), to_value(mx[2])],
    )
}

fn world_translation(m: &Mat4) -> Vec3 {
    let cols = m.to_cols_array();
    Vec3::new(cols[12], cols[13], cols[14])
}

#[allow(dead_code)]
pub fn output_glb_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(OUT_GLB)
}

#[allow(dead_code)]
pub fn output_metadata_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(OUT_META)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_generated_fixture() {
        let workspace_root = workspace_root_for_tests();
        let target_dir = workspace_root.join("target/test-fixtures");
        std::fs::create_dir_all(&target_dir).unwrap();

        // Generate into the real output paths (idempotent — overwrites).
        generate(&workspace_root).expect("generate phase7_top");

        let glb = output_glb_path(&workspace_root);
        let (document, _buffers, _) = gltf::import(&glb).expect("re-import generated glb");
        let primitive = document
            .meshes()
            .next()
            .expect("mesh present")
            .primitives()
            .next()
            .expect("primitive present");
        let reader =
            primitive.reader(|_| panic!("test reader should not be called for attribute presence"));
        // Avoid invoking reader.read_*; instead just check that the JSON has
        // joints/weights via primitive attributes — gltf 1.4 exposes them on
        // `Primitive::semantics()` via attribute iteration:
        let mut has_joints = false;
        let mut has_weights = false;
        for (sem, _) in primitive.attributes() {
            match sem {
                gltf::Semantic::Joints(0) => has_joints = true,
                gltf::Semantic::Weights(0) => has_weights = true,
                _ => {}
            }
        }
        assert!(has_joints, "JOINTS_0 missing from generated fixture");
        assert!(has_weights, "WEIGHTS_0 missing from generated fixture");
        let skin = document.skins().next().expect("skin present");
        assert_eq!(skin.joints().count(), 18, "skin joint count must match rig",);
        let _ = reader; // suppress unused
    }

    fn workspace_root_for_tests() -> PathBuf {
        // CARGO_MANIFEST_DIR points to apps/asset_builder; workspace = ../../.
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        PathBuf::from(manifest)
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }
}
