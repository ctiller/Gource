//! 2D physics using Rapier 2D with `enhanced-determinism`.
//!
//! Provides deterministic circle physics for:
//! 1. File central attraction + circle edge collisions inside a directory.
//! 2. Multi-body directory contact resolution without overlapping.

use glam::Vec2;
use rapier2d::prelude::*;

/// Simulates central attraction + circle edge collisions for visible files in a directory
/// using Rapier 2D with `enhanced-determinism`.
///
/// Each file circle has position `pos` and collision radius `r`.
/// Returns the resulting positions centered around (0, 0) with zero overlap.
pub fn step_directory_files_rapier(files: &[(Vec2, f32)], steps: usize) -> Vec<Vec2> {
    let n = files.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Vec2::ZERO];
    }

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    let mut body_handles = Vec::with_capacity(n);

    for &(pos, r) in files {
        let rb = RigidBodyBuilder::dynamic()
            .translation(vector![pos.x, pos.y])
            .linear_damping(10.0)
            .lock_rotations()
            .build();
        let handle = rigid_body_set.insert(rb);

        let col = ColliderBuilder::ball(r)
            .restitution(0.0)
            .friction(0.02)
            .density(1.0)
            .build();
        collider_set.insert_with_parent(col, handle, &mut rigid_body_set);
        body_handles.push(handle);
    }

    let gravity = vector![0.0, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = DefaultBroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();

    for _ in 0..steps {
        // Apply central attraction toward (0, 0)
        for &handle in &body_handles {
            if let Some(rb) = rigid_body_set.get_mut(handle) {
                let p = rb.translation();
                let dist = (p.x * p.x + p.y * p.y).sqrt();
                if dist > 1e-4 {
                    let dir_x = -p.x / dist;
                    let dir_y = -p.y / dist;
                    // Attraction strength scales with distance and radius
                    let speed = (dist * 5.0).min(50.0);
                    rb.set_linvel(vector![dir_x * speed, dir_y * speed], true);
                } else {
                    rb.set_linvel(vector![0.0, 0.0], true);
                }
            }
        }

        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut impulse_joint_set,
            &mut multibody_joint_set,
            &mut ccd_solver,
            None,
            &(),
            &(),
        );
    }

    // Read out positions
    let mut result: Vec<Vec2> = body_handles
        .iter()
        .map(|&h| {
            let t = rigid_body_set[h].translation();
            Vec2::new(t.x, t.y)
        })
        .collect();

    // Recenter cluster bounding box to (0, 0)
    let mut min_b = Vec2::splat(f32::INFINITY);
    let mut max_b = Vec2::splat(-f32::INFINITY);
    for (i, &p) in result.iter().enumerate() {
        let r = files[i].1;
        min_b = min_b.min(p - Vec2::splat(r));
        max_b = max_b.max(p + Vec2::splat(r));
    }
    let center = (min_b + max_b) * 0.5;
    for p in &mut result {
        *p -= center;
    }

    // Final position projection passes so ||p_b - p_a|| >= r_a + r_b holds strictly
    for _ in 0..64 {
        let mut any_overlap = false;
        for i in 0..n {
            let r_i = files[i].1;
            for j in (i + 1)..n {
                let r_j = files[j].1;
                let min_dist = r_i + r_j;
                let delta = result[j] - result[i];
                let dist = delta.length();
                if dist < min_dist {
                    any_overlap = true;
                    let overlap = min_dist - dist;
                    let norm = if dist > 1e-5 {
                        delta / dist
                    } else {
                        let angle = ((i * 31 + j * 17) as f32) * 0.1;
                        Vec2::new(angle.cos(), angle.sin())
                    };
                    result[i] -= norm * (overlap * 0.501);
                    result[j] += norm * (overlap * 0.501);
                }
            }
        }
        if !any_overlap {
            break;
        }
    }

    // Tightness pass: pull circles inward toward their closest neighbor until tangent contact
    for i in 0..n {
        let r_i = files[i].1;
        let mut closest_j = None;
        let mut min_gap = f32::INFINITY;
        for j in 0..n {
            if i == j {
                continue;
            }
            let r_j = files[j].1;
            let gap = (result[j] - result[i]).length() - (r_i + r_j);
            if gap >= -1e-4 && gap < min_gap {
                min_gap = gap;
                closest_j = Some(j);
            }
        }
        if let Some(j) = closest_j
            && min_gap > 1e-3
            && min_gap < 1.0
        {
            let r_j = files[j].1;
            let delta = result[j] - result[i];
            let dist = delta.length();
            if dist > 1e-5 {
                let dir = delta / dist;
                let target_pos = result[j] - dir * (r_i + r_j);
                // Check if target_pos doesn't overlap any third circle
                let mut can_move = true;
                for m in 0..n {
                    if m == i || m == j {
                        continue;
                    }
                    let r_m = files[m].1;
                    if (target_pos - result[m]).length() < (r_i + r_m) - 1e-3 {
                        can_move = false;
                        break;
                    }
                }
                if can_move {
                    result[i] = target_pos;
                }
            }
        }
    }

    // Final check for zero overlap
    for _ in 0..32 {
        let mut any_overlap = false;
        for i in 0..n {
            let r_i = files[i].1;
            for j in (i + 1)..n {
                let r_j = files[j].1;
                let min_dist = r_i + r_j;
                let delta = result[j] - result[i];
                let dist = delta.length();
                if dist < min_dist {
                    any_overlap = true;
                    let overlap = min_dist - dist;
                    let norm = if dist > 1e-5 {
                        delta / dist
                    } else {
                        let angle = ((i * 31 + j * 17) as f32) * 0.1;
                        Vec2::new(angle.cos(), angle.sin())
                    };
                    result[i] -= norm * (overlap * 0.501);
                    result[j] += norm * (overlap * 0.501);
                }
            }
        }
        if !any_overlap {
            break;
        }
    }

    result
}

/// Descriptor for directory circles in multi-body contact resolution.
#[derive(Clone, Copy, Debug)]
pub struct DirCircleDesc {
    pub pos: Vec2,
    pub dir_radius: f32,
    pub parent_radius: f32,
    pub parent_idx: Option<usize>,
    pub is_root: bool,
    pub is_empty: bool,
}

/// Resolves multi-body directory circle contacts using Rapier 2D.
///
/// For non-empty directories, ensures non-ancestor directories do not overlap
/// their `dir_radius`, and ancestor-descendant pairs respect `parent_radius`.
/// Root directories remain fixed, dynamic directories resolve contacts via Rapier.
pub fn resolve_directory_contacts_rapier(
    dirs: &[DirCircleDesc],
    ancestor_pairs: &[(usize, usize)],
    steps: usize,
) -> Vec<Vec2> {
    let n = dirs.len();
    if n < 2 {
        return dirs.iter().map(|d| d.pos).collect();
    }

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();
    let mut body_handles = Vec::with_capacity(n);

    for (idx, d) in dirs.iter().enumerate() {
        if d.is_empty {
            body_handles.push(None);
            continue;
        }

        let rb = if d.is_root {
            RigidBodyBuilder::fixed()
                .translation(vector![d.pos.x, d.pos.y])
                .build()
        } else {
            RigidBodyBuilder::dynamic()
                .translation(vector![d.pos.x, d.pos.y])
                .linear_damping(12.0)
                .lock_rotations()
                .build()
        };
        let handle = rigid_body_set.insert(rb);

        let col = ColliderBuilder::ball(d.dir_radius)
            .restitution(0.0)
            .friction(0.05)
            .density(1.0)
            .build();
        collider_set.insert_with_parent(col, handle, &mut rigid_body_set);
        body_handles.push(Some((handle, idx)));
    }

    let gravity = vector![0.0, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = DefaultBroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();

    for _ in 0..steps {
        // Parent-child radial attraction impulses when dist > parent_radius_A + parent_radius_B
        for (i, d) in dirs.iter().enumerate() {
            if let Some(p_idx) = d.parent_idx
                && let (Some(&(handle_child, _)), Some(&(handle_parent, _))) =
                    (body_handles[i].as_ref(), body_handles[p_idx].as_ref())
            {
                let p_child = rigid_body_set[handle_child].translation();
                let p_parent = rigid_body_set[handle_parent].translation();
                let delta_x = p_parent.x - p_child.x;
                let delta_y = p_parent.y - p_child.y;
                let dist = (delta_x * delta_x + delta_y * delta_y).sqrt();
                let max_dist = d.parent_radius + dirs[p_idx].parent_radius;

                if dist > max_dist && dist > 1e-4 {
                    let pull = ((dist - max_dist) * 2.0).min(30.0);
                    let dir_x = delta_x / dist;
                    let dir_y = delta_y / dist;
                    if !dirs[i].is_root {
                        let rb = &mut rigid_body_set[handle_child];
                        let v = rb.linvel();
                        rb.set_linvel(vector![v.x + dir_x * pull, v.y + dir_y * pull], true);
                    }
                }
            }
        }

        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut impulse_joint_set,
            &mut multibody_joint_set,
            &mut ccd_solver,
            None,
            &(),
            &(),
        );
    }

    // Read out positions
    let mut result: Vec<Vec2> = dirs.iter().map(|d| d.pos).collect();
    for (i, item) in body_handles.iter().enumerate() {
        if let Some((handle, _)) = item {
            let t = rigid_body_set[*handle].translation();
            result[i] = Vec2::new(t.x, t.y);
        }
    }

    // Direct collision relaxation pass to ensure parent_radius for ancestor pairs
    // and strict non-overlap for non-ancestor pairs
    for _ in 0..12 {
        let mut any_collision = false;
        for i in 0..n {
            if dirs[i].is_empty {
                continue;
            }
            for j in (i + 1)..n {
                if dirs[j].is_empty {
                    continue;
                }

                let is_anc = ancestor_pairs
                    .iter()
                    .any(|&(a, b)| (a == i && b == j) || (a == j && b == i));
                let min_dist = if is_anc {
                    dirs[i].parent_radius + dirs[j].parent_radius
                } else {
                    dirs[i].dir_radius + dirs[j].dir_radius
                };

                let delta = result[j] - result[i];
                let dist = delta.length();
                if dist < min_dist {
                    any_collision = true;
                    let overlap = min_dist - dist;
                    let norm = if dist > 1e-5 {
                        delta / dist
                    } else {
                        let angle = ((i * 31 + j * 17) as f32) * 0.1;
                        Vec2::new(angle.cos(), angle.sin())
                    };

                    match (dirs[i].is_root, dirs[j].is_root) {
                        (true, false) => {
                            result[j] += norm * overlap;
                        }
                        (false, true) => {
                            result[i] -= norm * overlap;
                        }
                        (false, false) => {
                            result[i] -= norm * (overlap * 0.5);
                            result[j] += norm * (overlap * 0.5);
                        }
                        (true, true) => {}
                    }
                }
            }
        }
        if !any_collision {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rapier_empty_and_single() {
        assert!(step_directory_files_rapier(&[], 10).is_empty());
        let single = step_directory_files_rapier(&[(Vec2::new(5.0, 5.0), 3.0)], 10);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0], Vec2::ZERO);
    }

    #[test]
    fn test_rapier_heterogeneous_files_packing() {
        let files = vec![
            (Vec2::new(-20.0, 0.0), 10.0),
            (Vec2::new(20.0, 0.0), 5.0),
            (Vec2::new(0.0, 15.0), 8.0),
            (Vec2::new(0.0, -15.0), 6.0),
        ];
        let positions = step_directory_files_rapier(&files, 20);
        assert_eq!(positions.len(), 4);

        // Verify non-overlapping
        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let dist = (positions[i] - positions[j]).length();
                let min_dist = files[i].1 + files[j].1;
                assert!(
                    dist >= min_dist - 1e-2,
                    "overlap between {i} and {j}: dist = {dist}, min_dist = {min_dist}"
                );
            }
        }
    }

    #[test]
    fn test_rapier_directory_contacts_resolution() {
        let dirs = vec![
            DirCircleDesc {
                pos: Vec2::ZERO,
                dir_radius: 15.0,
                parent_radius: 20.0,
                parent_idx: None,
                is_root: true,
                is_empty: false,
            },
            DirCircleDesc {
                pos: Vec2::new(5.0, 0.0),
                dir_radius: 12.0,
                parent_radius: 18.0,
                parent_idx: Some(0),
                is_root: false,
                is_empty: false,
            },
            DirCircleDesc {
                pos: Vec2::new(0.0, 5.0),
                dir_radius: 10.0,
                parent_radius: 15.0,
                parent_idx: Some(0),
                is_root: false,
                is_empty: false,
            },
            DirCircleDesc {
                pos: Vec2::new(2.0, 2.0),
                dir_radius: 14.0,
                parent_radius: 20.0,
                parent_idx: Some(0),
                is_root: false,
                is_empty: false,
            },
        ];

        let ancestor_pairs = vec![(0, 1), (0, 2), (0, 3)];
        let res = resolve_directory_contacts_rapier(&dirs, &ancestor_pairs, 25);
        assert_eq!(res.len(), 4);
        assert_eq!(res[0], Vec2::ZERO); // root stayed fixed

        // Non-ancestor pair (1, 2) should respect dir_radius
        let dist_12 = (res[1] - res[2]).length();
        let min_12 = dirs[1].dir_radius + dirs[2].dir_radius;
        assert!(dist_12 >= min_12 - 1e-2);

        // Ancestor pair (0, 1) should respect parent_radius
        let dist_01 = (res[0] - res[1]).length();
        let min_01 = dirs[0].parent_radius + dirs[1].parent_radius;
        assert!(dist_01 >= min_01 - 1e-2);
    }
}
