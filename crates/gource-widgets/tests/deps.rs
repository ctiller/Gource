use gource_core::Vec3;
use gource_vm::dashboard::{
    DashboardPanelVM, DashboardVM, EditorsLeaderboardVM, SparklineVM, StackedDiffBarsVM,
    TheseusCohortAreaVM,
};
use gource_widgets::dashboard::DashboardStack;
use std::fs;

#[test]
fn widgets_only_depends_on_allowed_crates() {
    let manifest_path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let content = fs::read_to_string(manifest_path).expect("Failed to read Cargo.toml");

    let mut in_dependencies = false;
    let allowed_crates = ["gource-core", "gource-draw", "gource-vm", "log"];
    let forbidden_crates = [
        "gource-scene",
        "gource-history",
        "gource-settings",
        "gource-vcs",
        "gource-app",
    ];

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        } else if trimmed.starts_with('[') {
            in_dependencies = false;
            continue;
        }

        if in_dependencies
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && let Some((dep_key, _)) = trimmed.split_once('=')
        {
            let dep_name = dep_key.trim();
            assert!(
                allowed_crates.contains(&dep_name),
                "Found unexpected dependency '{dep_name}' in gource-widgets [dependencies]"
            );
            assert!(
                !forbidden_crates.contains(&dep_name),
                "Found forbidden dependency '{dep_name}' in gource-widgets [dependencies]"
            );
        }
    }
}

#[test]
fn test_dashboard_stack_sync_from_vm_all_branches() {
    let sparkline = SparklineVM::new("Files", "1.2k")
        .with_values(&[10.0, 20.0, 30.0])
        .with_line_colour(Vec3::new(0.5, 0.5, 1.0))
        .with_delta("+5", true);

    let stacked_diff = StackedDiffBarsVM::new("Churn", "+100 -50").with_diffs(&[(100, 50)]);

    let theseus = TheseusCohortAreaVM::new("Theseus")
        .with_cohorts(&["cohort1".to_string()], &[Vec3::ONE], &[vec![10, 20, 30]])
        .with_analytics(Some(45.0), Some(0.12));

    let leaderboard = EditorsLeaderboardVM::new("Editors", 3).with_rows(&[(
        "alice".to_string(),
        Vec3::ONE,
        10,
        200,
    )]);

    let vm_visible = DashboardVM {
        visible: true,
        panels: vec![
            DashboardPanelVM::Sparkline(sparkline),
            DashboardPanelVM::StackedDiffBars(stacked_diff),
            DashboardPanelVM::TheseusCohort(theseus),
            DashboardPanelVM::EditorsLeaderboard(leaderboard),
        ],
    };

    let mut stack = DashboardStack::new();
    stack.sync_from_vm(&vm_visible);
    assert_eq!(stack.panels.len(), 4);

    let vm_invisible = DashboardVM {
        visible: false,
        panels: vm_visible.panels.clone(),
    };

    stack.sync_from_vm(&vm_invisible);
    assert_eq!(stack.panels.len(), 0);
}
