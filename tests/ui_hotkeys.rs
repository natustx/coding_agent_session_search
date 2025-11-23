use coding_agent_search::ui::tui::footer_legend;

#[test]
fn footer_mentions_editor_and_clear_keys() {
    let long = footer_legend(true);
    assert!(long.contains("o open"));
    assert!(long.contains("A"));
    assert!(long.contains("W"));
    assert!(long.contains("F"));
}
