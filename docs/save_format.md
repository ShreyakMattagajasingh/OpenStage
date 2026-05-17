# Avatar Save Format - Schema v1

Each character is one file: `user_data/characters/<id>.json`.
Exported PNGs live under `user_data/exports/`; agent/debug screenshots live
under `user_data/debug_screenshots/`.

The save records choices, not baked geometry. Loading re-resolves slot assets
through the catalog, so any update to an asset propagates automatically.

```json
{
  "schemaVersion": 1,
  "id": "char_001",
  "name": "Alex",
  "createdAt": "1778856049000",
  "updatedAt": "1778856049000",

  "baseBody": "body_phase4_rig_001",
  "bodyType": "body_phase4_rig_001",
  "skinTone": [0.62, 0.41, 0.28],

  "slots": {
    "top": "top_phase7_basic_001"
  },

  "colors": {
    "body": [0.62, 0.41, 0.28],
    "top": [0.10, 0.20, 0.80]
  },

  "expression": "happy",
  "animation": "idle"
}
```

## Field Rules

| Field | Notes |
| ----- | ----- |
| `schemaVersion` | Reject mismatches; future versions add a migration step. |
| `id` | Stable, sanitized id. Generated as `char_<unix_ms>` on new save. |
| `name` | Free text, max 64 chars. |
| `baseBody` | Body `assetId`, loaded first. |
| `bodyType` | Body compatibility id. In the current app this matches the body asset id. |
| `slots.*` | Wearable slot asset ids. Body is stored separately in `baseBody`. |
| `colors.*` | User-facing sRGB `[r,g,b]` in `0..1`, keyed by slot. GPU upload converts to linear. |
| `expression` | One of the expression preset ids. |
| `animation` | Optional clip id/name, currently `idle`. |

## Resolving On Load

```text
missing assetId   -> log warning, skip slot, surface an error in the side panel
missing color     -> fall back to asset defaultColor / slot default tint
incompatible slot -> block that wearable, keep the rest of the character loaded
```

## Current Phase 10 Behavior

- Saving is enabled only in avatar mode with a body equipped.
- `Save` creates or overwrites a schema-v1 JSON file.
- The side-panel Gallery lists saved JSON files sorted by `updatedAt` descending.
- Loading resolves every asset through the current catalog: body first, then
  wearables, then colors and expression.
