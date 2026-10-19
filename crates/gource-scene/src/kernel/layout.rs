//! Pluggable layout kernels (Q24.8 fixed point, zero floating-point arithmetic):
//!
//! - [`solve_settled_tree`]: Bottom-up + top-down crossing-free, non-overlapping
//!   multi-ring radial tree solver (`O(N)` in directories). Handles both deep
//!   trees and high-fanout "spider" directories (50–100+ child branches) by
//!   allocating angular wedges proportional to child subtree radii and spilling
//!   into staggered concentric arc-rings when a single ring's angular budget fills.
//! - [`segments_intersect`] & [`uncross_edges`]: Exact 64-bit integer segment
//!   intersection detection and edge-uncrossing velocity impulses.
//! - [`step_settled_pos`]: Critically damped spring/exponential approach to a
//!   settled target position, snapping to exact rest within deadband.
//! - [`step_world_files`]: World-space per-file particle simulation with
//!   same-directory cohesion, same-directory contact packing, cross-directory
//!   file repulsion, and asymptotic velocity damping to full rest.

use crate::kernel::dirs::Radii;
use crate::kernel::fixed::{Acc, Fx, IVec2, ONE, PI_Q16, TICK_HZ, UNIT, div_round, mul_fx, sat};
use crate::kernel::trig::{GOLDEN_ANGLE, SIN, direction};

/// Inverse sine `asin(clamp(num / den, 0, 1))` in `u16` turn units
/// (`0..=16384`, where `65536 = 360°` and `16384 = 90°`).
#[inline]
pub fn asin_u16(num: i64, den: i64) -> u16 {
    if den <= 0 || num <= 0 {
        return 0;
    }
    if num >= den {
        return 16384;
    }
    let target = div_round(num * UNIT as i64, den).clamp(0, UNIT as i64) as i32;
    let idx = SIN[..=1024].partition_point(|&s| s < target);
    ((idx << 4).min(16384)) as u16
}

/// Polar angle `atan2(v.y, v.x)` in `u16` turn units (`0..=65535`).
#[inline]
pub fn atan2_u16(v: IVec2) -> u16 {
    let Some(u) = v.unit() else {
        return 0;
    };
    let q = asin_u16(u.y.unsigned_abs() as i64, UNIT as i64);
    match (v.x >= 0, v.y >= 0) {
        (true, true) => q,
        (false, true) => 32768u16.wrapping_sub(q),
        (false, false) => 32768u16.wrapping_add(q),
        (true, false) => 0u16.wrapping_sub(q),
    }
}

/// Signed shortest angular difference `a - b` in `[-32768, 32767]`.
#[inline]
pub fn angle_diff_i32(a: u16, b: u16) -> i32 {
    (a.wrapping_sub(b) as i16) as i32
}

/// Rotate a local offset `v` by `angle` (`u16` turn units).
#[inline]
pub fn rotate_by_u16(v: IVec2, angle: u16) -> IVec2 {
    if angle == 0 || v == IVec2::ZERO {
        return v;
    }
    let u = direction(angle);
    let (vx, vy) = (v.x as i64, v.y as i64);
    let (c, s) = (u.x as i64, u.y as i64);
    IVec2::new(
        sat(div_round(vx * c - vy * s, UNIT as i64)),
        sat(div_round(vx * s + vy * c, UNIT as i64)),
    )
}

/// Directory input to [`solve_settled_tree`], ordered in tree preorder (`0` is root).
#[derive(Clone, Debug)]
pub struct TreeDirIn {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Radius (Q8) of this directory's own packed visible files (`0` if none).
    pub own_file_radius: Fx,
    /// Number of direct visible files in this directory.
    pub visible_files: u32,
    /// True if this directory or any of its descendants has visible files.
    pub visible: bool,
}

/// Settled directory output from [`solve_settled_tree`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeDirOut {
    /// Deterministic crossing-free world target position (Q8).
    pub target_pos: IVec2,
    /// Outward ray angle from parent (`u16` turn units).
    pub outward_angle: u16,
    /// Directory radii (`parent_radius` = own file radius, `radius` = subtree bounding radius).
    pub radii: Radii,
}

/// Place a single ring of child subtrees with radii `child_radii` outside `r_inner`
/// within angular budget `cone_budget` (`u16` units, up to `65536`).
///
/// Returns `(vec_of_(distance, signed_local_angle_offset), ring_outer_radius)`.
fn place_ring_batch(
    child_radii: &[Fx],
    r_inner: Fx,
    cone_budget: i32,
    full_circle: bool,
    gap: Fx,
    stagger_shift: i32,
) -> (Vec<(Fx, i32)>, Fx) {
    let k = child_radii.len();
    if k == 0 {
        return (Vec::new(), r_inner);
    }
    let half_gap = (gap / 2).max(ONE);

    if k == 1 {
        let r0 = child_radii[0].max(2 * ONE);
        let d0 = r_inner.saturating_add(r0).saturating_add(half_gap);
        let outer = d0.saturating_add(r0);
        return (vec![(d0, stagger_shift)], outer);
    }

    // Sum of effective diameters to upper-bound the ring baseline radius.
    let mut chord_sum: i64 = 0;
    let mut max_r: Fx = 2 * ONE;
    for &r in child_radii {
        let reff = r.max(2 * ONE).saturating_add(half_gap);
        chord_sum += 2 * reff as i64;
        max_r = max_r.max(r);
    }

    let sum_angles_at = |r_base: Fx| -> i32 {
        let mut total = 0i32;
        for &r in child_radii {
            let reff = r.max(2 * ONE).saturating_add(half_gap);
            let d = r_base.saturating_add(reff).max(reff + ONE);
            let alpha = asin_u16(reff as i64, d as i64) as i32;
            total = total.saturating_add(2 * alpha);
        }
        total
    };

    let budget = cone_budget.clamp(4096, 65536);
    let mut r_base = r_inner.max(2 * ONE);
    if sum_angles_at(r_base) > budget {
        // Upper bound from arc length: R * (budget / 65536) * 2*pi >= chord_sum
        // Since 2*pi > 6, chord_sum * 65536 / (budget * 6) + max_r is a safe upper bound.
        let r_extra =
            div_round(chord_sum * 65536, budget as i64 * 6).min(1_000_000 * ONE as i64) as Fx;
        let mut lo = r_base;
        let mut hi = r_base.saturating_add(r_extra).saturating_add(max_r);
        for _ in 0..20 {
            let mid = lo + (hi - lo) / 2;
            if sum_angles_at(mid) <= budget {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        r_base = hi;
    }

    let mut alphas = Vec::with_capacity(k);
    let mut dists = Vec::with_capacity(k);
    let mut total_alpha_span = 0i64;
    let mut outer_r = r_inner;

    for &r in child_radii {
        let r_clean = r.max(2 * ONE);
        let reff = r_clean.saturating_add(half_gap);
        let d = r_base.saturating_add(reff);
        let alpha = (asin_u16(reff as i64, d as i64) as i32).max(128);
        alphas.push(alpha);
        dists.push(d);
        total_alpha_span += 2 * alpha as i64;
        outer_r = outer_r.max(d.saturating_add(r_clean));
    }

    let mut placements = Vec::with_capacity(k);
    if full_circle {
        // Distribute across the full 65536 circle proportional to each child's alpha_m.
        let span_den = total_alpha_span.max(1);
        let mut cursor = stagger_shift;
        for m in 0..k {
            let w = div_round(2 * alphas[m] as i64 * 65536, span_den) as i32;
            let center_angle = cursor + w / 2;
            placements.push((dists[m], center_angle));
            cursor += w;
        }
    } else {
        // Distribute symmetrically inside [-span/2, +span/2], keeping natural tight span
        // when it fits inside budget, with only a small gentle margin per child.
        let natural = total_alpha_span.min(budget as i64) as i32;
        let extra_per_child = ((budget - natural).max(0) / (k as i32 * 4)).min(1024);
        let actual_span = (natural + extra_per_child * (k as i32)).min(budget);
        let mut cursor = -actual_span / 2 + stagger_shift;
        for m in 0..k {
            let w = 2 * alphas[m] + extra_per_child;
            let center_angle = cursor + w / 2;
            placements.push((dists[m], center_angle));
            cursor += w;
        }
    }

    (placements, outer_r)
}

/// Compute a deterministic, crossing-free, non-overlapping layout for the directory tree,
/// followed by [`optimize_settled_layout`] to eliminate any crossed edges and compact
/// subtrees to minimize total layout size.
///
/// `dirs` must be ordered in tree preorder (`0` is root, every parent precedes its children).
pub fn solve_settled_tree(dirs: &[TreeDirIn], padding: Fx, min_dir_size: Fx) -> Vec<TreeDirOut> {
    let mut out = solve_settled_tree_initial(dirs, padding, min_dir_size);
    optimize_settled_layout(dirs, &mut out, padding, min_dir_size);
    out
}

/// Stage 1: Cone-free bottom-up + top-down multi-ring radial placement.
pub fn solve_settled_tree_initial(
    dirs: &[TreeDirIn],
    padding: Fx,
    min_dir_size: Fx,
) -> Vec<TreeDirOut> {
    let n = dirs.len();
    if n == 0 {
        return Vec::new();
    }

    let mut out = vec![TreeDirOut::default(); n];
    // Per-directory local polar placement `(distance_q8, local_angle_offset_i32)` for each child.
    let mut local_child_polar: Vec<Vec<(usize, Fx, i32)>> = vec![Vec::new(); n];
    let gap = 5 * ONE;

    // Pass 1: Bottom-up subtree radius & local child ring packing.
    for i in (0..n).rev() {
        let d = &dirs[i];
        let own_r = if d.visible_files > 0 {
            d.own_file_radius.max(5 * ONE)
        } else {
            0
        };
        let parent_r_view = own_r.max(min_dir_size / 2).max(4 * ONE);
        let own_area_q16 = div_round(PI_Q16 * (own_r as i64 / 4).pow(2), 1 << 12);

        let vis_children: Vec<usize> = d
            .children
            .iter()
            .copied()
            .filter(|&c| c < n && dirs[c].visible)
            .collect();

        let children_area_q16: i64 = vis_children.iter().map(|&c| out[c].radii.area).sum();
        let total_area_q16 = own_area_q16.saturating_add(children_area_q16);

        if !d.visible {
            out[i].radii = Radii {
                area: 0,
                radius: min_dir_size,
                parent_radius: parent_r_view,
            };
            continue;
        }

        if vis_children.is_empty() {
            let pad_r = mul_fx(own_r.max(8 * ONE), padding).max(own_r + 3 * ONE);
            out[i].radii = Radii {
                area: total_area_q16,
                radius: pad_r.max(min_dir_size),
                parent_radius: parent_r_view,
            };
            continue;
        }

        let is_root = d.parent.is_none();
        let k = vis_children.len();
        // Cone-free 360-degree placement: every directory uses the full circle except
        // a narrow 22.5-degree corridor around the incoming parent edge.
        let cone_budget = if is_root {
            65536
        } else if k == 1 {
            0
        } else {
            61440 // 337.5 deg
        };

        let mut r_inner = if own_r > 0 {
            own_r.saturating_add(gap)
        } else {
            6 * ONE
        };

        let child_radii: Vec<Fx> = vis_children
            .iter()
            .map(|&c| out[c].radii.radius.max(6 * ONE))
            .collect();

        let mut placements = Vec::with_capacity(k);
        let mut max_outer = r_inner;

        // Up to 12 children fit cleanly on a single proportional-wedge ring;
        // > 12 children (spider directories) pack into concentric staggered arc-rings!
        if k <= 12 {
            let (ring_pos, outer) =
                place_ring_batch(&child_radii, r_inner, cone_budget, is_root, gap, 0);
            for (idx, (dist, ang)) in vis_children.iter().copied().zip(ring_pos) {
                placements.push((idx, dist, ang));
            }
            max_outer = max_outer.max(outer);
        } else {
            // Multi-ring spider packing:
            let avg_r: Fx = (child_radii.iter().map(|&r| r as i64).sum::<i64>() / k as i64) as Fx;
            r_inner = r_inner.max(avg_r);

            let mut start = 0usize;
            let mut ring_idx = 0usize;
            while start < k {
                let half_gap = (gap / 2).max(ONE);
                let mut angle_acc = 0i32;
                let mut end = start;
                let min_on_ring = (8 + ring_idx * 4).min(k - start);

                while end < k {
                    let r_c = child_radii[end].saturating_add(half_gap);
                    let d_est = r_inner.saturating_add(r_c).max(r_c + ONE);
                    let a_est = 2 * asin_u16(r_c as i64, d_est as i64) as i32;
                    if end - start >= min_on_ring && angle_acc + a_est > cone_budget {
                        break;
                    }
                    angle_acc += a_est;
                    end += 1;
                }

                let batch = &child_radii[start..end];
                let stagger = if ring_idx % 2 == 1 {
                    cone_budget / ((2 * batch.len().max(1)) as i32 * 2)
                } else {
                    0
                };
                let (ring_pos, outer) =
                    place_ring_batch(batch, r_inner, cone_budget, is_root, gap, stagger);
                for (offset, (dist, ang)) in ring_pos.into_iter().enumerate() {
                    placements.push((vis_children[start + offset], dist, ang));
                }
                max_outer = max_outer.max(outer);
                r_inner = outer.saturating_add(gap);
                start = end;
                ring_idx += 1;
            }
        }

        local_child_polar[i] = placements;
        out[i].radii = Radii {
            area: total_area_q16,
            radius: max_outer.saturating_add(2 * ONE).max(min_dir_size),
            parent_radius: parent_r_view,
        };
    }

    // Pass 2: Top-down world target positions and outward angles.
    out[0].target_pos = IVec2::ZERO;
    out[0].outward_angle = 0;

    for i in 0..n {
        let p_pos = out[i].target_pos;
        let p_ang = out[i].outward_angle;
        for &(child_idx, dist, local_ang) in &local_child_polar[i] {
            let world_ang = p_ang.wrapping_add(local_ang as u16);
            let offset = direction(world_ang).unit_times(dist as i64);
            out[child_idx].target_pos = p_pos + offset;
            out[child_idx].outward_angle = world_ang;
        }
        // Hidden children stay at their parent's target position until made visible.
        for &child_idx in &dirs[i].children {
            if child_idx < n && !dirs[child_idx].visible {
                out[child_idx].target_pos = p_pos;
                out[child_idx].outward_angle = p_ang;
            }
        }
    }

    out
}

/// Squared distance from point `p` to segment `(a, b)` in `i64` (Q16).
#[inline]
pub fn point_segment_dist_sq(p: IVec2, a: IVec2, b: IVec2) -> i64 {
    let ab = b - a;
    let ap = p - a;
    let ab_len2 = ab.len_sq();
    if ab_len2 <= 0 {
        return ap.len_sq();
    }
    let dot = ap.x as i64 * ab.x as i64 + ap.y as i64 * ab.y as i64;
    if dot <= 0 {
        return ap.len_sq();
    }
    if dot >= ab_len2 {
        return (p - b).len_sq();
    }
    let proj_x = a.x as i64 + div_round(ab.x as i64 * dot, ab_len2);
    let proj_y = a.y as i64 + div_round(ab.y as i64 * dot, ab_len2);
    let dx = p.x as i64 - proj_x;
    let dy = p.y as i64 - proj_y;
    dx * dx + dy * dy
}

/// Stage 2: Continuous first-contact subtree compaction, obstacle rolling, and 2-opt
/// edge uncrossing optimizer.
///
/// Minimizes total edge length and layout bounding radius while strictly maintaining:
/// 1. Pairwise non-overlap of every visible directory's own file-cluster disc (`parent_radius`).
/// 2. Zero edge crossings (`!segments_intersect`) across all visible tree edges.
/// 3. Node-to-edge clearance between visible directories and non-incident tree edges.
pub fn optimize_settled_layout(
    dirs: &[TreeDirIn],
    out: &mut [TreeDirOut],
    padding: Fx,
    min_dir_size: Fx,
) {
    let n = dirs.len();
    if n <= 1 || out.len() < n {
        return;
    }

    let gap = mul_fx(4 * ONE, padding).max(3 * ONE);
    let edge_clear_pad = 2 * ONE;

    let disc_r: Vec<Fx> = (0..n)
        .map(|i| {
            out[i]
                .radii
                .parent_radius
                .max(if dirs[i].visible_files > 0 {
                    dirs[i].own_file_radius.max(5 * ONE)
                } else {
                    (min_dir_size / 3).max(4 * ONE)
                })
        })
        .collect();

    // Build visible node list and visible subtree node sets bottom-up.
    let vis_nodes: Vec<usize> = (0..n).filter(|&i| dirs[i].visible).collect();
    if vis_nodes.len() <= 1 {
        return;
    }

    let mut sub_vis: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_sub_flag: Vec<Vec<bool>> = vec![vec![false; n]; n];
    for i in (0..n).rev() {
        if !dirs[i].visible {
            continue;
        }
        let mut s = vec![i];
        in_sub_flag[i][i] = true;
        for &c in &dirs[i].children {
            if c < n && dirs[c].visible {
                for &w in &sub_vis[c] {
                    in_sub_flag[i][w] = true;
                    s.push(w);
                }
            }
        }
        sub_vis[i] = s;
    }

    let mut out_vis: Vec<Vec<usize>> = vec![Vec::new(); n];
    for u in 1..n {
        if dirs[u].visible {
            out_vis[u] = vis_nodes
                .iter()
                .copied()
                .filter(|&z| !in_sub_flag[u][z])
                .collect();
        }
    }

    // Collect all visible tree edges `(parent, child)`.
    let mut vis_edges: Vec<(usize, usize)> = Vec::with_capacity(vis_nodes.len());
    for &u in &vis_nodes {
        if let Some(p) = dirs[u].parent
            && p < n
            && dirs[p].visible
        {
            vis_edges.push((p, u));
        }
    }

    let mut pos: Vec<IVec2> = out.iter().map(|o| o.target_pos).collect();

    // Phase A: 2-opt uncrossing of any crossed sibling subtrees (e.g. if called after an external solver).
    for _pass in 0..4 {
        let mut swapped_any = false;
        for ei in 0..vis_edges.len() {
            let (p1, c1) = vis_edges[ei];
            for &(p2, c2) in &vis_edges[(ei + 1)..] {
                if p1 == p2 || p1 == c2 || c1 == p2 || c1 == c2 {
                    continue;
                }
                if !segments_intersect(pos[p1], pos[c1], pos[p2], pos[c2]) {
                    continue;
                }
                // Find lowest common ancestor `lca` and its two distinct children `a1`, `a2`
                // containing `c1` and `c2`.
                let mut lca_opt = None;
                for cand in (0..n).rev() {
                    if dirs[cand].visible && in_sub_flag[cand][c1] && in_sub_flag[cand][c2] {
                        lca_opt = Some(cand);
                        break;
                    }
                }
                if let Some(lca) = lca_opt {
                    let a1_opt = dirs[lca]
                        .children
                        .iter()
                        .copied()
                        .find(|&ch| ch < n && dirs[ch].visible && in_sub_flag[ch][c1]);
                    let a2_opt = dirs[lca]
                        .children
                        .iter()
                        .copied()
                        .find(|&ch| ch < n && dirs[ch].visible && in_sub_flag[ch][c2]);
                    if let (Some(a1), Some(a2)) = (a1_opt, a2_opt)
                        && a1 != a2
                    {
                        let pivot = pos[lca];
                        let ang1 = atan2_u16(pos[a1] - pivot);
                        let ang2 = atan2_u16(pos[a2] - pivot);
                        let delta_ang = ang2.wrapping_sub(ang1);
                        let neg_delta = 0u16.wrapping_sub(delta_ang);
                        for &w in &sub_vis[a1] {
                            pos[w] = pivot + rotate_by_u16(pos[w] - pivot, delta_ang);
                        }
                        for &w in &sub_vis[a2] {
                            pos[w] = pivot + rotate_by_u16(pos[w] - pivot, neg_delta);
                        }
                        swapped_any = true;
                        break;
                    }
                }
            }
            if swapped_any {
                break;
            }
        }
        if !swapped_any {
            break;
        }
    }

    // Helper: check if shifting `sub_vis[u]` by `delta` preserves all disc, edge-crossing,
    // and node-edge clearance invariants.
    let is_valid_shift = |u: usize, delta: IVec2, pos: &[IVec2]| -> bool {
        if delta == IVec2::ZERO {
            return false;
        }
        let sub = &sub_vis[u];
        let outs = &out_vis[u];
        let in_sub = &in_sub_flag[u];

        // 1. Disc non-overlap between `w in sub` and `z in outs`.
        for &w in sub {
            let pw = pos[w] + delta;
            let rw = disc_r[w];
            for &z in outs {
                let pz = pos[z];
                let min_d = rw + disc_r[z] + gap;
                if (pw.x - pz.x).abs() >= min_d || (pw.y - pz.y).abs() >= min_d {
                    continue;
                }
                let min_d64 = min_d as i64;
                if (pw - pz).len_sq() < min_d64 * min_d64 {
                    return false;
                }
            }
        }

        // 2. Edge non-crossing between edges with >=1 endpoint in `sub` and edges with <2 endpoints in `sub`.
        for &(p1, c1) in &vis_edges {
            let m1_p = in_sub[p1];
            let m1_c = in_sub[c1];
            if !m1_p && !m1_c {
                continue;
            }
            let a1 = if m1_p { pos[p1] + delta } else { pos[p1] };
            let a2 = if m1_c { pos[c1] + delta } else { pos[c1] };

            for &(p2, c2) in &vis_edges {
                let m2_p = in_sub[p2];
                let m2_c = in_sub[c2];
                if m2_p && m2_c {
                    continue;
                }
                if p1 == p2 || p1 == c2 || c1 == p2 || c1 == c2 {
                    continue;
                }
                let b1 = if m2_p { pos[p2] + delta } else { pos[p2] };
                let b2 = if m2_c { pos[c2] + delta } else { pos[c2] };
                if segments_intersect(a1, a2, b1, b2) {
                    return false;
                }
            }

            // 3. Node-to-edge clearance for moved edge `(a1, a2)` against outside nodes `z`.
            let seg_min_x = a1.x.min(a2.x);
            let seg_max_x = a1.x.max(a2.x);
            let seg_min_y = a1.y.min(a2.y);
            let seg_max_y = a1.y.max(a2.y);
            for &z in outs {
                if z == p1 || z == c1 {
                    continue;
                }
                let pz = pos[z];
                let min_r = (disc_r[z] / 2).max(3 * ONE) + edge_clear_pad;
                if pz.x < seg_min_x - min_r
                    || pz.x > seg_max_x + min_r
                    || pz.y < seg_min_y - min_r
                    || pz.y > seg_max_y + min_r
                {
                    continue;
                }
                let min_r64 = min_r as i64;
                if point_segment_dist_sq(pz, a1, a2) < min_r64 * min_r64 {
                    return false;
                }
            }
        }

        // 4. Node-to-edge clearance for moved nodes `w in sub` against unmoved edges `(p2, c2)`.
        for &(p2, c2) in &vis_edges {
            if in_sub[p2] || in_sub[c2] {
                continue;
            }
            let b1 = pos[p2];
            let b2 = pos[c2];
            let seg_min_x = b1.x.min(b2.x);
            let seg_max_x = b1.x.max(b2.x);
            let seg_min_y = b1.y.min(b2.y);
            let seg_max_y = b1.y.max(b2.y);
            for &w in sub {
                if w == p2 || w == c2 {
                    continue;
                }
                let pw = pos[w] + delta;
                let min_r = (disc_r[w] / 2).max(3 * ONE) + edge_clear_pad;
                if pw.x < seg_min_x - min_r
                    || pw.x > seg_max_x + min_r
                    || pw.y < seg_min_y - min_r
                    || pw.y > seg_max_y + min_r
                {
                    continue;
                }
                let min_r64 = min_r as i64;
                if point_segment_dist_sq(pw, b1, b2) < min_r64 * min_r64 {
                    return false;
                }
            }
        }

        true
    };

    // Helper: compute closed-form first-contact slide distance along unit vector `dir_u`
    // and return the largest valid displacement `delta`.
    let best_slide_along = |u: usize, dir_u: IVec2, max_t: Fx, pos: &[IVec2]| -> Option<IVec2> {
        if max_t < ONE || dir_u == IVec2::ZERO {
            return None;
        }
        let mut t_cap = max_t;
        let sub = &sub_vis[u];
        let outs = &out_vis[u];
        let ux = dir_u.x as i64;
        let uy = dir_u.y as i64;

        for &w in sub {
            let pw = pos[w];
            let rw = disc_r[w];
            for &z in outs {
                let pz = pos[z];
                let r_sum = (rw + disc_r[z] + gap) as i64;
                let dx = (pw.x - pz.x) as i64;
                let dy = (pw.y - pz.y) as i64;

                if (ux >= 0 && dx >= r_sum)
                    || (ux <= 0 && dx <= -r_sum)
                    || (uy >= 0 && dy >= r_sum)
                    || (uy <= 0 && dy <= -r_sum)
                {
                    continue;
                }
                let reach = t_cap as i64 + r_sum;
                if dx.abs() > reach || dy.abs() > reach {
                    continue;
                }
                let proj = div_round(dx * ux + dy * uy, UNIT as i64);
                if proj >= 0 {
                    continue;
                }
                let a = -proj;
                let d2 = dx * dx + dy * dy;
                let perp2 = (d2 - a * a).max(0);
                let r_pad = r_sum + ONE as i64;
                let r_pad2 = r_pad * r_pad;
                if perp2 < r_pad2 {
                    let h = crate::kernel::fixed::isqrt((r_pad2 - perp2) as u64) as i64 + 1;
                    let t_hit = (a - h).max(0) as Fx;
                    if t_hit < t_cap {
                        t_cap = t_hit;
                        if t_cap < ONE {
                            return None;
                        }
                    }
                }
            }
        }

        for num in [4i64, 3, 2, 1] {
            let t = (t_cap as i64 * num / 4) as Fx;
            if t < ONE {
                continue;
            }
            let delta = dir_u.unit_times(t as i64);
            if is_valid_shift(u, delta, pos) {
                return Some(delta);
            }
        }
        None
    };

    // Phase B: Multi-pass subtree compaction + obstacle rolling.
    // Order nodes by `(subtree_size, dist_to_parent_sq)` ascending so:
    // 1. Leaves compact onto their parent spokes first.
    // 2. Spokes compact onto spider hubs from inner rings to outer rings.
    // 3. Spider hubs and higher-level subtrees compact toward root once their spokes are tight.
    for _round in 0..3 {
        let mut order: Vec<usize> = vis_nodes.iter().copied().filter(|&u| u > 0).collect();
        order.sort_by_key(|&u| {
            let p = dirs[u].parent.unwrap_or(0);
            let d2 = (pos[u] - pos[p]).len_sq();
            (sub_vis[u].len(), d2)
        });

        let mut moved_in_round = false;
        for u in order {
            let Some(p) = dirs[u].parent else {
                continue;
            };
            let min_edge = disc_r[p] + disc_r[u] + gap;
            let v_to_p = pos[p] - pos[u];
            let d_to_p = v_to_p.length();

            // 1. Direct slide toward parent `p`.
            if d_to_p > min_edge + ONE
                && let Some(dir_p) = v_to_p.unit()
            {
                let max_t = d_to_p - min_edge;
                if let Some(delta) = best_slide_along(u, dir_p, max_t, &pos) {
                    for &w in &sub_vis[u] {
                        pos[w] += delta;
                    }
                    moved_in_round = true;
                }
            }

            // 2. Obstacle rolling: try +/- 28 deg around `dir_to_parent` if still far from `p`.
            let v_to_p2 = pos[p] - pos[u];
            let d_to_p2 = v_to_p2.length();
            if d_to_p2 > min_edge + 6 * ONE
                && let Some(dir_p) = v_to_p2.unit()
            {
                let cur_cost = d_to_p2 as i64 + (pos[u].length() as i64) / 4;
                let max_roll = ((d_to_p2 - min_edge) / 2).min(48 * ONE);
                for rot in [5097u16, 0u16.wrapping_sub(5097)] {
                    let roll_dir = rotate_by_u16(dir_p, rot);
                    if let Some(delta) = best_slide_along(u, roll_dir, max_roll, &pos) {
                        let new_u = pos[u] + delta;
                        let new_d = (pos[p] - new_u).length();
                        let new_cost = new_d as i64 + (new_u.length() as i64) / 4;
                        if new_d >= min_edge && new_cost + (2 * ONE as i64) < cur_cost {
                            for &w in &sub_vis[u] {
                                pos[w] += delta;
                            }
                            moved_in_round = true;
                            break;
                        }
                    }
                }
            }
        }
        if !moved_in_round {
            break;
        }
    }

    // Write compacted positions, updated outward angles, and true tightened subtree radii back to `out`.
    out[0].target_pos = pos[0];
    out[0].outward_angle = 0;
    for i in 1..n {
        if dirs[i].visible {
            out[i].target_pos = pos[i];
            if let Some(p) = dirs[i].parent {
                out[i].outward_angle = atan2_u16(pos[i] - pos[p]);
            }
        } else if let Some(p) = dirs[i].parent {
            out[i].target_pos = out[p].target_pos;
            out[i].outward_angle = out[p].outward_angle;
        }
    }

    for i in (0..n).rev() {
        if !dirs[i].visible {
            continue;
        }
        let pi = out[i].target_pos;
        let mut max_r = out[i].radii.parent_radius.max(min_dir_size);
        for &w in &sub_vis[i] {
            let d = (out[w].target_pos - pi).length() + disc_r[w] + 2 * ONE;
            if d > max_r {
                max_r = d;
            }
        }
        out[i].radii.radius = max_r;
    }
}

/// Signed 2D orientation `(b - a) x (c - a)` in `i64`.
#[inline]
fn orient2d(a: IVec2, b: IVec2, c: IVec2) -> i64 {
    let abx = b.x as i64 - a.x as i64;
    let aby = b.y as i64 - a.y as i64;
    let acx = c.x as i64 - a.x as i64;
    let acy = c.y as i64 - a.y as i64;
    abx * acy - aby * acx
}

/// Exact integer proper intersection test for open line segments `(a1, a2)` and `(b1, b2)`.
#[inline]
pub fn segments_intersect(a1: IVec2, a2: IVec2, b1: IVec2, b2: IVec2) -> bool {
    let a_min_x = a1.x.min(a2.x);
    let a_max_x = a1.x.max(a2.x);
    let b_min_x = b1.x.min(b2.x);
    let b_max_x = b1.x.max(b2.x);
    if a_max_x < b_min_x || b_max_x < a_min_x {
        return false;
    }
    let a_min_y = a1.y.min(a2.y);
    let a_max_y = a1.y.max(a2.y);
    let b_min_y = b1.y.min(b2.y);
    let b_max_y = b1.y.max(b2.y);
    if a_max_y < b_min_y || b_max_y < a_min_y {
        return false;
    }

    let o1 = orient2d(a1, a2, b1).signum();
    let o2 = orient2d(a1, a2, b2).signum();
    if o1 == 0 || o2 == 0 || o1 == o2 {
        return false;
    }
    let o3 = orient2d(b1, b2, a1).signum();
    let o4 = orient2d(b1, b2, a2).signum();
    o3 != 0 && o4 != 0 && o3 != o4
}

/// Tree edge `(parent_idx, child_idx)` for [`uncross_edges`].
#[derive(Clone, Copy, Debug)]
pub struct TreeEdge {
    pub parent: usize,
    pub child: usize,
}

/// Compute uncrossing velocity impulses (Q8 units/s) for directory nodes whose
/// tree edges intersect in 2D.
///
/// When edge `(p1 -> d1)` crosses `(p2 -> d2)`, pushes `d1` along
/// `unit(p1 - p2) + unit(d2 - d1)` and `d2` in the opposite direction so the
/// crossed child endpoints swap sides to match their parents.
pub fn uncross_edges(positions: &[IVec2], edges: &[TreeEdge], weight: Fx) -> Vec<IVec2> {
    let mut accels = vec![Acc::default(); positions.len()];
    if edges.len() < 2 || weight <= 0 {
        return vec![IVec2::ZERO; positions.len()];
    }

    for i in 0..edges.len() {
        let e1 = edges[i];
        let p1 = positions[e1.parent];
        let d1 = positions[e1.child];
        if p1 == d1 {
            continue;
        }
        for e2 in &edges[(i + 1)..] {
            // Edges sharing an endpoint cannot properly cross.
            if e1.parent == e2.parent
                || e1.parent == e2.child
                || e1.child == e2.parent
                || e1.child == e2.child
            {
                continue;
            }
            let p2 = positions[e2.parent];
            let d2 = positions[e2.child];
            if p2 == d2 || !segments_intersect(p1, d1, p2, d2) {
                continue;
            }

            let u_p = (p1 - p2).unit().unwrap_or(IVec2::ZERO);
            let u_d = (d2 - d1).unit().unwrap_or(IVec2::ZERO);
            let dir = (u_p + u_d).unit().unwrap_or_else(|| {
                let edge_v = d1 - p1;
                IVec2::new(-edge_v.y, edge_v.x)
                    .unit()
                    .unwrap_or(IVec2::ZERO)
            });
            let push = dir.unit_times(weight as i64);
            accels[e1.child].add(push);
            accels[e2.child].sub(push);
        }
    }

    accels.into_iter().map(Acc::get).collect()
}

/// Advance a position `pos` and velocity `vel` (Q8 units/s) toward `target` over
/// one tick (1/60s) with critically-damped spring attraction + external impulse,
/// snapping to exact `target` with `vel = 0` inside a 1-subpixel deadband.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn step_settled_pos(
    pos: IVec2,
    vel: IVec2,
    target: IVec2,
    ext_accel: IVec2,
    stiffness: i64,
    damping_num: i64,
    damping_den: i64,
    max_speed: Fx,
) -> (IVec2, IVec2) {
    let delta = target - pos;
    if delta.x.abs() <= 2
        && delta.y.abs() <= 2
        && vel.x.abs() <= ONE / 4
        && vel.y.abs() <= ONE / 4
        && ext_accel == IVec2::ZERO
    {
        return (target, IVec2::ZERO);
    }

    // Spring pull toward target + external impulse, damped per tick.
    let spring_vel = delta * (stiffness as i32);
    let combined = (vel + spring_vel + ext_accel).scale(damping_num, damping_den);
    let clamped = combined.clamp_len(max_speed);
    let next_pos = pos.step(clamped);

    let rem = target - next_pos;
    if rem.x.abs() <= 2 && rem.y.abs() <= 2 && ext_accel == IVec2::ZERO {
        (target, IVec2::ZERO)
    } else {
        (next_pos, clamped)
    }
}

/// Local-space file particle state for [`step_hybrid_dir_files`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalFileParticle {
    pub pos: IVec2,
    pub vel: IVec2,
    pub target_pos: IVec2,
    pub radius: Fx,
    pub tag: u16,
}

/// Step a single directory's files in [`gource_settings::LayoutEngine::Hybrid`] mode:
/// pulls each file toward its non-overlapping settled slot `target_pos` while
/// resolving transient pairwise contacts during growth/edits, and snaps to rest
/// once settled. Returns `true` if all files in the directory are now at rest.
pub fn step_hybrid_dir_files(files: &mut [LocalFileParticle]) -> bool {
    let n = files.len();
    if n == 0 {
        return true;
    }

    let all_settled = files.iter().all(|f| {
        let d = f.target_pos - f.pos;
        d.x.abs() <= 2 && d.y.abs() <= 2 && f.vel.x.abs() <= ONE / 4 && f.vel.y.abs() <= ONE / 4
    });
    if all_settled {
        for f in files.iter_mut() {
            f.pos = f.target_pos;
            f.vel = IVec2::ZERO;
        }
        return true;
    }

    let mut forces = vec![Acc::default(); n];
    for (i, f) in files.iter().enumerate() {
        forces[i].add((f.target_pos - f.pos) * 6);
    }

    // Transient pairwise contact separation so growing/edited files swirl smoothly around neighbors.
    if n > 1 && n <= 128 {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_unstable_by_key(|&idx| files[idx].pos.x);
        let max_r = files.iter().map(|f| f.radius).max().unwrap_or(4 * ONE);
        let reach = 2 * max_r + 2 * ONE;

        for a in 0..n {
            let i = order[a];
            let fi = files[i];
            let x_limit = fi.pos.x.saturating_add(reach);
            for &j in &order[(a + 1)..] {
                let fj = files[j];
                if fj.pos.x > x_limit {
                    break;
                }
                let min_d = fi.radius.saturating_add(fj.radius);
                if (fi.pos.y - fj.pos.y).abs() >= min_d {
                    continue;
                }
                let d = fj.pos - fi.pos;
                let d2 = d.len_sq();
                let min_d64 = min_d as i64;
                if d2 < min_d64 * min_d64 {
                    let push = if d2 > 0 {
                        let dist = crate::kernel::fixed::isqrt(d2 as u64) as i64;
                        let overlap = min_d64 - dist;
                        d.scale(overlap * 6, dist.max(1))
                    } else {
                        let ang = GOLDEN_ANGLE.wrapping_mul(fi.tag.wrapping_add(fj.tag).max(1));
                        direction(ang).unit_times(min_d64 * 2)
                    };
                    forces[i].sub(push);
                    forces[j].add(push);
                }
            }
        }
    }

    // Max speed 150 * ONE (2.5 px/tick at 60 Hz) guarantees smooth swirl (< 5.0 px/tick).
    let max_speed = 150 * ONE;
    let mut now_settled = true;
    for (i, f) in files.iter_mut().enumerate() {
        let next_vel = (f.vel + forces[i].get()).scale(5, 8).clamp_len(max_speed);
        let next_pos = f.pos.step(next_vel);
        let rem = f.target_pos - next_pos;
        if rem.x.abs() <= 2 && rem.y.abs() <= 2 && next_vel.length() <= ONE {
            f.pos = f.target_pos;
            f.vel = IVec2::ZERO;
        } else {
            f.pos = next_pos;
            f.vel = next_vel;
            now_settled = false;
        }
    }

    now_settled
}

/// World-space file particle state for [`step_world_files`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldFileParticle {
    /// Directory index owning this file.
    pub dir_idx: usize,
    /// Current world-space position (Q8).
    pub pos: IVec2,
    /// Current world-space velocity (Q8 units/s).
    pub vel: IVec2,
    /// Settled world-space target position (Q8).
    pub target_pos: IVec2,
    /// Current animated diameter (Q8).
    pub size: Fx,
    /// Target diameter (Q8).
    pub target_size: Fx,
    /// Collision radius (Q8).
    pub radius: Fx,
    /// Stable entity tag for deterministic coincidence tie-breaks.
    pub tag: u16,
}

/// Advance file particles in world space for one tick:
///
/// 1. Smoothly animates `size` toward `target_size`.
/// 2. Pulls each file toward `target_pos` (in `Hybrid` mode) or toward its
///    directory's live centroid (in `Particles` mode when `use_live_centroids` is true).
/// 3. Applies x-sorted sweep-and-prune pairwise separation:
///    - Same-directory files separate at `r_i + r_j + same_dir_gap`.
///    - Different-directory files separate at `r_i + r_j + cross_dir_gap` with
///      stronger repulsion so distinct directories push apart.
/// 4. Applies velocity damping and snaps settled files to rest.
pub fn step_world_files(
    files: &mut [WorldFileParticle],
    num_dirs: usize,
    use_live_centroids: bool,
    same_dir_gap: Fx,
    cross_dir_gap: Fx,
) -> Vec<IVec2> {
    let n = files.len();
    if n == 0 {
        return vec![IVec2::ZERO; num_dirs];
    }

    // 1. Animate file sizes and radii.
    let mut max_file_r: Fx = 4 * ONE;
    for f in files.iter_mut() {
        let ds = f.target_size - f.size;
        if ds.abs() <= 2 {
            f.size = f.target_size;
        } else {
            let step = div_round(ds as i64 * 8, TICK_HZ as i64) as Fx;
            f.size += if step != 0 { step } else { ds.signum() };
        }
        f.radius = (f.size / 2).max(2 * ONE);
        max_file_r = max_file_r.max(f.radius);
    }

    // 2. Compute live directory centroids (and cross-directory reaction forces per dir).
    let mut dir_sum = vec![Acc::default(); num_dirs];
    let mut dir_count = vec![0u32; num_dirs];
    let mut dir_push = vec![Acc::default(); num_dirs];

    if use_live_centroids {
        for f in files.iter() {
            if f.dir_idx < num_dirs {
                dir_sum[f.dir_idx].add(f.pos);
                dir_count[f.dir_idx] += 1;
            }
        }
    }

    let mut forces = vec![Acc::default(); n];
    for (idx, f) in files.iter().enumerate() {
        let attract_target =
            if use_live_centroids && f.dir_idx < num_dirs && dir_count[f.dir_idx] > 1 {
                let c = dir_sum[f.dir_idx]
                    .get()
                    .scale(1, dir_count[f.dir_idx] as i64);
                // Blend 65% live centroid + 35% settled slot so clusters stay cohesive yet uncrossed.
                c.scale(13, 20) + f.target_pos.scale(7, 20)
            } else {
                f.target_pos
            };
        let to_target = attract_target - f.pos;
        let pull_k = if use_live_centroids { 5 } else { 8 };
        forces[idx].add(to_target * pull_k);
    }

    // 3. Sweep-and-prune along X for same-dir contact packing and cross-dir repulsion.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_unstable_by_key(|&idx| files[idx].pos.x);

    let max_reach = (2 * max_file_r).saturating_add(cross_dir_gap.max(same_dir_gap));
    const MAX_NEIGHBORS_PER_FILE: usize = 48;

    for pos_a in 0..n {
        let i = order[pos_a];
        let fi = files[i];
        let x_limit = fi.pos.x.saturating_add(max_reach);
        let mut checks = 0usize;

        for &j in &order[(pos_a + 1)..] {
            let fj = files[j];
            if fj.pos.x > x_limit {
                break;
            }
            let same_dir = fi.dir_idx == fj.dir_idx;
            let gap = if same_dir {
                same_dir_gap
            } else {
                cross_dir_gap
            };
            let min_d = fi.radius.saturating_add(fj.radius).saturating_add(gap);
            if (fi.pos.y - fj.pos.y).abs() >= min_d {
                continue;
            }
            checks += 1;
            if checks > MAX_NEIGHBORS_PER_FILE {
                break;
            }

            let d = fj.pos - fi.pos;
            let d2 = d.len_sq();
            let min_d64 = min_d as i64;
            if d2 < min_d64 * min_d64 {
                let push = if d2 > 0 {
                    let dist = crate::kernel::fixed::isqrt(d2 as u64) as i64;
                    let overlap = min_d64 - dist;
                    let stiffness = if same_dir { 10 } else { 18 };
                    d.scale(overlap * stiffness, dist.max(1))
                } else {
                    let ang = GOLDEN_ANGLE.wrapping_mul(fi.tag.wrapping_add(fj.tag).max(1));
                    direction(ang).unit_times(min_d64 * 4)
                };
                forces[i].sub(push);
                forces[j].add(push);
                if !same_dir {
                    if fi.dir_idx < num_dirs {
                        dir_push[fi.dir_idx].sub(push.scale(1, 2));
                    }
                    if fj.dir_idx < num_dirs {
                        dir_push[fj.dir_idx].add(push.scale(1, 2));
                    }
                }
            }
        }
    }

    // 4. Integrate velocities with damping and deadband settling.
    let max_speed = 900 * ONE;
    for (i, f) in files.iter_mut().enumerate() {
        let acc = forces[i].get();
        let next_vel = (f.vel + acc).scale(3, 4).clamp_len(max_speed);
        let next_pos = f.pos.step(next_vel);
        let err = f.target_pos - next_pos;
        if !use_live_centroids
            && err.x.abs() <= 2
            && err.y.abs() <= 2
            && next_vel.length() <= ONE / 2
        {
            f.pos = f.target_pos;
            f.vel = IVec2::ZERO;
        } else if next_vel.x.abs() <= 4 && next_vel.y.abs() <= 4 && acc.length() <= ONE / 2 {
            f.vel = IVec2::ZERO;
        } else {
            f.pos = next_pos;
            f.vel = next_vel;
        }
    }

    dir_push.into_iter().map(Acc::get).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_trig_landmarks() {
        assert_eq!(asin_u16(0, 100), 0);
        assert_eq!(asin_u16(100, 100), 16384);
        assert_eq!(asin_u16(150, 100), 16384);
        // sin(30 deg) = 0.5 -> 30 / 360 * 65536 = 5461
        let a30 = asin_u16(1, 2);
        assert!((a30 as i32 - 5461).abs() <= 32, "a30={a30}");

        assert_eq!(atan2_u16(IVec2::new(10 * ONE, 0)), 0);
        assert!((atan2_u16(IVec2::new(0, 10 * ONE)) as i32 - 16384).abs() <= 32);
        assert!((atan2_u16(IVec2::new(-10 * ONE, 0)) as i32 - 32768).abs() <= 32);
        assert!((atan2_u16(IVec2::new(0, -10 * ONE)) as i32 - 49152).abs() <= 32);
    }

    #[test]
    fn segment_intersection_and_uncrossing() {
        let p1 = IVec2::new(0, 20 * ONE);
        let d1 = IVec2::new(20 * ONE, -20 * ONE);
        let p2 = IVec2::new(0, -20 * ONE);
        let d2 = IVec2::new(20 * ONE, 20 * ONE);
        assert!(segments_intersect(p1, d1, p2, d2));

        let mut pos = vec![p1, d1, p2, d2];
        let edges = vec![
            TreeEdge {
                parent: 0,
                child: 1,
            },
            TreeEdge {
                parent: 2,
                child: 3,
            },
        ];
        let impulses = uncross_edges(&pos, &edges, 100 * ONE);
        assert!(impulses[1].y > 0, "d1 should be pushed up toward p1's side");
        assert!(
            impulses[3].y < 0,
            "d2 should be pushed down toward p2's side"
        );

        // Apply impulse and verify they uncross:
        pos[1] += impulses[1];
        pos[3] += impulses[3];
        assert!(!segments_intersect(pos[0], pos[1], pos[2], pos[3]));
    }

    #[test]
    fn spider_directory_tree_has_zero_overlaps_and_zero_crossings() {
        // Build a spider tree: root(0) -> crates(1) -> {wasm-ops(2) with 56 children, x86-forms(3) with 42 children},
        // where each child also has a `src` grandchild!
        let mut dirs = vec![
            TreeDirIn {
                parent: None,
                children: vec![1],
                own_file_radius: 0,
                visible_files: 0,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: vec![2, 3],
                own_file_radius: 0,
                visible_files: 0,
                visible: true,
            },
            TreeDirIn {
                parent: Some(1),
                children: Vec::new(),
                own_file_radius: 0,
                visible_files: 0,
                visible: true,
            },
            TreeDirIn {
                parent: Some(1),
                children: Vec::new(),
                own_file_radius: 0,
                visible_files: 0,
                visible: true,
            },
        ];

        let mut edges = vec![
            TreeEdge {
                parent: 0,
                child: 1,
            },
            TreeEdge {
                parent: 1,
                child: 2,
            },
            TreeEdge {
                parent: 1,
                child: 3,
            },
        ];

        for (spider_parent, count) in [(2usize, 56usize), (3usize, 42usize)] {
            for _ in 0..count {
                let op_idx = dirs.len();
                let src_idx = op_idx + 1;
                dirs[spider_parent].children.push(op_idx);
                dirs.push(TreeDirIn {
                    parent: Some(spider_parent),
                    children: vec![src_idx],
                    own_file_radius: 5 * ONE,
                    visible_files: 1,
                    visible: true,
                });
                dirs.push(TreeDirIn {
                    parent: Some(op_idx),
                    children: Vec::new(),
                    own_file_radius: 6 * ONE,
                    visible_files: 2,
                    visible: true,
                });
                edges.push(TreeEdge {
                    parent: spider_parent,
                    child: op_idx,
                });
                edges.push(TreeEdge {
                    parent: op_idx,
                    child: src_idx,
                });
            }
        }

        let initial = solve_settled_tree_initial(&dirs, 384, 15 * ONE);
        let solved = solve_settled_tree(&dirs, 384, 15 * ONE);
        assert_eq!(solved.len(), dirs.len());
        assert!(
            solved[0].radii.radius < initial[0].radii.radius,
            "optimizer must compact total layout radius: solved={} vs initial={}",
            solved[0].radii.radius,
            initial[0].radii.radius
        );

        // Check that no two visible directories overlap their own file-cluster discs!
        for i in 0..dirs.len() {
            for j in (i + 1)..dirs.len() {
                let dist = (solved[i].target_pos - solved[j].target_pos).length();
                let min_sep = solved[i].radii.parent_radius + solved[j].radii.parent_radius;
                assert!(
                    dist >= min_sep,
                    "dirs {i} and {j} overlap: dist={dist} < min_sep={min_sep}"
                );
            }
        }

        // Check that zero tree edges cross!
        for i in 0..edges.len() {
            let e1 = edges[i];
            for e2 in &edges[(i + 1)..] {
                if e1.parent == e2.parent
                    || e1.parent == e2.child
                    || e1.child == e2.parent
                    || e1.child == e2.child
                {
                    continue;
                }
                assert!(
                    !segments_intersect(
                        solved[e1.parent].target_pos,
                        solved[e1.child].target_pos,
                        solved[e2.parent].target_pos,
                        solved[e2.child].target_pos
                    ),
                    "edges ({}->{}) and ({}->{}) cross!",
                    e1.parent,
                    e1.child,
                    e2.parent,
                    e2.child
                );
            }
        }
    }

    #[test]
    fn optimizer_uncrosses_crossed_subtrees_and_compacts() {
        let dirs = vec![
            TreeDirIn {
                parent: None,
                children: vec![1, 2],
                own_file_radius: 4 * ONE,
                visible_files: 1,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: vec![3],
                own_file_radius: 5 * ONE,
                visible_files: 1,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: vec![4],
                own_file_radius: 5 * ONE,
                visible_files: 1,
                visible: true,
            },
            TreeDirIn {
                parent: Some(1),
                children: Vec::new(),
                own_file_radius: 5 * ONE,
                visible_files: 1,
                visible: true,
            },
            TreeDirIn {
                parent: Some(2),
                children: Vec::new(),
                own_file_radius: 5 * ONE,
                visible_files: 1,
                visible: true,
            },
        ];

        let mut out = solve_settled_tree_initial(&dirs, 384, 15 * ONE);
        // Deliberately cross edges (1->3) and (2->4) and inflate distances:
        out[1].target_pos = IVec2::new(40 * ONE, 30 * ONE);
        out[2].target_pos = IVec2::new(40 * ONE, -30 * ONE);
        out[3].target_pos = IVec2::new(120 * ONE, -50 * ONE);
        out[4].target_pos = IVec2::new(120 * ONE, 50 * ONE);
        assert!(segments_intersect(
            out[1].target_pos,
            out[3].target_pos,
            out[2].target_pos,
            out[4].target_pos
        ));

        optimize_settled_layout(&dirs, &mut out, 384, 15 * ONE);
        assert!(!segments_intersect(
            out[1].target_pos,
            out[3].target_pos,
            out[2].target_pos,
            out[4].target_pos
        ));
    }
}
