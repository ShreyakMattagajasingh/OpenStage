# Skeleton standard — `avatar_skeleton_v1`

All rigged avatar assets MUST use this skeleton. The asset_builder rejects any
GLB whose root skeleton name or bone names disagree.

## Conventions

- **Up axis:** +Y.
- **Forward axis:** +Z (the avatar's chest faces +Z in bind pose).
- **Units:** meters.
- **Origin:** centered between feet at ground level (`y = 0`).
- **Height:** roughly 1.7 units in bind pose.
- **Hand-front-back:** A-pose (arms angled ~30° from vertical), palms facing +Z.

## MVP bone list (18 bones)

```
root
├── hips
│   ├── spine
│   │   └── chest
│   │       ├── neck
│   │       │   └── head
│   │       ├── upperarm_l → lowerarm_l → hand_l
│   │       └── upperarm_r → lowerarm_r → hand_r
│   ├── upperleg_l → lowerleg_l → foot_l
│   └── upperleg_r → lowerleg_r → foot_r
```

## Reserved post-MVP bones

These names are reserved — assets MAY include them but the M1 runtime ignores
them. The asset_builder warns if extra unknown bones appear.

```
jaw  eye_l  eye_r  shoulder_l  shoulder_r  toe_l  toe_r
finger_<thumb|index|middle|ring|pinky>_<1|2|3>_<l|r>
```

## Attachment points

Accessories declare an `attachBone` in their asset metadata. The runtime will
parent the asset's root transform to that bone's world matrix.

| Slot        | attachBone   |
| ----------- | ------------ |
| `hat`       | `head`       |
| `glasses`   | `head`       |
| `earrings`  | `head`       |
| `necklace`  | `chest`      |
| `watch`     | `lowerarm_l` |
| `backpack`  | `chest`      |
| `handheld`  | `hand_r`     |
