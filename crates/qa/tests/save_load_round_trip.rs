//! Phase 16 integration test: a fully-equipped avatar round-trips
//! losslessly through `CharacterStore` save → load.

use std::time::{SystemTime, UNIX_EPOCH};

use avatar::{Avatar, AvatarSave, CharacterStore, Expression, Slot};

fn tmp_store() -> CharacterStore {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!("qa_save_load_{}_{}", std::process::id(), unique));
    CharacterStore::new(root)
}

#[test]
fn rigged_body_with_top_color_and_expression_round_trips() {
    let store = tmp_store();
    let mut avatar = Avatar {
        body_type: "body_phase4_rig_001".into(),
        ..Avatar::default()
    };
    avatar.equip(Slot::Body, "body_phase4_rig_001");
    avatar.equip(Slot::Top, "top_phase7_basic_001");
    avatar.set_slot_color(Slot::Body, [0.62, 0.41, 0.28]);
    avatar.set_slot_color(Slot::Top, [0.10, 0.20, 0.80]);
    avatar.expression = Expression::Happy;

    let save = AvatarSave::from_parts(
        "qa_char_001",
        "QA Test Character",
        &avatar,
        Some("idle".into()),
        "1".into(),
        "2".into(),
    );
    let written = store.save(&save).expect("save");
    assert!(written.exists());

    let loaded = store.load("qa_char_001").expect("load");
    assert_eq!(loaded.id, "qa_char_001");
    assert_eq!(loaded.name, "QA Test Character");
    assert_eq!(loaded.base_body, "body_phase4_rig_001");
    assert_eq!(loaded.body_type, "body_phase4_rig_001");
    assert_eq!(loaded.expression, Expression::Happy);
    assert_eq!(loaded.animation.as_deref(), Some("idle"));

    let restored = loaded.into_avatar().expect("restore");
    assert_eq!(restored.equipped(Slot::Body), Some("body_phase4_rig_001"));
    assert_eq!(restored.equipped(Slot::Top), Some("top_phase7_basic_001"));
    assert_eq!(restored.slot_color(Slot::Top), Some([0.10, 0.20, 0.80]));
    assert_eq!(restored.expression, Expression::Happy);
}

#[test]
fn static_body_without_skin_round_trips() {
    let store = tmp_store();
    let mut avatar = Avatar {
        body_type: "body_duck_001".into(),
        ..Avatar::default()
    };
    avatar.equip(Slot::Body, "body_duck_001");

    let save = AvatarSave::from_parts(
        "qa_duck_001",
        "QA Duck",
        &avatar,
        None,
        "10".into(),
        "20".into(),
    );
    store.save(&save).expect("save duck");
    let loaded = store.load("qa_duck_001").expect("load duck");
    let restored = loaded.into_avatar().expect("restore duck");
    assert_eq!(restored.equipped(Slot::Body), Some("body_duck_001"));
    assert_eq!(restored.expression, Expression::default());
    assert!(loaded.animation.is_none());
}

#[test]
fn list_returns_newest_first() {
    let store = tmp_store();
    let avatar = Avatar::default();
    let older = AvatarSave::from_parts("old", "Older", &avatar, None, "1".into(), "1".into());
    let newer = AvatarSave::from_parts("new", "Newer", &avatar, None, "2".into(), "2".into());
    store.save(&older).unwrap();
    store.save(&newer).unwrap();
    let rows = store.list().unwrap();
    assert!(rows.len() >= 2);
    assert_eq!(rows[0].id, "new");
    assert_eq!(rows[1].id, "old");
}
