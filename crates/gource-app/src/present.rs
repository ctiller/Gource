//! Pure presenter functions constructing view models (`gource-vm`) from the simulation (`Gource`).

use crate::gource::Gource;
use crate::platform::Viewport;
use gource_core::{Vec2, Vec4};
use gource_vm::captions::{CaptionItemVM, CaptionsVM};
use gource_vm::dashboard::{
    DashboardPanelVM, DashboardVM, EditorsLeaderboardVM, SparklineVM, StackedDiffBarsVM,
    TheseusCohortAreaVM, format_compact_u64,
};
use gource_vm::legend::{LegendRowVM, LegendVM};
use gource_vm::scene::{
    ActionBeamDrawItem, DirDrawItem, EdgeDrawItem, FileDrawItem, SceneFrame, SceneVM, UserDrawItem,
};
use gource_vm::search::SearchVM;
use gource_vm::settings::SettingsVM;
use gource_vm::timeline::TimelineBarData;

/// Present the timeline scrubber view-model.
pub fn present_timeline(gource: &mut Gource) -> TimelineBarData {
    gource.build_timeline_bar_data()
}

/// Present the settings drawer view-model.
pub fn present_settings(gource: &Gource) -> SettingsVM {
    let data = gource.build_tuning_panel_data();
    SettingsVM {
        visible: gource.tuning_panel.is_visible(),
        active_tab: data.active_tab,
        rows: data.rows,
    }
}

/// Present the HUD analytics dashboard view-model.
pub fn present_dashboard(gource: &mut Gource) -> DashboardVM {
    if gource.settings.hide_dashboards || gource.settings.dashboards.is_empty() {
        return DashboardVM {
            visible: false,
            panels: Vec::new(),
        };
    }

    let hist = gource.ensure_history();
    let playhead_commit_idx = if hist.is_empty() {
        0
    } else {
        match hist
            .commits
            .binary_search_by_key(&gource.currtime, |c| c.timestamp)
        {
            Ok(idx) => idx,
            Err(idx) => {
                if idx == 0 {
                    0
                } else {
                    idx - 1
                }
            }
        }
    };

    let period_secs = match gource.settings.dashboard_period {
        gource_settings::DashboardPeriod::Day => 86400,
        gource_settings::DashboardPeriod::Week => 7 * 86400,
        gource_settings::DashboardPeriod::Month => 30 * 86400,
        gource_settings::DashboardPeriod::Year => 365 * 86400,
    };
    let window_secs = (gource.settings.dashboard_window_days as i64) * 86400;

    let series_data = if let (Some(cached_idx), Some(cached_data)) = (
        gource.cached_dashboard_commit,
        &gource.cached_dashboard_data,
    ) {
        if cached_idx == playhead_commit_idx {
            cached_data.clone()
        } else {
            let data = gource_history::DashboardSeriesData::extract(
                &hist,
                playhead_commit_idx,
                period_secs,
                window_secs,
                20,
            );
            gource.cached_dashboard_commit = Some(playhead_commit_idx);
            gource.cached_dashboard_data = Some(data.clone());
            data
        }
    } else {
        let data = gource_history::DashboardSeriesData::extract(
            &hist,
            playhead_commit_idx,
            period_secs,
            window_secs,
            20,
        );
        gource.cached_dashboard_commit = Some(playhead_commit_idx);
        gource.cached_dashboard_data = Some(data.clone());
        data
    };

    let mut panels = Vec::new();
    for panel_kind in &gource.settings.dashboards {
        match panel_kind {
            gource_settings::DashboardPanel::Lines => {
                let p =
                    SparklineVM::new("Lines of Code", format_compact_u64(series_data.total_lines))
                        .with_values(&series_data.lines_sparkline)
                        .with_delta(
                            format!("{:+}", series_data.lines_delta_in_window),
                            series_data.lines_delta_in_window >= 0,
                        );
                panels.push(DashboardPanelVM::Sparkline(p));
            }
            gource_settings::DashboardPanel::Diff => {
                let diffs: Vec<(u64, u64)> = series_data
                    .diff_bars
                    .iter()
                    .map(|&(a, r)| (a as u64, r as u64))
                    .collect();
                let p = StackedDiffBarsVM::new(
                    "Code Churn",
                    format!(
                        "+{} -{}",
                        format_compact_u64(diffs.iter().map(|d| d.0).sum()),
                        format_compact_u64(diffs.iter().map(|d| d.1).sum())
                    ),
                )
                .with_diffs(&diffs);
                panels.push(DashboardPanelVM::StackedDiffBars(p));
            }
            gource_settings::DashboardPanel::Theseus => {
                let cohort_colors: Vec<gource_core::Vec3> =
                    (0..series_data.theseus_cohorts.cohort_labels.len())
                        .map(gource_widgets::dashboard::DashboardStack::cohort_palette)
                        .collect();
                let p = TheseusCohortAreaVM::new("Git-of-Theseus")
                    .with_cohorts(
                        &series_data.theseus_cohorts.cohort_labels,
                        &cohort_colors,
                        &series_data.theseus_cohorts.samples,
                    )
                    .with_analytics(
                        series_data.theseus_cohorts.half_life_days,
                        Some(series_data.theseus_cohorts.churn_rate),
                    );
                panels.push(DashboardPanelVM::TheseusCohort(p));
            }
            gource_settings::DashboardPanel::Editors => {
                let p =
                    EditorsLeaderboardVM::new("Top Contributors", series_data.active_editors_count)
                        .with_rows(
                            &series_data
                                .top_editors
                                .iter()
                                .map(|(n, c, a, b)| {
                                    (n.clone(), gource_core::Vec3::from(*c), *a, *b)
                                })
                                .collect::<Vec<_>>(),
                        );
                panels.push(DashboardPanelVM::EditorsLeaderboard(p));
            }
            gource_settings::DashboardPanel::Commits => {
                let p = SparklineVM::new(
                    "Commits",
                    format_compact_u64(series_data.commits_in_window as u64),
                )
                .with_values(&series_data.commits_per_period)
                .with_line_colour(gource_core::Vec3::new(0.95, 0.65, 0.2));
                panels.push(DashboardPanelVM::Sparkline(p));
            }
            gource_settings::DashboardPanel::Churn => {
                let p =
                    SparklineVM::new("Files", format_compact_u64(series_data.total_files as u64))
                        .with_values(&series_data.files_sparkline)
                        .with_line_colour(gource_core::Vec3::new(0.85, 0.4, 0.9));
                panels.push(DashboardPanelVM::Sparkline(p));
            }
        }
    }

    DashboardVM {
        visible: true,
        panels,
    }
}

/// Present the file extension key / legend view-model.
pub fn present_legend(gource: &Gource) -> LegendVM {
    let visible = gource.file_key.show;
    let mut rows = Vec::new();
    for ext in &gource.file_key.active_keys {
        if let Some(entry) = gource.file_key.keymap.get(ext) {
            rows.push(LegendRowVM {
                ext: entry.ext.clone(),
                colour: entry.colour,
                count: entry.count,
            });
        }
    }
    LegendVM { visible, rows }
}

/// Present the search bar view-model.
pub fn present_search(gource: &Gource) -> SearchVM {
    SearchVM {
        visible: gource.search_widget.visible,
        query: gource.search_widget.query.clone(),
        selected_index: gource.search_widget.selected_index,
        results: gource.search_widget.results.clone(),
    }
}

/// Present active captions view-model.
pub fn present_captions(gource: &Gource) -> CaptionsVM {
    let items = gource
        .active_captions
        .iter()
        .map(|c| CaptionItemVM {
            caption: c.caption.clone(),
            timestamp: c.timestamp,
            pos: c.pos,
            colour: c.colour,
            alpha: c.alpha,
        })
        .collect();
    CaptionsVM { items }
}

/// Present the projected 2D scene view-model from the world, camera, and viewport.
pub fn present_scene(gource: &Gource, viewport: Viewport) -> SceneVM {
    let proj = gource.camera.projection(viewport.size());
    let mut frame = SceneFrame::default();

    // Collect directory draw items & edges recursively
    fn collect_dirs(
        world: &gource_scene::world::World,
        dir_id: gource_scene::file::DirId,
        proj: &gource_draw::Projection,
        settings: &gource_settings::GourceSettings,
        frame: &mut SceneFrame,
    ) {
        let dir = &world.dirs[dir_id];
        let screen_pos = proj.to_screen(dir.pos);
        let bloom_radius = dir.dir_radius * 2.0 * settings.bloom_multiplier;
        let screen_radius = proj.to_screen_len(bloom_radius);
        let bloom_col = dir.col * settings.bloom_intensity;

        let label = if dir.parent.is_some() && !settings.hide_dirnames && !dir.path_token.is_empty()
        {
            Some(dir.path_token.clone())
        } else {
            None
        };

        let label_alpha = if settings.highlight_dirs {
            1.0
        } else {
            ((5.0f32 - dir.since_last_node_change) / 5.0f32).clamp(0.0, 1.0)
        };

        let dir_item = DirDrawItem {
            id: slotmap::Key::data(&dir_id).as_ffi(),
            pos: screen_pos,
            radius: screen_radius,
            bloom_colour: Vec4::new(bloom_col.x, bloom_col.y, bloom_col.z, 1.0),
            label,
            label_alpha,
        };
        frame.dirs.push(dir_item);

        if let Some(pid) = dir.parent
            && !settings.hide_tree
            && (!settings.hide_root || world.dirs[pid].parent.is_some())
        {
            let parent_dir = &world.dirs[pid];
            let p1 = proj.to_screen(parent_dir.pos);
            let p2 = screen_pos;
            let spos = proj.to_screen(dir.spos);
            frame.edges.push(EdgeDrawItem {
                pos1: p1,
                col1: parent_dir.col,
                pos2: p2,
                col2: dir.col,
                spos,
            });
        }

        // Collect files in this dir
        if !settings.hide_files {
            for &fid in &dir.files {
                if let Some(file) = world.files.get(fid) {
                    if file.pawn.is_hidden() {
                        continue;
                    }
                    let world_pos = file.absolute_pos(dir.pos);
                    let screen_file_pos = proj.to_screen(world_pos);
                    let screen_size = proj.to_screen_len(file.pawn.size);
                    let col = if settings.file_colour_mode
                        == gource_settings::FileColourMode::Extension
                    {
                        file.colour()
                    } else {
                        file.display_colour(settings.file_colour_mode, 0)
                    };
                    let label = if !settings.hide_filenames {
                        Some(file.display_name(settings.file_extensions).to_string())
                    } else {
                        None
                    };

                    frame.files.push(FileDrawItem {
                        id: slotmap::Key::data(&fid).as_ffi(),
                        pos: screen_file_pos,
                        size: screen_size,
                        colour: col,
                        alpha: file.alpha(),
                        touch_alpha: 1.0,
                        label,
                        label_alpha: 1.0,
                    });
                }
            }
        }

        for &cid in &dir.children {
            collect_dirs(world, cid, proj, settings, frame);
        }
    }

    collect_dirs(
        &gource.world,
        gource.world.root,
        &proj,
        &gource.settings,
        &mut frame,
    );

    // Users & Actions
    if !gource.settings.hide_users {
        let user_scale_factor = if gource.settings.fixed_user_size {
            proj.distance / -crate::camera::STARTING_Z
        } else {
            1.0
        };

        for (uid, user) in &gource.world.users {
            if user.pawn.is_hidden() {
                continue;
            }
            let screen_pos = proj.to_screen(user.pawn.pos);
            let screen_size = proj.to_screen_len(user.pawn.size * user_scale_factor);
            let alpha = user.alpha(gource.settings.user_idle_time);

            frame.users.push(UserDrawItem {
                id: slotmap::Key::data(&uid).as_ffi(),
                pos: screen_pos,
                size: screen_size,
                colour: user.colour(),
                alpha,
                name: user.name().to_string(),
                label_alpha: if gource.settings.highlight_all_users {
                    1.0
                } else {
                    user.pawn.name_alpha()
                },
            });

            for a in &user.active_actions {
                if a.is_finished() {
                    continue;
                }
                if let Some(file) = gource.world.files.get(a.target) {
                    let dir_pos = file
                        .dir
                        .and_then(|d| gource.world.dirs.get(d))
                        .map(|d| d.pos)
                        .unwrap_or(Vec2::ZERO);
                    let src_screen = screen_pos;
                    let dest_screen = proj.to_screen(file.absolute_pos(dir_pos));
                    let col = a.colour;
                    let alpha = (1.0 - a.progress).clamp(0.0, 1.0);
                    frame.actions.push(ActionBeamDrawItem {
                        source_pos: src_screen,
                        target_pos: dest_screen,
                        colour: Vec4::new(col.x, col.y, col.z, alpha),
                        progress: a.progress,
                    });
                }
            }
        }
    }

    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppOptions;
    use crate::platform::Viewport;
    use gource_draw::Gfx;
    use gource_settings::GourceSettings;
    use gource_widgets::caption::RCaption;

    fn create_test_gource() -> (Gource, Gfx) {
        let mut gfx = Gfx::new();
        let settings = GourceSettings::default();
        let options = AppOptions::default();
        let viewport = Viewport::new(800, 600);
        let gource = Gource::new(settings, &mut gfx, &options, viewport, 60)
            .expect("Failed to initialize test Gource");
        (gource, gfx)
    }

    #[test]
    fn test_presenters_coverage() {
        let (mut gource, _) = create_test_gource();
        let viewport = Viewport::new(800, 600);

        // 1. present_timeline
        let tl_data = present_timeline(&mut gource);
        assert_eq!(tl_data.playhead_frac, 0.0);

        // 2. present_settings
        gource.tuning_panel.show(true);
        let s_vm = present_settings(&gource);
        assert!(s_vm.visible);
        assert!(!s_vm.rows.is_empty());
        gource.tuning_panel.show(false);
        let s_vm_hidden = present_settings(&gource);
        assert!(!s_vm_hidden.visible);

        // 3. present_dashboard
        // Visible branch
        gource.settings.dashboards = vec![gource_settings::DashboardPanel::Lines];
        let d_vm = present_dashboard(&mut gource);
        assert!(d_vm.visible);
        assert!(!d_vm.panels.is_empty());

        // Test cached dashboard path
        let d_vm_cached = present_dashboard(&mut gource);
        assert_eq!(d_vm, d_vm_cached);

        // Test all period variants
        for period in [
            gource_settings::DashboardPeriod::Day,
            gource_settings::DashboardPeriod::Week,
            gource_settings::DashboardPeriod::Month,
            gource_settings::DashboardPeriod::Year,
        ] {
            gource.settings.dashboard_period = period;
            gource.cached_dashboard_commit = None;
            let _ = present_dashboard(&mut gource);
        }

        // Test all dashboard panel variants
        gource.settings.dashboards = vec![
            gource_settings::DashboardPanel::Lines,
            gource_settings::DashboardPanel::Diff,
            gource_settings::DashboardPanel::Theseus,
            gource_settings::DashboardPanel::Editors,
            gource_settings::DashboardPanel::Commits,
            gource_settings::DashboardPanel::Churn,
        ];
        gource.cached_dashboard_commit = None;
        let d_vm_all = present_dashboard(&mut gource);
        assert_eq!(d_vm_all.panels.len(), 6);

        // Hidden branch
        gource.settings.hide_dashboards = true;
        let d_vm_hidden = present_dashboard(&mut gource);
        assert!(!d_vm_hidden.visible);
        assert!(d_vm_hidden.panels.is_empty());

        gource.settings.hide_dashboards = false;
        gource.settings.dashboards.clear();
        let d_vm_empty = present_dashboard(&mut gource);
        assert!(!d_vm_empty.visible);

        // 4. present_legend
        gource.file_key.set_show(true);
        let leg_vm = present_legend(&gource);
        assert!(leg_vm.visible);
        gource.file_key.set_show(false);
        let leg_vm_hidden = present_legend(&gource);
        assert!(!leg_vm_hidden.visible);

        // Add a key entry
        gource.file_key.set_show(true);
        gource.file_key.inc("rs", gource_core::Vec3::ONE, |_| 10.0);
        gource.file_key.logic(2.0, 600.0);
        let leg_vm_rows = present_legend(&gource);
        assert_eq!(leg_vm_rows.rows.len(), 1);
        assert_eq!(leg_vm_rows.rows[0].ext, "rs");

        // 5. present_search
        gource.search_widget.show();
        gource.search_widget.push_char('a');
        let search_vm = present_search(&gource);
        assert!(search_vm.visible);
        assert_eq!(search_vm.query, "a");

        // 6. present_captions
        let cap = RCaption::new("Hello World", 12345, gource.fonts.caption);
        gource.active_captions.push(cap);
        let cap_vm = present_captions(&gource);
        assert_eq!(cap_vm.items.len(), 1);
        assert_eq!(cap_vm.items[0].caption, "Hello World");

        // 7. present_scene
        let scene_vm = present_scene(&gource, viewport);
        assert!(!scene_vm.dirs.is_empty());

        // Add user, file, and action to test full scene projection
        let cf = gource_model::commit::CommitFile {
            filename: "/test.rs".to_string(),
            action: gource_model::commit::FileAction::Modify,
            colour: [1.0, 1.0, 1.0],
            lines_added: Some(5),
            lines_removed: Some(2),
            is_binary: false,
            is_shadow: false,
        };
        let file_id = gource
            .world
            .add_file(&cf, &gource.settings)
            .expect("Failed to add file");
        gource.world.files[file_id].pawn.set_hidden(false);
        let _user_id = gource.world.add_user("alice", &gource.settings);
        let commit = gource_model::commit::Commit {
            timestamp: 1000,
            username: "alice".to_string(),
            files: vec![cf.clone()],
            is_shadow: false,
        };
        gource
            .world
            .add_file_action(&commit, &cf, file_id, 0.0, &gource.settings);
        // Move action into active_actions for Alice
        if let Some(user) = gource.world.users.get_mut(_user_id)
            && let Some(act) = user.actions.pop()
        {
            user.active_actions.push(act);
        }

        // Project again with file, user, and action
        let scene_vm_full = present_scene(&gource, viewport);
        assert!(!scene_vm_full.files.is_empty());
        assert!(!scene_vm_full.users.is_empty());
        assert!(!scene_vm_full.actions.is_empty());

        // Test fixed_user_size and hide toggles
        gource.settings.fixed_user_size = true;
        gource.settings.highlight_all_users = true;
        gource.settings.highlight_dirs = true;
        let _ = present_scene(&gource, viewport);

        gource.settings.hide_files = true;
        gource.settings.hide_users = true;
        gource.settings.hide_dirnames = true;
        gource.settings.hide_tree = true;
        let scene_vm_hidden = present_scene(&gource, viewport);
        assert!(scene_vm_hidden.files.is_empty());
        assert!(scene_vm_hidden.users.is_empty());
        assert!(scene_vm_hidden.actions.is_empty());
    }
}
