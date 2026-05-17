# Asset Pipeline

Avatar Studio keeps authored/sample assets under `assets/processed/` and mirrors
their metadata into `user_data/asset_catalog.sqlite` at app startup.

## Layout

- `assets/processed/avatars/bodies/` — body/static body GLBs.
- `assets/processed/avatars/tops/`, `bottoms/`, `shoes/`, `hairs/`, `hats/`,
  `glasses/`, `accessories/` — wearable GLBs.
- `assets/processed/metadata/<asset-id>.json` — one sidecar per asset.
- `assets/processed/thumbnails/<asset-id>.png` — optional UI thumbnail.
- `user_data/asset_catalog.sqlite` — generated catalog cache; safe to rebuild.

Metadata JSON mirrors `assets::AssetMeta`:

```json
{
  "id": "top_phase7_basic_001",
  "displayName": "Phase 7 Top",
  "category": "top",
  "model": "avatars/tops/phase7_top.glb",
  "thumbnail": "thumbnails/top_phase7_basic_001.png",
  "supportsColor": false,
  "compatibleSkeleton": "avatar_skeleton_v1",
  "compatibleBodyTypes": ["body_phase4_rig_001"],
  "tags": ["sample", "rigged"],
  "version": 1
}
```

## Asset Builder

Run commands from the workspace root.

- `cargo run --bin asset_builder -- import <file.glb> --category top --id my_top --thumb`
  copies a GLB, writes metadata, renders a thumbnail, and upserts the catalog.
- `cargo run --bin asset_builder -- validate <file.glb> --category top`
  performs read-only GLB inspection.
- `cargo run --bin asset_builder -- thumbnail <asset-id>`
  regenerates a thumbnail for an existing catalog asset.
- `cargo run --bin asset_builder -- list --category top`
  lists catalog rows.
- `cargo run --bin asset_builder -- gen-fixture-top`
  regenerates the skinned Phase 7 sample top.
- `cargo run --bin asset_builder -- gen-fixture-pack`
  regenerates the Phase 17 multi-category sample pack.

The app also rescans `assets/processed/metadata/` on startup and upserts every
valid sidecar into SQLite, so checked-in metadata is the source of truth.

## Phase 17 Fixture Pack

`gen-fixture-pack` creates six coverage assets:

- `bottom_phase17_basic_001`
- `shoes_phase17_basic_001`
- `hair_phase17_basic_001`
- `hat_phase17_basic_001`
- `glasses_phase17_basic_001`
- `accessory_phase17_basic_001`

These fixtures reuse the skinned Phase 7 top mesh so every slot can be equipped
against `body_phase4_rig_001` immediately. They are not final art. Replacing
them later should preserve ids when possible so saves and tests continue to
load.

## Validation Expectations

For MVP rigged wearables:

- GLB loads through `renderer::load_glb`.
- `JOINTS_0` and `WEIGHTS_0` are present for skinned mesh primitives.
- `compatibleSkeleton` is `avatar_skeleton_v1`.
- `compatibleBodyTypes` includes `body_phase4_rig_001` unless intentionally
  generic.
- Default colors are sRGB floats in `0.0..=1.0`.
- Thumbnail path points under `assets/processed/thumbnails/`.

Static body assets such as Rubber Duck may omit skeleton and skinning data.
