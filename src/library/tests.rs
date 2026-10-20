//! 验证音频库分类路径与递归变更规则。
use super::*;

// 构造最小配置条目，测试只关注分类路径规则，不访问磁盘或解码器。
fn entry(name: &str, category: &str) -> AudioEntry {
    AudioEntry {
        name: name.into(),
        path: format!("{name}.wav"),
        icon_path: None,
        icon_glyph: "\u{EC4F}".into(),
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
// 显示索引映射回原始数组，并包含所选分类的全部后代。
fn visible_index_maps_category_descendants() {
    // 第一项故意放在分类外，用来确认过滤后的可见序号不会直接当作原数组下标。
    let entries = vec![
        entry("Railgun", "其他"),
        entry("Railgun", "游戏/武器"),
        entry("Rocket", "游戏/武器"),
        entry("Railgun special", "游戏/武器/特殊"),
    ];

    // “游戏”应包含武器及其后代，返回值仍必须指向 entries 中的真实位置。
    assert_eq!(visible_entry_index(&entries, "游戏", 0), Some(1));
    assert_eq!(visible_entry_index(&entries, "游戏", 1), Some(2));
    assert_eq!(visible_entry_index(&entries, "游戏", 2), Some(3));
    assert_eq!(visible_entry_index(&entries, "游戏", 3), None);
}

#[test]
// 删除“全部”只清空条目，保留其他分类节点供后续重新导入。
fn deleting_all_removes_library_entries_and_only_its_node() {
    // “全部”是清空条目的操作入口，不应连带删除用户维护的其他分类节点。
    let mut categories = vec!["全部".into(), "未分类".into(), "游戏".into()];
    let mut entries = vec![entry("a", "未分类"), entry("b", "游戏")];

    // 删除后分类树仍保留非虚拟节点，下一次导入可继续使用这些路径。
    delete_category(&mut categories, &mut entries, "全部");

    assert_eq!(categories, vec!["未分类", "游戏"]);
    assert!(entries.is_empty());
}

#[test]
// 删除“未分类”会连同其后代条目一起移除，不影响普通分类。
fn deleting_uncategorized_removes_only_its_entries_and_descendants() {
    // 同时准备根节点和后代条目，覆盖递归过滤而非仅比较精确路径。
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

    // 未分类删除只移除该子树，普通分类中的音频必须继续存在。
    delete_category(&mut categories, &mut entries, "未分类");

    assert_eq!(categories, vec!["全部", "游戏"]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "c");
}

#[test]
// 没有条目时，路径展开不应凭空恢复已删除的空根节点。
fn category_paths_do_not_restore_deleted_empty_roots() {
    // 没有条目时只能返回显式分类；删除后不存在的空根不能由推导逻辑复活。
    assert!(category_paths(&[], &[]).is_empty());
    assert_eq!(category_paths(&["游戏".into()], &[]), vec!["游戏"]);
}

#[test]
// 删除真实分类后，条目退回未分类而不是被物理删除。
fn deleting_regular_category_moves_entries_to_uncategorized() {
    // 普通分类删除保留音频数据，只把受影响条目的归属改为未分类。
    let mut categories = vec!["游戏".into(), "游戏/武器".into(), "其他".into()];
    let mut entries = vec![entry("a", "游戏"), entry("b", "游戏/武器")];

    delete_category(&mut categories, &mut entries, "游戏");

    assert_eq!(categories, vec!["其他"]);
    assert!(entries.iter().all(|entry| entry.category == "未分类"));
}
#[test]
// 导入分类使用当前选择；空选择采用未分类兜底。
fn import_uses_selected_category_or_uncategorized_fallback() {
    // 非空选择原样保留层级路径，空字符串才触发导入兜底。
    assert_eq!(import_category("游戏/武器"), "游戏/武器");
    assert_eq!(import_category(""), "未分类");
}
#[test]
// 空树的新分类应成为顶层节点。
fn new_category_from_empty_tree_is_a_root() {
    // 无父节点可挂载时，默认名称直接作为顶层路径返回。
    assert_eq!(new_category(&[], &[], ""), "未命名分类");
}
#[test]
// 新分类在父节点下生成，并通过编号避免同级路径冲突。
fn new_categories_preserve_parent_child_and_sibling_paths() {
    // 分别验证顶层、已有父节点和已有子节点下的路径拼接规则。
    let mut categories = vec!["游戏".into(), "游戏/枪械".into()];
    assert_eq!(new_category(&categories, &[], ""), "未命名分类");
    assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类");
    assert_eq!(
        new_category(&categories, &[], "游戏/枪械"),
        "游戏/枪械/未命名分类"
    );

    categories.push("游戏/未命名分类".into());
    // 同级默认名称已占用时，后缀从 2 开始递增而不覆盖原节点。
    assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类 2");
}
#[test]
// 路径展开必须把子分类紧跟在对应父节点下。
fn category_paths_keep_children_under_their_parent() {
    // 输入顺序先出现其他根节点，期望结果仍按父节点分组而不是简单排序。
    let categories = vec![
        "分类1".into(),
        "分类2".into(),
        "分类3".into(),
        "分类1/子分类".into(),
    ];

    // 期望的前序序列把子分类紧跟父节点，供树形显示和拖放索引共同使用。
    assert_eq!(
        category_paths(&categories, &[]),
        vec!["分类1", "分类1/子分类", "分类2", "分类3"]
    );
}
#[test]
// 移入目标会重写整棵子树及其中条目的分类路径。
fn moving_category_into_target_moves_descendants_and_audio_entries() {
    // 条目声明的深层路径会被一并纳入树，移动根节点因此必须携带隐含子树。
    let mut categories = vec!["分类1".into(), "分类2".into(), "分类3".into()];
    let mut entries = vec![entry("音效", "分类1/子项"), entry("目标", "分类2")];

    let destination = move_category(
        &mut categories,
        &mut entries,
        "分类1",
        "分类2",
        CategoryDropPosition::Into,
    );

    // 目标路径由目标根加源叶名称组成；分类树和条目归属应同时改写。
    assert_eq!(destination.as_deref(), Some("分类2/分类1"));
    assert_eq!(
        category_paths(&categories, &entries),
        vec!["分类2", "分类2/分类1", "分类2/分类1/子项", "分类3"]
    );
    assert_eq!(entries[0].category, "分类2/分类1/子项");
    assert_eq!(entries[1].category, "分类2");
}

#[test]
// 移到同级之前只改变兄弟顺序，不丢失父路径。
fn moving_category_before_sibling_reorders_tree_without_losing_parent() {
    // 源节点移到已有父节点的兄弟位置，只改变同级顺序，不改变父级层次。
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

    // 目的地应位于父类下，且原有第一项、第二项的相对关系保持可见。
    assert_eq!(destination.as_deref(), Some("父类/其他"));
    assert_eq!(
        category_paths(&categories, &[]),
        vec!["父类", "父类/第一项", "父类/其他", "父类/第二项"]
    );
}

#[test]
// 不能把节点移入自身后代，也不能覆盖已有同名目的地。
fn moving_category_rejects_descendant_cycles_and_name_collisions() {
    let mut categories = vec!["甲".into(), "甲/子".into(), "乙".into(), "乙/甲".into()];
    // 记录原始分类，验证所有非法移动都在纯校验阶段被拒绝且不产生副作用。
    let original = categories.clone();

    // 目标是源后代时会形成循环；目标已有同名路径时也不能覆盖现有节点。
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
// 移到同级之后应跳过目标的完整后代子树。
fn moving_category_after_sibling_places_it_after_sibling_subtree() {
    // 第二项带有后代，After 必须越过整棵目标子树，而不是插入到后代中间。
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

    // 结果顺序用于确认插入点位于目标子树末尾之后，而非仅位于目标节点之后。
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
// 移动后的新路径仍可作为删除入口，并把条目按删除规则回退。
fn moved_category_can_be_deleted_at_its_new_path() {
    // 返回的新根路径仍可作为删除入口，验证移动后的索引和递归规则彼此衔接。
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

    // 删除移动后的整棵子树后，条目回退未分类，未涉及的兄弟分类继续保留。
    delete_category(&mut categories, &mut entries, &moved);

    assert!(!category_paths(&categories, &entries).contains(&moved));
    assert_eq!(entries[0].category, "未分类");
    assert!(categories.contains(&"分类2".into()));
    assert!(categories.contains(&"分类3".into()));
}

#[test]
// 重命名必须同步更新后代节点和条目的分类路径。
fn renaming_category_updates_descendants_and_audio_paths() {
    // 同时检查分类节点与条目路径，确保重命名覆盖显式后代和实际归属。
    let mut categories = vec!["旧根".into(), "旧根/子项".into(), "其他".into()];
    let mut entries = vec![entry("音效", "旧根/子项")];

    // 只替换旧根前缀，后代相对路径“/子项”必须原样保留。
    rename_category(&mut categories, &mut entries, "旧根", "新根");

    assert_eq!(categories, vec!["新根", "新根/子项", "其他"]);
    assert_eq!(entries[0].category, "新根/子项");
}
