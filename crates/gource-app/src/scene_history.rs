//! History materialization extension trait for World.

use gource_core::{Vec2, Vec3};
use gource_history::{History, TreeSnapshot};
use gource_model::commit::{CommitFile, FileAction};
use gource_scene::dirnode::DirNode;
use gource_scene::world::World;
use gource_settings::GourceSettings;
use std::collections::BTreeMap;

pub trait WorldHistory {
    fn materialize_from_snapshot(
        &mut self,
        snap: &TreeSnapshot,
        history: &History,
        settings: &GourceSettings,
        settle_steps: usize,
    );
}

impl WorldHistory for World {
    fn materialize_from_snapshot(
        &mut self,
        snap: &gource_history::TreeSnapshot,
        history: &gource_history::History,
        settings: &GourceSettings,
        settle_steps: usize,
    ) {
        // Reset dirs and files, keeping only the root
        self.files.clear();
        self.files_by_path.clear();
        self.removed_files.clear();
        self.users.clear();
        self.users_by_name.clear();
        self.users_by_tag.clear();
        self.new_users.clear();
        self.user_tree = None;
        self.dir_tree = None;

        self.weighted_mode = settings.file_size_metric != gource_settings::FileSizeMetric::None;

        self.dirs.clear();
        self.dir_map.clear();
        let root_node = DirNode::new("/", self.params.file_area, self.params.padding);
        self.root = self.dirs.insert(root_node);
        self.dir_map.insert("/".to_string(), self.root);

        // Track last-touched file position for each active user to place them accurately
        let mut user_last_file_path: BTreeMap<String, String> = BTreeMap::new();

        // Materialize live files
        for live in &snap.files {
            let fullpath = match history.paths.resolve(live.path) {
                Some(p) => p.to_string(),
                None => continue,
            };

            let colour = history
                .paths
                .get(live.path)
                .map(|e| e.colour)
                .unwrap_or(gource_vcs::commit::WHITE);

            let cf = CommitFile {
                filename: fullpath.clone(),
                action: FileAction::Add,
                colour,
                ..Default::default()
            };

            let fid = match self.add_file(&cf, settings) {
                Some(id) => id,
                None => continue,
            };

            // Find dominant cohort color if present
            let dominant_cohort_colour = live
                .cohorts
                .buckets
                .iter()
                .max_by_key(|(_, lines)| *lines)
                .and_then(|(cid, _)| history.cohorts.get(*cid))
                .map(|label| self.hasher.colour_hash(label));

            let (dir_id, was_hidden) = {
                let file = &mut self.files[fid];
                file.lines = live.lines;
                file.byte_size = live.byte_size;
                file.touch_count = live.touch_count;
                file.created_timestamp = live.created_timestamp;
                file.dominant_cohort_colour = dominant_cohort_colour;

                // Touch to make the file visible in the simulation
                let was_hidden = file.pawn.is_hidden();
                file.touch(live.last_timestamp, Vec3::from(colour));

                // If a file size metric is enabled, set target weight and snap initial size
                let weight: u64 = match settings.file_size_metric {
                    gource_settings::FileSizeMetric::None => 0,
                    gource_settings::FileSizeMetric::Size => live.byte_size,
                    gource_settings::FileSizeMetric::Lines => live.lines as u64,
                    gource_settings::FileSizeMetric::Diff => live.lines.max(1) as u64,
                    gource_settings::FileSizeMetric::Churn => live.cohorts.total_churn_removed,
                };
                if weight > 0 {
                    file.weighted = true;
                    let ref_weight = match settings.file_size_metric {
                        gource_settings::FileSizeMetric::Size => 1024,
                        gource_settings::FileSizeMetric::Lines => 100,
                        gource_settings::FileSizeMetric::Diff => 50,
                        gource_settings::FileSizeMetric::Churn => 100,
                        _ => 100,
                    };
                    file.set_weight_target(weight, ref_weight, self.tuning.file_diameter);
                    file.sim.size = file.sim.target_size;
                    file.sim.radius = file.sim.target_size / 2;
                    file.pawn.size = file.target_size();
                    file.pawn.dims = Vec2::splat(file.pawn.size);
                }

                (file.dir, was_hidden)
            };

            if let Some(did) = dir_id {
                if was_hidden {
                    self.dirs[did].add_visible();
                }
                self.dirs[did].since_last_file_change = 0.0;
                self.on_node_updated(did, true);
            }

            if let Some(user_name) = history.users.get(live.last_user) {
                user_last_file_path.insert(user_name.to_string(), fullpath);
            }
        }

        // Apply weighted layout if a file size metric is active
        if settings.file_size_metric != gource_settings::FileSizeMetric::None {
            self.update_weighted_layout();
        }

        // Materialize active users and position them near their touched files
        for (username, path) in user_last_file_path {
            let uid = self.add_user(&username, settings);
            if let Some(&fid) = self.files_by_path.get(&path)
                && let Some(file_pos) = self.file_sim_pos(fid)
            {
                self.place_user_near(uid, file_pos);
            }
        }

        // Settle the layout
        self.settle(settle_steps);
    }
}
