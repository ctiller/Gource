//! File layout inside a directory: the default ring layout, the per-tick
//! approach to a slot, and the weighted (variable-size) circle solver.
//!
//! File positions are relative to their directory's position.

use crate::kernel::dirs::Radii;
use crate::kernel::fixed::{Fx, IVec2, ONE, PI_Q16, TICK_HZ, div_round, isqrt, sat, sqrt_area};
use crate::kernel::grid::Grid;
use crate::kernel::rng::{pair_dir, salt};
use crate::kernel::trig::{sin_cos, turn_fraction};

/// A ring slot: C++ `calcFileDest` + `updateFilePositions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    /// UNIT-scaled direction `(sin a, cos a)` (note: x = sin, y = cos, as in
    /// C++), `a = (file_no + 0.5) / max_files` of a turn.
    pub dest: IVec2,
    /// Ring radius, Q8.
    pub distance: Fx,
}

/// The slots of `visible` files in order (C++ `updateFilePositions`, hidden
/// files skipped by the caller). Start with `max_files = 1`, `diameter = 1`,
/// `d = 0`, `left = visible`; for each file: emit `Slot { dest(max_files,
/// file_no), distance: d }`, `left -= 1`, `file_no += 1`; when `file_no >=
/// max_files`: `diameter += 1`, `d += file_diameter`, `max_files =
/// max(1, floor(diameter * PI))`, then `max_files = min(max_files, left)`
/// if `left < max_files`, `file_no = 0`.
pub fn ring_slots(visible: u32, file_diameter: Fx) -> Vec<Slot> {
    if visible == 0 {
        return Vec::new();
    }

    let mut slots = Vec::with_capacity(visible as usize);
    let mut max_files: u32 = 1;
    let mut diameter: i64 = 1;
    let mut d: Fx = 0;
    let mut left: u32 = visible;
    let mut file_no: u32 = 0;

    while left > 0 {
        let angle = turn_fraction(2 * file_no as u64 + 1, 2 * max_files as u64);
        let (sin, cos) = sin_cos(angle);
        slots.push(Slot {
            dest: IVec2::new(sin, cos),
            distance: d,
        });
        left -= 1;
        file_no += 1;

        if file_no >= max_files && left > 0 {
            diameter += 1;
            d = d.saturating_add(file_diameter);
            let next_max = ((diameter * PI_Q16) >> 16).max(1) as u32;
            max_files = next_max.min(left);
            file_no = 0;
        }
    }

    slots
}

/// One tick of a file moving to its slot (C++ `RFile::logic`, speed 5):
/// `accel = target - pos`; `pos + accel * 5 / TICK_HZ` (rounded), never
/// overshooting `target`.
pub fn approach(pos: IVec2, target: IVec2) -> IVec2 {
    let accel = target - pos;
    // Step by div_round(accel * 5, TICK_HZ).
    let step_x = div_round(accel.x as i64 * 5, TICK_HZ as i64) as i32;
    let step_y = div_round(accel.y as i64 * 5, TICK_HZ as i64) as i32;

    let next_x = if (step_x == 0 && accel.x != 0)
        || (accel.x > 0 && pos.x + step_x >= target.x)
        || (accel.x < 0 && pos.x + step_x <= target.x)
    {
        target.x
    } else {
        pos.x + step_x
    };

    let next_y = if (step_y == 0 && accel.y != 0)
        || (accel.y > 0 && pos.y + step_y >= target.y)
        || (accel.y < 0 && pos.y + step_y <= target.y)
    {
        target.y
    } else {
        pos.y + step_y
    };

    IVec2::new(next_x, next_y)
}

/// One tick of a weighted file's size animation: `size + (target - size) *
/// 4 / TICK_HZ` (rounded, and snapping to `target` once within 1/256 unit).
pub fn animate_size(size: Fx, target: Fx) -> Fx {
    // Snapping to target once within 1/256 unit (tolerance 1 in Q8)
    if (target as i64 - size as i64).abs() <= 1 {
        return target;
    }

    let delta = target as i64 - size as i64;
    let step = div_round(delta * 4, TICK_HZ as i64);
    if step == 0 {
        return target;
    }
    let next = size as i64 + step;

    if (target as i64 - next).abs() <= 1
        || (delta > 0 && next >= target as i64)
        || (delta < 0 && next <= target as i64)
    {
        target
    } else {
        sat(next)
    }
}

/// A file circle in the weighted solver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Body {
    /// Position relative to the directory, Q8.
    pub pos: IVec2,
    /// Velocity, Q8 units/s.
    pub vel: IVec2,
    /// Collision radius, Q8 (callers floor it at `ONE / 20`).
    pub radius: Fx,
    /// Stable id (RNG key for coincident pairs).
    pub id: u64,
}

/// One tick of the weighted file solver (replaces the Rapier step), in two
/// substeps of `h = 1 / (2 * TICK_HZ)` s.
///
/// Per substep, per body: `vel += -pos * 2h`, clamp `|vel| <= 25` units/s,
/// damp `vel *= (1 - 14h)`, `pos += vel * h`.
///
/// Then up to 16 projection passes over all pairs `i < j`: if `dist < r_i +
/// r_j`, push apart along `unit(p_j - p_i)` (for coincident pairs use
/// [`crate::kernel::rng::pair_dir`] with `salt::FILE_PLACE`) by `overlap * 0.35`,
/// split as `w_i = r_j / (r_i + r_j)` and `w_j = r_i / (r_i + r_j)`; stop
/// early when no pair overlaps.
///
/// Finally zero any `vel` shorter than 0.05 units/s. A single body snaps to
/// the origin with zero velocity.
pub fn step_weighted(bodies: &mut [Body], seed: u64, tick: u64) {
    if bodies.is_empty() {
        return;
    }
    if bodies.len() == 1 {
        bodies[0].pos = IVec2::ZERO;
        bodies[0].vel = IVec2::ZERO;
        return;
    }

    let h_den = 2 * TICK_HZ as i64; // 120
    let max_vel = 25 * ONE; // 25 units/s in Q8
    let vel_cutoff_sq = {
        // 0.05 units/s = ONE * 5 / 100
        let c = div_round(ONE as i64 * 5, 100);
        c * c
    };

    // Two substeps
    for _ in 0..2 {
        for b in bodies.iter_mut() {
            // vel += -pos * 2h => vel += -pos * 2 / 120 = -pos / 60
            let dv_x = div_round(-b.pos.x as i64, TICK_HZ as i64);
            let dv_y = div_round(-b.pos.y as i64, TICK_HZ as i64);
            b.vel.x = sat(b.vel.x as i64 + dv_x);
            b.vel.y = sat(b.vel.y as i64 + dv_y);

            // clamp |vel| <= 25 units/s
            b.vel = b.vel.clamp_len(max_vel);

            // damp vel *= (1 - 14h) = vel * (120 - 14) / 120 = vel * 106 / 120
            b.vel.x = sat(div_round(b.vel.x as i64 * (h_den - 14), h_den));
            b.vel.y = sat(div_round(b.vel.y as i64 * (h_den - 14), h_den));

            // pos += vel * h = pos + vel / 120
            let dp_x = div_round(b.vel.x as i64, h_den);
            let dp_y = div_round(b.vel.y as i64, h_den);
            b.pos.x = sat(b.pos.x as i64 + dp_x);
            b.pos.y = sat(b.pos.y as i64 + dp_y);
        }
    }

    // Up to 16 projection passes over all pairs i < j
    let n = bodies.len();
    for _ in 0..16 {
        let mut had_overlap = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let r_i = bodies[i].radius as i64;
                let r_j = bodies[j].radius as i64;
                let sum_r = r_i + r_j;

                let delta = bodies[j].pos - bodies[i].pos;
                let dist_sq = delta.len_sq();
                if dist_sq < sum_r * sum_r {
                    let dist = isqrt(dist_sq as u64) as i64;
                    if dist < sum_r {
                        had_overlap = true;
                        let overlap = sum_r - dist;
                        // total push = overlap * 0.35 = div_round(overlap * 35, 100)
                        let push_mag = div_round(overlap * 35, 100);

                        // Direction from i to j
                        let dir = if dist == 0 {
                            pair_dir(seed, tick, bodies[i].id, bodies[j].id, salt::FILE_PLACE)
                        } else {
                            delta.unit().unwrap_or(IVec2::ZERO)
                        };

                        // w_i = r_j / sum_r, w_j = r_i / sum_r
                        let push_i_mag = div_round(push_mag * r_j, sum_r);
                        let push_j_mag = push_mag - push_i_mag;

                        // i is pushed along -dir, j along +dir
                        let shift_i = dir.unit_times(push_i_mag);
                        let shift_j = dir.unit_times(push_j_mag);

                        bodies[i].pos -= shift_i;
                        bodies[j].pos += shift_j;
                    }
                }
            }
        }
        if !had_overlap {
            break;
        }
    }

    // Finally zero any vel shorter than 0.05 units/s
    for b in bodies.iter_mut() {
        if b.vel.len_sq() < vel_cutoff_sq {
            b.vel = IVec2::ZERO;
        }
    }
}

/// Initial tight packing of circles with the given radii (replaces the
/// float tangent packer): sort by radius descending (stable by index); put
/// circle 0 at the origin, circle 1 tangent on +x; place each next circle at
/// the candidate tangent to two placed circles that overlaps no placed circle
/// (tolerance 1/256 unit) and minimises `|c| + r` (ties: smaller x, then
/// smaller y); fall back to `(max_extent + r, 0)`. Only consider the 48
/// placed circles with the largest `|p| + r` once more than 48 are placed.
/// Finally centre the bounding box on the origin. Returns positions in the
/// input order.
pub fn pack(radii: &[Fx]) -> Vec<IVec2> {
    if radii.is_empty() {
        return Vec::new();
    }
    if radii.len() == 1 {
        return vec![IVec2::ZERO];
    }

    let n = radii.len();
    // Sort descending by radius, stable by original index
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| radii[b].cmp(&radii[a]).then_with(|| a.cmp(&b)));

    // positions in sorted order
    let mut placed_pos: Vec<IVec2> = Vec::with_capacity(n);
    let mut placed_r: Vec<Fx> = Vec::with_capacity(n);

    // Circle 0 at origin
    placed_pos.push(IVec2::ZERO);
    placed_r.push(radii[order[0]]);

    // Circle 1 tangent on +x
    let r0 = radii[order[0]];
    let r1 = radii[order[1]];
    placed_pos.push(IVec2::new(r0 + r1, 0));
    placed_r.push(r1);

    for k in 2..n {
        let rk = radii[order[k]];
        let placed_count = placed_pos.len();

        // Determine which placed circles form the frontier (at most 48 with largest |p| + r)
        let frontier_indices: Vec<usize> = if placed_count <= 48 {
            (0..placed_count).collect()
        } else {
            let mut cand_indices: Vec<usize> = (0..placed_count).collect();
            cand_indices.sort_by(|&a, &b| {
                let ext_a = placed_pos[a].length() as i64 + placed_r[a] as i64;
                let ext_b = placed_pos[b].length() as i64 + placed_r[b] as i64;
                ext_b.cmp(&ext_a).then_with(|| a.cmp(&b))
            });
            cand_indices.truncate(48);
            cand_indices
        };

        let mut best_candidate: Option<IVec2> = None;
        let mut best_key: (i64, i32, i32) = (i64::MAX, i32::MAX, i32::MAX);

        let f_len = frontier_indices.len();
        for fi in 0..f_len {
            for fj in (fi + 1)..f_len {
                let i = frontier_indices[fi];
                let j = frontier_indices[fj];

                let pi = placed_pos[i];
                let pj = placed_pos[j];
                let ri = placed_r[i];
                let rj = placed_r[j];

                let dik = ri as i64 + rk as i64;
                let djk = rj as i64 + rk as i64;

                let delta = pj - pi;
                let d_sq = delta.len_sq();
                if d_sq == 0 {
                    continue;
                }
                let d = isqrt(d_sq as u64) as i64;
                if d == 0 {
                    continue;
                }

                // Triangle inequality check
                if d > dik + djk || d < (dik - djk).abs() {
                    continue;
                }

                // a = (dik^2 - djk^2 + d^2) / (2 * d)
                let a = div_round(dik * dik - djk * djk + d_sq, 2 * d);
                let h_sq = dik * dik - a * a;
                if h_sq < 0 {
                    continue;
                }
                let h = isqrt(h_sq as u64) as i64;

                // u = unit(pj - pi) in UNIT scale
                let Some(u) = delta.unit() else {
                    continue;
                };
                // perp n = (-u.y, u.x)
                let n_vec = IVec2::new(-u.y, u.x);

                // c = pi + u * a ± n * h
                let ua = u.unit_times(a);
                let nh = n_vec.unit_times(h);

                let c1 = pi + ua + nh;
                let c2 = pi + ua - nh;

                for c in [c1, c2] {
                    // Check overlap against all placed circles with tolerance 1/256 unit (= 1 in Q8)
                    let mut overlaps = false;
                    for p in 0..placed_count {
                        let req_dist = placed_r[p] as i64 + rk as i64;
                        let actual_dist_sq = (c - placed_pos[p]).len_sq();
                        let min_allowed = (req_dist - 1).max(0);
                        if actual_dist_sq < min_allowed * min_allowed {
                            overlaps = true;
                            break;
                        }
                    }

                    if !overlaps {
                        let ext = c.length() as i64 + rk as i64;
                        let key = (ext, c.x, c.y);
                        if key < best_key {
                            best_key = key;
                            best_candidate = Some(c);
                        }
                    }
                }
            }
        }

        let chosen_pos = match best_candidate {
            Some(c) => c,
            None => {
                // Fall back to (max_extent + r, 0)
                let mut max_extent: i64 = 0;
                for p in 0..placed_count {
                    let ext = placed_pos[p].length() as i64 + placed_r[p] as i64;
                    if ext > max_extent {
                        max_extent = ext;
                    }
                }
                IVec2::new(sat(max_extent + rk as i64), 0)
            }
        };

        placed_pos.push(chosen_pos);
        placed_r.push(rk);
    }

    // Finally centre the bounding box on the origin
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;

    for i in 0..n {
        let p = placed_pos[i];
        let r = placed_r[i];
        min_x = min_x.min(p.x - r);
        max_x = max_x.max(p.x + r);
        min_y = min_y.min(p.y - r);
        max_y = max_y.max(p.y + r);
    }

    let mid_x = div_round(min_x as i64 + max_x as i64, 2) as i32;
    let mid_y = div_round(min_y as i64 + max_y as i64, 2) as i32;
    let offset = IVec2::new(mid_x, mid_y);

    for p in placed_pos.iter_mut() {
        *p -= offset;
    }

    // Return positions in original input order
    let mut result = vec![IVec2::ZERO; n];
    for (sorted_idx, &orig_idx) in order.iter().enumerate() {
        result[orig_idx] = placed_pos[sorted_idx];
    }
    result
}

/// A directory in weighted-mode contact resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirDisc {
    pub pos: IVec2,
    /// Contact radius, Q8 (the caller passes `max(parent_radius, 10 units)`).
    pub radius: Fx,
    /// The root never moves.
    pub fixed: bool,
    /// Empty directories are ignored.
    pub empty: bool,
    pub id: u64,
}

/// Push overlapping directory discs apart (replaces the Rapier contact
/// pass): up to 12 passes; for each overlapping non-empty pair, move both by
/// half the overlap along the centre line, or the non-fixed one by the full
/// overlap if the other is fixed. Use [`crate::kernel::grid::Grid`] for the pair
/// search. Returns the new positions in input order.
pub fn resolve_discs(discs: &[DirDisc], seed: u64, tick: u64) -> Vec<IVec2> {
    if discs.is_empty() {
        return Vec::new();
    }

    let mut positions: Vec<IVec2> = discs.iter().map(|d| d.pos).collect();
    let n = discs.len();
    let mut query_buf = Vec::new();

    for _ in 0..12 {
        let shift = Grid::auto_shift(discs.iter().map(|d| d.radius));
        let grid = Grid::build(
            (0..n).filter(|&i| !discs[i].empty).map(|i| {
                let r = discs[i].radius;
                let p = positions[i];
                (
                    i as u32,
                    IVec2::new(p.x - r, p.y - r),
                    IVec2::new(p.x + r, p.y + r),
                )
            }),
            shift,
        );

        let mut pairs = Vec::new();
        for i in 0..n {
            if discs[i].empty {
                continue;
            }
            let r = discs[i].radius;
            let p = positions[i];
            grid.query(
                IVec2::new(p.x - r, p.y - r),
                IVec2::new(p.x + r, p.y + r),
                &mut query_buf,
            );
            for &j_u32 in &query_buf {
                let j = j_u32 as usize;
                if i < j && !discs[j].empty {
                    pairs.push((i, j));
                }
            }
        }

        pairs.sort_unstable();
        pairs.dedup();

        let mut had_collision = false;
        for (i, j) in pairs {
            let r_i = discs[i].radius as i64;
            let r_j = discs[j].radius as i64;
            let sum_r = r_i + r_j;

            let delta = positions[j] - positions[i];
            let dist_sq = delta.len_sq();
            if dist_sq < sum_r * sum_r {
                let dist = isqrt(dist_sq as u64) as i64;
                if dist < sum_r {
                    had_collision = true;
                    let overlap = sum_r - dist;

                    let dir = if dist == 0 {
                        pair_dir(seed, tick, discs[i].id, discs[j].id, salt::DIR_COINCIDENT)
                    } else {
                        delta.unit().unwrap_or(IVec2::ZERO)
                    };

                    match (discs[i].fixed, discs[j].fixed) {
                        (true, false) => {
                            positions[j] += dir.unit_times(overlap);
                        }
                        (false, true) => {
                            positions[i] -= dir.unit_times(overlap);
                        }
                        (false, false) => {
                            let half_overlap = div_round(overlap, 2);
                            let remaining = overlap - half_overlap;
                            positions[i] -= dir.unit_times(half_overlap);
                            positions[j] += dir.unit_times(remaining);
                        }
                        (true, true) => {}
                    }
                }
            }
        }

        if !had_collision {
            break;
        }
    }

    positions
}

/// Weighted-mode radii of a directory (C++-port `calc_weighted_radius`):
/// `files` are `(distance_from_centre + radius, radius)` pairs (Q8) of the
/// visible files. `file_area = max(sum(PI * r^2), max_extent^2)`; `area =
/// file_area + children_area`; `radius = max(10 units, max(ONE,
/// sqrt(area)) * padding)`; `parent_radius = max(10 units, max(ONE,
/// sqrt(file_area) * padding), max_extent * padding)`; finally `radius =
/// max(radius, parent_radius)`.
pub fn weighted_radii(files: &[(Fx, Fx)], children_area: i64, padding: Fx) -> Radii {
    let mut sum_pi_r2: i64 = 0;
    let mut max_extent: Fx = 0;

    for &(dist_plus_r, r) in files {
        if dist_plus_r > max_extent {
            max_extent = dist_plus_r;
        }
        // r is Q8 length -> r^2 is Q16 area
        let r64 = r as i64;
        let r_sq = r64 * r64;
        let pi_r_sq = (r_sq * PI_Q16) >> 16;
        sum_pi_r2 += pi_r_sq;
    }

    let max_extent_sq = max_extent as i64 * max_extent as i64;
    let file_area = sum_pi_r2.max(max_extent_sq);
    let area = file_area.saturating_add(children_area);

    let ten_units: Fx = 10 * ONE;

    let area_len = sqrt_area(area).max(ONE);
    let dir_rad = (div_round(area_len as i64 * padding as i64, ONE as i64) as Fx).max(ten_units);

    let file_area_len = sqrt_area(file_area).max(ONE);
    let parent_rad_from_area = div_round(file_area_len as i64 * padding as i64, ONE as i64) as Fx;
    let parent_rad_from_extent = div_round(max_extent as i64 * padding as i64, ONE as i64) as Fx;
    let parent_radius = ten_units
        .max(parent_rad_from_area)
        .max(parent_rad_from_extent);

    let radius = dir_rad.max(parent_radius);

    Radii {
        area,
        radius,
        parent_radius,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UNIT;

    #[test]
    fn ring_slots_counts_and_properties() {
        for n in [0, 1, 2, 7, 20] {
            let slots = ring_slots(n, 8 * ONE);
            assert_eq!(slots.len(), n as usize);
            if n > 0 {
                // First ring has 1 slot at distance 0
                assert_eq!(slots[0].distance, 0);
            }
            if n > 1 {
                // Second ring distance = file_diameter
                assert_eq!(slots[1].distance, 8 * ONE);
            }
            // All slots have unit length directions
            for s in &slots {
                let len = s.dest.length();
                assert!((len - UNIT).abs() <= 2, "slot len = {len}");
            }
        }
    }

    #[test]
    fn ring_slots_matches_golden_math() {
        let slots = ring_slots(7, 8 * ONE);
        assert_eq!(slots[0].distance, 0);
        for s in &slots[1..7] {
            assert_eq!(s.distance, 8 * ONE);
        }
    }

    #[test]
    fn approach_converges_and_never_overshoots() {
        let start = IVec2::new(0, 0);
        let target = IVec2::new(100 * ONE, -50 * ONE);

        let mut curr = start;
        for _ in 0..200 {
            let next = approach(curr, target);
            assert!(
                (target.x - next.x).abs() <= (target.x - curr.x).abs(),
                "overshot x"
            );
            assert!(
                (target.y - next.y).abs() <= (target.y - curr.y).abs(),
                "overshot y"
            );
            curr = next;
        }
        assert_eq!(curr, target);

        // Zero delta
        assert_eq!(approach(target, target), target);

        // Negative deltas
        let curr_neg = approach(IVec2::new(10, 20), IVec2::new(0, 0));
        assert!(curr_neg.x <= 10 && curr_neg.x >= 0);
        assert!(curr_neg.y <= 20 && curr_neg.y >= 0);
    }

    #[test]
    fn animate_size_up_down_snap() {
        // Animate up
        let mut size = 0;
        let target = 10 * ONE;
        for _ in 0..200 {
            size = animate_size(size, target);
        }
        assert_eq!(size, target);

        // Animate down
        let mut size_down = 20 * ONE;
        let target_down = 2 * ONE;
        for _ in 0..200 {
            size_down = animate_size(size_down, target_down);
        }
        assert_eq!(size_down, target_down);

        // Snap within 1/256 unit (= 1 in Q8)
        assert_eq!(animate_size(100, 101), 101);
        assert_eq!(animate_size(101, 100), 100);
    }

    #[test]
    fn step_weighted_single_body_snaps_to_origin() {
        let mut b = [Body {
            pos: IVec2::new(50 * ONE, -20 * ONE),
            vel: IVec2::new(10 * ONE, 10 * ONE),
            radius: 4 * ONE,
            id: 1,
        }];
        step_weighted(&mut b, 42, 1);
        assert_eq!(b[0].pos, IVec2::ZERO);
        assert_eq!(b[0].vel, IVec2::ZERO);

        let mut empty: [Body; 0] = [];
        step_weighted(&mut empty, 42, 1);
    }

    #[test]
    fn step_weighted_coincident_bodies_separate_deterministically() {
        let mut b1 = [
            Body {
                pos: IVec2::ZERO,
                vel: IVec2::ZERO,
                radius: 4 * ONE,
                id: 10,
            },
            Body {
                pos: IVec2::ZERO,
                vel: IVec2::ZERO,
                radius: 4 * ONE,
                id: 20,
            },
        ];
        let mut b2 = b1;

        step_weighted(&mut b1, 999, 1);
        step_weighted(&mut b2, 999, 1);

        // Determinism
        assert_eq!(b1, b2);

        // Separated
        assert_ne!(b1[0].pos, b1[1].pos);
        let dist = (b1[0].pos - b1[1].pos).length();
        assert!(dist > 0);
    }

    #[test]
    fn step_weighted_non_overlapping_and_small_vel_zeroed() {
        let mut bodies = vec![
            Body {
                pos: IVec2::new(10 * ONE, 0),
                vel: IVec2::new(0, 0),
                radius: 5 * ONE,
                id: 1,
            },
            Body {
                pos: IVec2::new(-10 * ONE, 0),
                vel: IVec2::new(0, 0),
                radius: 5 * ONE,
                id: 2,
            },
            Body {
                pos: IVec2::new(0, 5 * ONE),
                vel: IVec2::new(0, 0),
                radius: 4 * ONE,
                id: 3,
            },
        ];

        for t in 0..100 {
            step_weighted(&mut bodies, 1234, t);
        }

        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                let dist = (bodies[i].pos - bodies[j].pos).length();
                let min_dist = bodies[i].radius + bodies[j].radius;
                // non-overlapping within 2 Q8 units tolerance
                assert!(
                    dist >= min_dist - 2,
                    "overlap: dist {dist} < min_dist {min_dist}"
                );
            }
        }
    }

    #[test]
    fn pack_handles_various_sizes_and_preserves_order() {
        // 0 radii
        assert!(pack(&[]).is_empty());

        // 1 radius
        assert_eq!(pack(&[5 * ONE]), vec![IVec2::ZERO]);

        // 2 radii
        let p2 = pack(&[4 * ONE, 6 * ONE]);
        assert_eq!(p2.len(), 2);
        let dist_2 = (p2[0] - p2[1]).length();
        assert!((dist_2 - 10 * ONE).abs() <= 2);

        // 100 radii of varied sizes
        let mut radii = Vec::new();
        for i in 0..100 {
            radii.push(((i * 7 % 15) + 2) * ONE);
        }
        let placed = pack(&radii);
        assert_eq!(placed.len(), 100);

        // Check no overlaps within 2 Q8 units
        for i in 0..100 {
            for j in (i + 1)..100 {
                let dist = (placed[i] - placed[j]).length();
                let min_dist = radii[i] + radii[j];
                assert!(
                    dist >= min_dist - 2,
                    "pack overlap between {i} and {j}: dist = {dist}, min_dist = {min_dist}"
                );
            }
        }

        // Check centred bounding box
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        for i in 0..100 {
            min_x = min_x.min(placed[i].x - radii[i]);
            max_x = max_x.max(placed[i].x + radii[i]);
            min_y = min_y.min(placed[i].y - radii[i]);
            max_y = max_y.max(placed[i].y + radii[i]);
        }
        let center_x = (min_x as i64 + max_x as i64) / 2;
        let center_y = (min_y as i64 + max_y as i64) / 2;
        assert!(center_x.abs() <= 1);
        assert!(center_y.abs() <= 1);
    }

    #[test]
    fn resolve_discs_tests() {
        assert!(resolve_discs(&[], 0, 0).is_empty());

        let discs = vec![
            DirDisc {
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                fixed: true,
                empty: false,
                id: 1,
            },
            DirDisc {
                pos: IVec2::new(5 * ONE, 0),
                radius: 10 * ONE,
                fixed: false,
                empty: false,
                id: 2,
            },
            DirDisc {
                pos: IVec2::new(100 * ONE, 100 * ONE),
                radius: 5 * ONE,
                fixed: false,
                empty: true,
                id: 3,
            },
        ];

        let res = resolve_discs(&discs, 42, 1);
        assert_eq!(res.len(), 3);
        // Fixed disc never moves
        assert_eq!(res[0], IVec2::ZERO);

        // Disc 1 and 2 separated
        let dist_12 = (res[1] - res[0]).length();
        assert!(dist_12 >= 20 * ONE - 2, "dist_12 = {dist_12}");

        // Empty disc 3 untouched
        assert_eq!(res[2], IVec2::new(100 * ONE, 100 * ONE));

        // Two non-fixed overlapping discs both move
        let non_fixed = vec![
            DirDisc {
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                fixed: false,
                empty: false,
                id: 10,
            },
            DirDisc {
                pos: IVec2::new(2 * ONE, 0),
                radius: 10 * ONE,
                fixed: false,
                empty: false,
                id: 20,
            },
        ];
        let res_nf = resolve_discs(&non_fixed, 42, 1);
        assert_ne!(res_nf[0], IVec2::ZERO);
        assert_ne!(res_nf[1], IVec2::new(2 * ONE, 0));
        let dist_nf = (res_nf[1] - res_nf[0]).length();
        assert!(dist_nf >= 20 * ONE - 2);

        // Coincident discs
        let coincident = vec![
            DirDisc {
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                fixed: false,
                empty: false,
                id: 100,
            },
            DirDisc {
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                fixed: false,
                empty: false,
                id: 200,
            },
        ];
        let res_c = resolve_discs(&coincident, 777, 2);
        assert_ne!(res_c[0], res_c[1]);

        // Two fixed discs do not move even if overlapping
        let both_fixed = vec![
            DirDisc {
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                fixed: true,
                empty: false,
                id: 11,
            },
            DirDisc {
                pos: IVec2::new(ONE, 0),
                radius: 10 * ONE,
                fixed: true,
                empty: false,
                id: 22,
            },
        ];
        let res_bf = resolve_discs(&both_fixed, 1, 1);
        assert_eq!(res_bf[0], IVec2::ZERO);
        assert_eq!(res_bf[1], IVec2::new(ONE, 0));
    }

    #[test]
    fn step_weighted_determinism() {
        let b1 = [
            Body {
                pos: IVec2::new(10 * ONE, 5 * ONE),
                vel: IVec2::new(2 * ONE, -ONE),
                radius: 5 * ONE,
                id: 1,
            },
            Body {
                pos: IVec2::new(8 * ONE, 4 * ONE),
                vel: IVec2::new(-ONE, 3 * ONE),
                radius: 5 * ONE,
                id: 2,
            },
        ];
        let mut run1 = b1;
        let mut run2 = b1;
        step_weighted(&mut run1, 555, 10);
        step_weighted(&mut run2, 555, 10);
        assert_eq!(run1, run2);
    }

    #[test]
    fn weighted_radii_formula_cases() {
        // Empty files
        let r0 = weighted_radii(&[], 0, 384);
        assert_eq!(r0.radius, 10 * ONE);
        assert_eq!(r0.parent_radius, 10 * ONE);

        // Small files below min 10 units
        let small_files = [(2 * ONE, ONE)];
        let r_small = weighted_radii(&small_files, 0, 384);
        assert_eq!(r_small.radius, 10 * ONE);
        assert_eq!(r_small.parent_radius, 10 * ONE);

        // Large cluster
        let large_files = [(50 * ONE, 10 * ONE), (30 * ONE, 8 * ONE)];
        let r_large = weighted_radii(&large_files, 1000 * ONE as i64 * ONE as i64, 384);
        assert!(r_large.radius > 10 * ONE);
        assert!(r_large.parent_radius > 10 * ONE);
        assert!(r_large.radius >= r_large.parent_radius);
    }
}
