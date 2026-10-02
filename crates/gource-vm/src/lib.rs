//! Display view-models (`TimelineVM`, `DashboardVM`, `LegendVM`, `SearchVM`,
//! `CaptionsVM`, `TooltipVM`, `SettingsVM`, `SceneFrame`) for Gource.
//!
//! `gource-vm` depends only on `gource-core`, `gource-model`, and `serde`.
//! Presenters in `gource-app` construct these view-models from the domain models,
//! and widgets in `gource-widgets` render them into a `DrawList`.

pub mod captions;
pub mod dashboard;
pub mod legend;
pub mod scene;
pub mod search;
pub mod settings;
pub mod timeline;
pub mod tooltip;

pub use captions::{CaptionItemVM, CaptionsVM};
pub use dashboard::{
    DashboardPanelVM, DashboardVM, EditorsLeaderboardVM, SparklineVM, StackedDiffBarsVM,
    TheseusCohortAreaVM, format_compact_u64,
};
pub use legend::{LegendRowVM, LegendVM};
pub use scene::{
    ActionBeamDrawItem, DirDrawItem, EdgeDrawItem, FileDrawItem, SceneFrame, SceneVM, UserDrawItem,
};
pub use search::{SearchItem, SearchItemKind, SearchVM};
pub use settings::{SettingsVM, TuningRowView, TuningTab};
pub use timeline::{TimelineBarData, TimelineBarMarker, TimelineHoverCard, TimelineVM};
pub use tooltip::TooltipVM;

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::{Vec2, Vec3, Vec4};

    #[test]
    fn timeline_vm_builders_and_defaults() {
        let marker = TimelineBarMarker::new(1.5, "v1.0", Vec4::ONE);
        assert_eq!(marker.frac, 1.0);
        assert_eq!(marker.label, "v1.0");

        let hover = TimelineHoverCard::new(0.5, "2025-01-01", 12)
            .with_diff(100, 40)
            .with_top_editors(&[("alice".into(), Vec4::ONE, 7)]);
        assert_eq!(hover.commits, 12);
        assert_eq!(hover.lines_added, 100);
        assert_eq!(hover.lines_removed, 40);
        assert_eq!(hover.top_editors.len(), 1);

        let vm = TimelineVM::default();
        assert_eq!(vm.clip_in_frac, 0.0);
        assert_eq!(vm.clip_out_frac, 1.0);
        assert_eq!(vm.direction_label, "▶ 1.0x");
    }

    #[test]
    fn dashboard_vm_builders_and_compact_format() {
        assert_eq!(format_compact_u64(999), "999");
        assert_eq!(format_compact_u64(1_250), "1.2k");
        assert_eq!(format_compact_u64(15_400), "15.4k");
        assert_eq!(format_compact_u64(250_000), "250k");
        assert_eq!(format_compact_u64(2_500_000), "2.5M");
        assert_eq!(format_compact_u64(120_000_000), "120M");
        assert_eq!(format_compact_u64(3_450_000_000), "3.45B");
        assert_eq!(format_compact_u64(25_000_000_000), "25.0B");
        assert_eq!(format_compact_u64(250_000_000_000), "250B");

        let sp = SparklineVM::new("Files", "1.2k")
            .with_values(&[1.0, 2.0])
            .with_delta("+10", true)
            .with_line_colour(Vec3::ONE)
            .with_height(70.0);
        assert_eq!(DashboardPanelVM::Sparkline(sp).height(1.0), 70.0);

        let db = StackedDiffBarsVM::new("Churn", "+10 / -5")
            .with_diffs(&[(10, 5)])
            .with_height(85.0);
        assert_eq!(DashboardPanelVM::StackedDiffBars(db).height(1.0), 85.0);

        let th = TheseusCohortAreaVM::new("Cohort")
            .with_cohorts(&["2025".into()], &[Vec3::ONE], &[vec![100]])
            .with_analytics(Some(30.0), Some(0.1))
            .with_height(120.0);
        assert_eq!(DashboardPanelVM::TheseusCohort(th).height(1.0), 120.0);

        let lb = EditorsLeaderboardVM::new("Editors", 2)
            .with_rows(&[("alice".into(), Vec3::ONE, 5, 100)])
            .with_max_rows(3);
        assert_eq!(DashboardPanelVM::EditorsLeaderboard(lb).height(1.0), 56.0);
    }

    #[test]
    fn search_settings_and_other_vms() {
        assert_eq!(SearchItemKind::File.badge_label(), "FILE");
        assert_eq!(SearchItemKind::Directory.badge_label(), "DIR");
        assert_eq!(SearchItemKind::User.badge_label(), "USER");
        let _ = SearchItemKind::File.badge_colour();
        let _ = SearchItemKind::Directory.badge_colour();
        let _ = SearchItemKind::User.badge_colour();

        let f = SearchItem::new_file(1, "main.rs", "src/main.rs");
        let d = SearchItem::new_dir(2, "src");
        let u = SearchItem::new_user(3, "alice");
        assert_eq!((f.id, d.id, u.id), (1, 2, 3));

        for tab in TuningTab::ALL {
            assert!(!tab.label().is_empty());
            let _ = tab.badge_colour();
        }
        let s1 = TuningRowView::new_slider(0, "Speed", "--seconds-per-day", "1.0", 0.5, false);
        let s2 = TuningRowView::new_toggle(1, "Bloom", "--hide-bloom", "ON", true, true);
        let s3 = TuningRowView::new_cycle(2, "Mode", "--camera-mode", "overview", false);
        assert_eq!(s1.slider_frac, Some(0.5));
        assert_eq!(s2.toggle_state, Some(true));
        assert_eq!(s3.slider_frac, None);

        let settings_vm = SettingsVM::default();
        assert_eq!(settings_vm.active_tab, TuningTab::Visual);

        let _ = LegendVM::default();
        let _ = CaptionsVM::default();
        let _ = TooltipVM {
            visible: true,
            screen_pos: Vec2::ZERO,
            title: "t".into(),
            subtitle: None,
            accent_colour: Vec3::ONE,
        };
        let _ = SceneFrame::default();
    }
}
