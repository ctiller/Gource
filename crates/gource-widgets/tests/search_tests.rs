use glam::UVec2;
use gource_draw::{FontId, Gfx};
use gource_widgets::search::{SearchItemKind, SearchWidget};

#[test]
fn test_search_widget_lifecycle() {
    let mut widget = SearchWidget::new(FontId(1), 1.0);
    assert!(!widget.is_active());
    assert_eq!(widget.alpha, 0.0);

    widget.show();
    assert!(widget.is_active());

    widget.logic(0.2);
    assert!(widget.alpha > 0.99);

    widget.toggle();
    assert!(!widget.visible);
    widget.logic(0.2);
    assert!(widget.alpha < 0.01);
    assert!(!widget.is_active());
}

#[test]
fn test_search_widget_query_and_results() {
    let mut widget = SearchWidget::new(FontId(1), 1.0);
    widget.show();

    let files = [
        (1u64, "main.rs", "src/main.rs"),
        (2u64, "lib.rs", "src/lib.rs"),
        (3u64, "search.rs", "src/search.rs"),
    ];
    let dirs = [(10u64, "src"), (11u64, "tests")];
    let users = [(100u64, "alice"), (101u64, "bob")];

    // Filter with empty query
    widget.update_filter(
        files.iter().map(|(id, n, p)| (*id, *n, *p)),
        dirs.iter().map(|(id, p)| (*id, *p)),
        users.iter().map(|(id, n)| (*id, *n)),
    );
    assert!(widget.results.is_empty());

    // Type query: "lib"
    widget.push_char('l');
    widget.push_char('i');
    widget.push_char('b');
    assert_eq!(widget.query, "lib");

    widget.update_filter(
        files.iter().map(|(id, n, p)| (*id, *n, *p)),
        dirs.iter().map(|(id, p)| (*id, *p)),
        users.iter().map(|(id, n)| (*id, *n)),
    );
    assert_eq!(widget.results.len(), 1);
    assert_eq!(widget.results[0].kind, SearchItemKind::File);
    assert_eq!(widget.results[0].name, "lib.rs");
    assert_eq!(widget.selected_result().unwrap().id, 2);

    // Backspace
    widget.backspace();
    assert_eq!(widget.query, "li");

    // Match user "alice"
    widget.update_filter(
        files.iter().map(|(id, n, p)| (*id, *n, *p)),
        dirs.iter().map(|(id, p)| (*id, *p)),
        users.iter().map(|(id, n)| (*id, *n)),
    );
    // "alice" has "li"
    assert!(widget.results.iter().any(|r| r.name == "alice"));
}

#[test]
fn test_search_widget_navigation() {
    let mut widget = SearchWidget::new(FontId(1), 1.0);
    widget.show();

    let files = [
        (1u64, "file1.rs", "src/file1.rs"),
        (2u64, "file2.rs", "src/file2.rs"),
        (3u64, "file3.rs", "src/file3.rs"),
    ];

    widget.push_char('f');
    widget.update_filter(
        files.iter().map(|(id, n, p)| (*id, *n, *p)),
        std::iter::empty(),
        std::iter::empty(),
    );
    assert_eq!(widget.results.len(), 3);
    assert_eq!(widget.selected_index, 0);

    widget.select_next();
    assert_eq!(widget.selected_index, 1);
    widget.select_next();
    assert_eq!(widget.selected_index, 2);
    widget.select_next();
    assert_eq!(widget.selected_index, 0); // wraps around

    widget.select_prev();
    assert_eq!(widget.selected_index, 2); // wraps back
}

#[test]
fn test_search_widget_draw_headless() {
    let mut gfx = Gfx::new();
    let mut list = gource_draw::DrawList::new(UVec2::new(800, 600));
    let mut widget = SearchWidget::new(FontId(0), 1.0);

    widget.resize(800, 600, 1.0);
    widget.show();
    widget.logic(0.2); // reach alpha = 1.0

    let files = [(1u64, "test.rs", "crates/test.rs")];
    widget.push_char('t');
    widget.update_filter(
        files.iter().map(|(id, n, p)| (*id, *n, *p)),
        std::iter::empty(),
        std::iter::empty(),
    );

    widget.draw(&mut gfx, &mut list);
    assert!(!list.batches.is_empty());
}

#[test]
fn test_search_widget_edge_cases_and_coverage() {
    let mut gfx = Gfx::new();
    let mut list = gource_draw::DrawList::new(UVec2::new(800, 600));
    let mut widget = SearchWidget::new(FontId(0), 1.0);

    // Badges
    assert_eq!(SearchItemKind::File.badge_label(), "FILE");
    assert_eq!(SearchItemKind::Directory.badge_label(), "DIR");
    assert_eq!(SearchItemKind::User.badge_label(), "USER");
    assert!(SearchItemKind::File.badge_colour().w > 0.0);
    assert!(SearchItemKind::Directory.badge_colour().w > 0.0);
    assert!(SearchItemKind::User.badge_colour().w > 0.0);

    // Toggle
    assert!(!widget.visible);
    widget.toggle();
    assert!(widget.visible);
    widget.toggle();
    assert!(!widget.visible);

    // Draw while alpha <= 0.01 does nothing
    widget.alpha = 0.0;
    widget.draw(&mut gfx, &mut list);

    // Draw with empty query (draws placeholder text)
    widget.show();
    widget.alpha = 1.0;
    widget.draw(&mut gfx, &mut list);

    // Select prev when empty does nothing
    widget.select_prev();
    assert_eq!(widget.selected_index, 0);

    // Max display results limit (10 items)
    widget.push_char('a');
    let many_users: Vec<(u64, &str)> = (0..15).map(|i| (i, "alice")).collect();
    widget.update_filter(
        std::iter::empty(),
        std::iter::empty(),
        many_users.iter().copied(),
    );
    assert_eq!(widget.results.len(), 8);

    // Max display results limit on dirs
    let many_dirs: Vec<(u64, &str)> = (0..15).map(|i| (i, "abc")).collect();
    widget.update_filter(
        std::iter::empty(),
        many_dirs.iter().copied(),
        std::iter::empty(),
    );
    assert_eq!(widget.results.len(), 8);

    // Max display results limit on files
    let many_files: Vec<(u64, &str, &str)> = (0..15).map(|i| (i, "abc.rs", "src/abc.rs")).collect();
    widget.update_filter(
        many_files.iter().copied(),
        std::iter::empty(),
        std::iter::empty(),
    );
    assert_eq!(widget.results.len(), 8);

    // Navigation and draw with full results list
    widget.selected_index = 5;
    widget.select_prev();
    assert_eq!(widget.selected_index, 4);
    widget.draw(&mut gfx, &mut list);
}
