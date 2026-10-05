//! 验证音频库分类路径与递归变更规则。
use super::*;

fn entry(name: &str, category: &str) -> AudioEntry {
    AudioEntry {
        name: name.into(),
        path: format!("{name}.wav"),
        category: category.into(),
        duration_hns: 0,
        file_size_bytes: 0,
        sample_rate: 0,
        channels: 0,
        loop_playback: false,
        hotkey: None,
        hotkey_binding: None,
    }
}

#[test]
fn visible_index_maps_category_descendants() {
    let entries = vec![
        entry("Railgun", "其他"),
        entry("Railgun", "游戏/武器"),
        entry("Rocket", "游戏/武器"),
        entry("Railgun special", "游戏/武器/特殊"),
    ];

    assert_eq!(visible_entry_index(&entries, "游戏", 0), Some(1));
    assert_eq!(visible_entry_index(&entries, "游戏", 1), Some(2));
    assert_eq!(visible_entry_index(&entries, "游戏", 2), Some(3));
    assert_eq!(visible_entry_index(&entries, "游戏", 3), None);
}

#[test]
fn deleting_all_removes_library_entries_and_only_its_node() {
    let mut categories = vec!["全部".into(), "未分类".into(), "游戏".into()];
    let mut entries = vec![entry("a", "未分类"), entry("b", "游戏")];

    delete_category(&mut categories, &mut entries, "全部");

    assert_eq!(categories, vec!["未分类", "游戏"]);
    assert!(entries.is_empty());
}

#[test]
fn deleting_uncategorized_removes_only_its_entries_and_descendants() {
    let mut categories = vec![
        "全部".into(),
        "未分类".into(),
        "未分类/临时".into(),
        "游戏".into(),
    ];
    let mut entries = vec![
        entry("a", "未分类"),
        entry("b", "未分类/临时"),
        entry("c", "游戏"),
    ];

    delete_category(&mut categories, &mut entries, "未分类");

    assert_eq!(categories, vec!["全部", "游戏"]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "c");
}

#[test]
fn category_paths_do_not_restore_deleted_empty_roots() {
    assert!(category_paths(&[], &[]).is_empty());
    assert_eq!(category_paths(&["游戏".into()], &[]), vec!["游戏"]);
}

#[test]
fn deleting_regular_category_moves_entries_to_uncategorized() {
    let mut categories = vec!["游戏".into(), "游戏/武器".into(), "其他".into()];
    let mut entries = vec![entry("a", "游戏"), entry("b", "游戏/武器")];

    delete_category(&mut categories, &mut entries, "游戏");

    assert_eq!(categories, vec!["其他"]);
    assert!(entries.iter().all(|entry| entry.category == "未分类"));
}
#[test]
fn import_uses_selected_category_or_uncategorized_fallback() {
    assert_eq!(import_category("游戏/武器"), "游戏/武器");
    assert_eq!(import_category(""), "未分类");
}
#[test]
fn new_category_from_empty_tree_is_a_root() {
    assert_eq!(new_category(&[], &[], ""), "未命名分类");
}
#[test]
fn new_categories_preserve_parent_child_and_sibling_paths() {
    let mut categories = vec!["游戏".into(), "游戏/枪械".into()];
    assert_eq!(new_category(&categories, &[], ""), "未命名分类");
    assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类");
    assert_eq!(
        new_category(&categories, &[], "游戏/枪械"),
        "游戏/枪械/未命名分类"
    );

    categories.push("游戏/未命名分类".into());
    assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类 2");
}
#[test]
fn category_paths_keep_children_under_their_parent() {
    let categories = vec![
        "分类1".into(),
        "分类2".into(),
        "分类3".into(),
        "分类1/子分类".into(),
    ];

    assert_eq!(
        category_paths(&categories, &[]),
        vec!["分类1", "分类1/子分类", "分类2", "分类3"]
    );
}
#[test]
fn moving_category_into_target_moves_descendants_and_audio_entries() {
    let mut categories = vec!["分类1".into(), "分类2".into(), "分类3".into()];
    let mut entries = vec![entry("音效", "分类1/子项"), entry("目标", "分类2")];

    let destination = move_category(
        &mut categories,
        &mut entries,
        "分类1",
        "分类2",
        CategoryDropPosition::Into,
    );

    assert_eq!(destination.as_deref(), Some("分类2/分类1"));
    assert_eq!(
        category_paths(&categories, &entries),
        vec!["分类2", "分类2/分类1", "分类2/分类1/子项", "分类3"]
    );
    assert_eq!(entries[0].category, "分类2/分类1/子项");
    assert_eq!(entries[1].category, "分类2");
}

#[test]
fn moving_category_before_sibling_reorders_tree_without_losing_parent() {
    let mut categories = vec![
        "父类".into(),
        "父类/第一项".into(),
        "父类/第二项".into(),
        "其他".into(),
    ];

    let destination = move_category(
        &mut categories,
        &mut [],
        "其他",
        "父类/第二项",
        CategoryDropPosition::Before,
    );

    assert_eq!(destination.as_deref(), Some("父类/其他"));
    assert_eq!(
        category_paths(&categories, &[]),
        vec!["父类", "父类/第一项", "父类/其他", "父类/第二项"]
    );
}

#[test]
fn moving_category_rejects_descendant_cycles_and_name_collisions() {
    let mut categories = vec!["甲".into(), "甲/子".into(), "乙".into(), "乙/甲".into()];
    let original = categories.clone();

    assert!(!can_move_category(
        &categories,
        &[],
        "甲",
        "甲/子",
        CategoryDropPosition::Into,
    ));
    assert!(
        move_category(
            &mut categories,
            &mut [],
            "甲",
            "乙",
            CategoryDropPosition::Into,
        )
        .is_none()
    );
    assert_eq!(categories, original);
}
#[test]
fn moving_category_after_sibling_places_it_after_sibling_subtree() {
    let mut categories = vec![
        "父类".into(),
        "父类/第一项".into(),
        "父类/第二项".into(),
        "父类/第二项/后代".into(),
        "父类/第三项".into(),
    ];

    assert_eq!(
        move_category(
            &mut categories,
            &mut [],
            "父类/第一项",
            "父类/第二项",
            CategoryDropPosition::After,
        )
        .as_deref(),
        Some("父类/第一项")
    );

    assert_eq!(
        category_paths(&categories, &[]),
        vec![
            "父类",
            "父类/第二项",
            "父类/第二项/后代",
            "父类/第一项",
            "父类/第三项",
        ]
    );
}
#[test]
fn moved_category_can_be_deleted_at_its_new_path() {
    let mut categories = vec!["分类1".into(), "分类2".into(), "分类3".into()];
    let mut entries = vec![entry("子类音效", "分类1/子类")];
    let moved = move_category(
        &mut categories,
        &mut entries,
        "分类1",
        "分类2",
        CategoryDropPosition::Into,
    )
    .expect("valid drag target");

    delete_category(&mut categories, &mut entries, &moved);

    assert!(!category_paths(&categories, &entries).contains(&moved));
    assert_eq!(entries[0].category, "未分类");
    assert!(categories.contains(&"分类2".into()));
    assert!(categories.contains(&"分类3".into()));
}

#[test]
fn renaming_category_updates_descendants_and_audio_paths() {
    let mut categories = vec!["旧根".into(), "旧根/子项".into(), "其他".into()];
    let mut entries = vec![entry("音效", "旧根/子项")];

    rename_category(&mut categories, &mut entries, "旧根", "新根");

    assert_eq!(categories, vec!["新根", "新根/子项", "其他"]);
    assert_eq!(entries[0].category, "新根/子项");
}
