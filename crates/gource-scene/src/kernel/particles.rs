//! Fully physics-driven per-file tree layout (`--layout-engine particles`).
//!
//! There are no precomputed targets and no computed arm lengths: every visible
//! file is a particle in world space, and a directory's tip is *derived* from
//! its files every tick:
//!
//! - A directory with visible files sits exactly on the centroid of those files.
//! - A directory without files but with several child branches (e.g. `crates/`)
//!   sits on the centroid of its children's anchors.
//! - A directory without files and a single child branch sits halfway between
//!   its parent's tip and its child's anchor.
//!
//! Forces (all integer, Q24.8, applied as per-tick velocity increments):
//!
//! 1. **Cohesion**: files are pulled towards their own directory's centroid,
//!    but only once they are outside the directory's ideal packed disc, so a
//!    cluster is held together without being crushed.
//! 2. **Collision**: every pair of overlapping files is pushed apart.
//! 3. **Cross-directory repulsion**: files of different directories repel
//!    within `repel_gap` of contact.
//! 4. **Downstream forces**, applied uniformly to *every* file in a subtree so
//!    the subtree translates rigidly:
//!    - an edge pull towards the parent's tip, with no rest length: how far out
//!      a branch sits is decided entirely by the file/file repulsion and
//!      collisions resisting the pull;
//!    - a sideways straightening push turning each branch to continue its
//!      parent's incoming direction (purely tangential, it never lengthens arms);
//!    - an edge-crossing push separating the subtrees below two crossed edges;
//!    - a weak gravity pulling the root towards the origin.

use crate::kernel::fixed::{Acc, Fx, IVec2, ONE, TICK_HZ, UNIT, div_round, isqrt, sat};
use crate::kernel::layout::{WorldFileParticle, segments_intersect};
use crate::kernel::trig::{GOLDEN_ANGLE, direction};

/// Directory topology for [`step_tree_particles`], in tree preorder (every
/// parent index precedes its children).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticleDir {
    pub parent: Option<usize>,
    /// True if the directory or any descendant has visible files.
    pub visible: bool,
}

/// The bend of an arm away from the straight line between its directory
/// tips: the arm is a quadratic Bezier whose control point is the chord
/// midpoint plus this offset (horizontal, and vertical for arms rolling over
/// other arms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArmBend {
    pub xy: IVec2,
    pub z: Fx,
}

/// Tunables for [`step_tree_particles`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleParams {
    /// Contact gap between files of the same directory (Q8).
    pub same_gap: Fx,
    /// Range beyond contact over which files of different directories repel (Q8).
    pub repel_gap: Fx,
    /// Collision stiffness (per Q8 of overlap).
    pub collide_k: i64,
    /// Cross-directory repulsion stiffness (per Q8 of penetration into `repel_gap`).
    pub repel_k: i64,
    /// Cohesion stiffness (per Q8 outside the packed disc).
    pub cohesion_k: i64,
    /// Edge pull stiffness (per Q8 of child-to-parent distance).
    pub edge_k: i64,
    /// Maximum edge pull per tick (Q8 units/s) on a single-file subtree; a
    /// subtree of `n` files feels `n^(-1/4)` of it.
    pub edge_max: Fx,
    /// Sideways straightening push per tick (Q8 units/s).
    pub straighten_push: Fx,
    /// Maximum velocity increment applied downstream of each crossed arm, per tick.
    pub crossing_push: Fx,
    /// Minimum crossing push per tick (Q8 units/s).
    pub crossing_min: Fx,
    /// Crossing push growth per Q8 of penetration past the other arm.
    pub crossing_k: i64,
    /// Clearance files keep from other directories' arms (Q8).
    pub arm_clear: Fx,
    /// Arm/file repulsion stiffness (per Q8 inside `arm_clear`).
    pub arm_repel_k: i64,
    /// Range over which arms of unrelated directories repel each other (Q8).
    pub arm_range: Fx,
    /// Arm/arm repulsion stiffness (per Q8 inside `arm_range`).
    pub arm_arm_k: i64,
    /// Rate at which arm bends relax straight (per second).
    pub arm_straighten_k: i64,
    /// Static friction: a resting file stays put unless the net force on it
    /// exceeds this (Q8 units/s per tick). Arms likewise only bend past it.
    pub static_friction: Fx,
    /// Sliding friction: a moving file loses this much speed per tick,
    /// opposite to its motion (Q8 units/s per tick).
    pub kinetic_friction: Fx,
    /// Viscous damping: velocity is multiplied by `damping.0 / damping.1`
    /// per tick (`(1, 1)` disables it).
    pub damping: (i64, i64),
    /// Maximum particle speed (Q8 units/s).
    pub max_speed: Fx,
    /// Downward acceleration per tick (Q8 units/s). The floor (`z = radius`)
    /// holds files up until they are deleted.
    pub gravity: Fx,
    /// Lowest height deleted files fall to (Q8, negative).
    pub bottom: Fx,
    /// Half-thickness of an arm (Q8): arms rest on the floor at this height.
    pub arm_radius: Fx,
}

impl ParticleParams {
    /// Defaults scaled to a file diameter (Q8).
    pub fn for_file_diameter(file_diameter: Fx) -> Self {
        let d = file_diameter.max(2 * ONE);
        Self {
            same_gap: ONE / 2,
            repel_gap: 4 * d,
            collide_k: 24,
            repel_k: 8,
            cohesion_k: 4,
            edge_k: 3,
            edge_max: 40 * ONE,
            straighten_push: 100 * ONE,
            crossing_push: 150 * ONE,
            crossing_min: 20 * ONE,
            crossing_k: 8,
            arm_clear: d,
            arm_repel_k: 8,
            arm_range: 2 * d,
            arm_arm_k: 6,
            arm_straighten_k: 1,
            static_friction: 8 * ONE,
            kinetic_friction: 6 * ONE,
            damping: (7, 8),
            max_speed: 400 * ONE,
            gravity: 16 * ONE,
            bottom: -10 * d,
            arm_radius: ONE,
        }
    }
}

impl Default for ParticleParams {
    fn default() -> Self {
        Self::for_file_diameter(8 * ONE)
    }
}

/// Per-directory geometry derived from file positions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirFrame {
    /// Directory tip (Q8, world space).
    pub pos: Vec<IVec2>,
    /// Height of the directory tip (Q8), derived like `pos`.
    pub pos_z: Vec<Fx>,
    /// Centroid of the directory's own visible files (Q8; `ZERO` if none).
    pub centroid: Vec<IVec2>,
    /// Number of the directory's own visible files.
    pub count: Vec<u32>,
    /// Max distance from the centroid to the far edge of an own file (Q8).
    pub extent: Vec<Fx>,
    /// Radius of the ideal packed disc of the own files (Q8).
    pub pack_radius: Vec<Fx>,
    /// Centroid of every visible file in the directory's subtree (Q8).
    pub sub_centroid: Vec<IVec2>,
    /// Number of visible files in the directory's subtree.
    pub sub_count: Vec<u32>,
    /// Radius around `sub_centroid` bounding the subtree's files (Q8, bottom-up
    /// bounding-sphere approximation).
    pub sub_radius: Vec<Fx>,
    /// Number of crossed edge pairs seen by the last [`step_tree_particles`].
    pub crossings: usize,
}

/// Derive directory tips, centroids and cluster radii from file positions.
pub fn dir_frame(dirs: &[ParticleDir], files: &[WorldFileParticle], same_gap: Fx) -> DirFrame {
    let n = dirs.len();
    let mut sum = vec![Acc::default(); n];
    let mut sum_z = vec![0i64; n];
    let mut count = vec![0u32; n];
    let mut r2 = vec![0i64; n];
    for f in files {
        if f.dir_idx < n {
            sum[f.dir_idx].add(f.pos);
            sum_z[f.dir_idx] += f.z as i64;
            count[f.dir_idx] += 1;
            let re = f.radius.max(0) as i64 + (same_gap / 2) as i64;
            r2[f.dir_idx] += re * re;
        }
    }

    let mut centroid = vec![IVec2::ZERO; n];
    let mut pack_radius = vec![0 as Fx; n];
    for d in 0..n {
        if count[d] > 0 {
            let c = count[d] as i64;
            centroid[d] = IVec2::new(sat(div_round(sum[d].x, c)), sat(div_round(sum[d].y, c)));
            // Disc packing density ~2/3 (small clusters pack loosely):
            // pi R^2 * 2/3 = sum(pi r^2).
            pack_radius[d] = isqrt((r2[d] * 3 / 2) as u64).min(i32::MAX as u64) as Fx;
        }
    }

    let mut extent = vec![0 as Fx; n];
    for f in files {
        if f.dir_idx < n {
            let e = (f.pos - centroid[f.dir_idx])
                .length()
                .saturating_add(f.radius);
            extent[f.dir_idx] = extent[f.dir_idx].max(e);
        }
    }

    // Bottom-up anchors: own centroid, else the mean of the children's anchors.
    let mut anchor = vec![IVec2::ZERO; n];
    let mut anchor_z = vec![0 as Fx; n];
    let mut has_anchor = vec![false; n];
    let mut child_sum = vec![Acc::default(); n];
    let mut child_sum_z = vec![0i64; n];
    let mut child_cnt = vec![0i64; n];
    for d in (0..n).rev() {
        if !dirs[d].visible {
            continue;
        }
        if count[d] > 0 {
            anchor[d] = centroid[d];
            anchor_z[d] = sat(div_round(sum_z[d], count[d] as i64));
            has_anchor[d] = true;
        } else if child_cnt[d] > 0 {
            let c = child_cnt[d];
            anchor[d] = IVec2::new(
                sat(div_round(child_sum[d].x, c)),
                sat(div_round(child_sum[d].y, c)),
            );
            anchor_z[d] = sat(div_round(child_sum_z[d], c));
            has_anchor[d] = true;
        }
        if has_anchor[d]
            && let Some(p) = dirs[d].parent
            && p < d
        {
            child_sum[p].add(anchor[d]);
            child_sum_z[p] += anchor_z[d] as i64;
            child_cnt[p] += 1;
        }
    }

    // Subtree centroids and bounding radii (bottom-up).
    let mut ssum = sum.clone();
    let mut scount = count.clone();
    for d in (0..n).rev() {
        if let Some(p) = dirs[d].parent.filter(|&p| p < d) {
            let (sx, sy, sc) = (ssum[d].x, ssum[d].y, scount[d]);
            ssum[p].x += sx;
            ssum[p].y += sy;
            scount[p] += sc;
        }
    }
    let mut sub_centroid = vec![IVec2::ZERO; n];
    for d in 0..n {
        if scount[d] > 0 {
            let c = scount[d] as i64;
            sub_centroid[d] =
                IVec2::new(sat(div_round(ssum[d].x, c)), sat(div_round(ssum[d].y, c)));
        }
    }
    let mut sub_radius = vec![0 as Fx; n];
    for d in (0..n).rev() {
        if scount[d] == 0 {
            continue;
        }
        if count[d] > 0 {
            let own = (centroid[d] - sub_centroid[d])
                .length()
                .saturating_add(extent[d]);
            sub_radius[d] = sub_radius[d].max(own);
        }
        if let Some(p) = dirs[d].parent.filter(|&p| p < d) {
            let r = (sub_centroid[d] - sub_centroid[p])
                .length()
                .saturating_add(sub_radius[d]);
            sub_radius[p] = sub_radius[p].max(r);
        }
    }

    // Top-down tips.
    let mut pos = vec![IVec2::ZERO; n];
    let mut pos_z = vec![0 as Fx; n];
    for d in 0..n {
        (pos[d], pos_z[d]) = match dirs[d].parent.filter(|&p| p < d) {
            None => (anchor[d], anchor_z[d]),
            Some(p) => {
                if !dirs[d].visible || !has_anchor[d] {
                    (pos[p], pos_z[p])
                } else if count[d] > 0 || child_cnt[d] >= 2 {
                    // On its files, or (a branching empty directory) among
                    // its children.
                    (anchor[d], anchor_z[d])
                } else {
                    let (pp, pz) = (pos[p], pos_z[p]);
                    (
                        pp + (anchor[d] - pp).scale(1, 2),
                        pz + (anchor_z[d] - pz) / 2,
                    )
                }
            }
        };
    }

    DirFrame {
        pos,
        pos_z,
        centroid,
        count,
        extent,
        pack_radius,
        sub_centroid,
        sub_count: scount,
        sub_radius,
        crossings: 0,
    }
}

/// A push perpendicular to line `(l1, l2)` towards the side `toward` is on,
/// growing with how far `wrong` has penetrated to the other side:
/// `min + k * depth`, capped at `max`. Proportional to the penetration, it
/// fades out as the crossing resolves instead of overshooting and re-crossing.
fn side_push(l1: IVec2, l2: IVec2, toward: IVec2, wrong: IVec2, p: &ParticleParams) -> IVec2 {
    let e = l2 - l1;
    let mut n = IVec2::new(-e.y, e.x);
    if n.dot(toward - l1) < 0 {
        n = -n;
    }
    let Some(u) = n.unit() else {
        return IVec2::ZERO;
    };
    let depth = (-(wrong - l1).dot(u) / UNIT as i64).max(0);
    let mag = (p.crossing_min as i64 + p.crossing_k * depth).min(p.crossing_push as i64);
    u.unit_times(mag)
}

/// A point in 3D: horizontal position and height (Q8).
type P3 = (IVec2, Fx);

/// The point of 3D segment `(a, b)` closest to `q`, and its position along
/// the segment in 64ths.
fn closest_on_segment3(q: P3, a: P3, b: P3) -> (P3, i64) {
    let (abx, aby, abz) = (
        (b.0.x - a.0.x) as i64,
        (b.0.y - a.0.y) as i64,
        (b.1 - a.1) as i64,
    );
    let l2 = abx * abx + aby * aby + abz * abz;
    if l2 == 0 {
        return (a, 0);
    }
    let dot =
        (q.0.x - a.0.x) as i64 * abx + (q.0.y - a.0.y) as i64 * aby + (q.1 - a.1) as i64 * abz;
    let t = dot.clamp(0, l2);
    let pt = (
        a.0 + IVec2::new(sat(div_round(abx * t, l2)), sat(div_round(aby * t, l2))),
        a.1 + sat(div_round(abz * t, l2)),
    );
    (pt, div_round(t * 64, l2))
}

/// Squared 3D distance and offset `b - a`.
fn delta3(a: P3, b: P3) -> (i64, i64, i64, i64) {
    let (dx, dy, dz) = (
        (b.0.x - a.0.x) as i64,
        (b.0.y - a.0.y) as i64,
        (b.1 - a.1) as i64,
    );
    (dx * dx + dy * dy + dz * dz, dx, dy, dz)
}

/// Bend weights (in eighths) of the arm sample points at `t = 0, 1/4, 1/2, 3/4, 1`:
/// a quadratic Bezier with control point `chord midpoint + bend` passes through
/// `lerp(parent, child, t) + 2t(1-t) * bend`.
const ARM_W: [i64; 5] = [0, 3, 4, 3, 0];

/// The 5-point polyline approximating the arm from `pp` to `pd` bent by `bend`.
pub fn arm_polyline(pp: IVec2, pd: IVec2, bend: IVec2) -> [IVec2; 5] {
    let chord = pd - pp;
    let mut pts = [pp; 5];
    for (k, pt) in pts.iter_mut().enumerate() {
        *pt = pp + chord.scale(k as i64, 4) + bend.scale(ARM_W[k], 8);
    }
    pts
}

/// Heights of the 5 arm polyline points from tip heights `zp`, `zd` and bend height `bz`.
pub fn arm_heights(zp: Fx, zd: Fx, bz: Fx) -> [Fx; 5] {
    let mut zs = [zp; 5];
    for (k, z) in zs.iter_mut().enumerate() {
        *z = zp
            + sat(div_round((zd - zp) as i64 * k as i64, 4))
            + sat(div_round(bz as i64 * ARM_W[k], 8));
    }
    zs
}

/// The first pair of crossing segments `(i, j)` between two arm polylines.
fn polylines_cross(a: &[IVec2; 5], b: &[IVec2; 5]) -> Option<(usize, usize)> {
    for i in 0..4 {
        for j in 0..4 {
            if segments_intersect(a[i], a[i + 1], b[j], b[j + 1]) {
                return Some((i, j));
            }
        }
    }
    None
}

/// Bounding box `(min, max)` of a polyline.
fn bbox(pts: &[IVec2; 5]) -> (IVec2, IVec2) {
    let mut lo = pts[0];
    let mut hi = pts[0];
    for &q in &pts[1..] {
        lo = lo.min(q);
        hi = hi.max(q);
    }
    (lo, hi)
}

/// Advance all file particles and arm bends by one tick and return the
/// post-step [`DirFrame`].
///
/// The world is 3D: files are spheres resting on a floor (`z = radius`) under
/// gravity, arms are thin tubes resting on the floor, and nothing passes
/// through anything else; contacts push along the 3D normal, so bodies roll
/// over each other. A deleted (`falling`) file loses the floor's support and
/// drops away to `bottom`, and whatever rested on it falls with it.
///
/// `files[i].dir_idx` indexes `dirs`; `target_pos` is ignored. `bends[d]` is the
/// bend of the arm into directory `d` (see [`ArmBend`]); it is updated in place.
pub fn step_tree_particles(
    dirs: &[ParticleDir],
    files: &mut [WorldFileParticle],
    bends: &mut [ArmBend],
    p: &ParticleParams,
) -> DirFrame {
    let n = dirs.len();
    let nf = files.len();
    assert_eq!(bends.len(), n, "one bend per directory");

    // 1. Animate sizes.
    let mut max_r: Fx = 2 * ONE;
    for f in files.iter_mut() {
        let ds = f.target_size - f.size;
        if ds.abs() <= 2 {
            f.size = f.target_size;
        } else {
            let step = div_round(ds as i64 * 8, TICK_HZ as i64) as Fx;
            f.size += if step != 0 { step } else { ds.signum() };
        }
        f.radius = (f.size / 2).max(ONE / 2);
        max_r = max_r.max(f.radius);
    }
    if nf == 0 {
        return dir_frame(dirs, files, p.same_gap);
    }

    let frame = dir_frame(dirs, files, p.same_gap);

    // 2. Downstream (subtree) forces and the arms.
    let mut fanout = vec![0i64; n];
    for (d, dir) in dirs.iter().enumerate() {
        if dir.visible
            && let Some(par) = dir.parent.filter(|&q| q < d)
        {
            fanout[par] += 1;
        }
    }
    let mut sub = vec![Acc::default(); n];
    // (parent, child, polyline, heights)
    let mut arms: Vec<(usize, usize, [IVec2; 5], [Fx; 5])> = Vec::with_capacity(n);
    let mut is_arm = vec![false; n];
    for d in 0..n {
        if !dirs[d].visible {
            continue;
        }
        let Some(par) = dirs[d].parent.filter(|&q| q < d && dirs[q].visible) else {
            // Root: weak gravity towards the origin.
            sub[d].add(-frame.pos[d].scale(1, 2));
            continue;
        };
        let pp = frame.pos[par];
        let pd = frame.pos[d];
        // Pull per file falls off as the fourth root of the subtree's file
        // count: big branches are steadier, but still come home.
        let mass = (isqrt(isqrt(frame.sub_count[d].max(1) as u64 * 256)) as i64).max(4);

        // Edge pull towards the parent's tip (no rest length).
        let to_parent = pp - pd;
        if let Some(u) = to_parent.unit() {
            let pull = (to_parent.length() as i64 * p.edge_k).min(p.edge_max as i64) * 4 / mass;
            sub[d].add(u.unit_times(pull));
        }

        // Sideways straightening: turn the branch towards the parent's
        // incoming direction, using only the component perpendicular to the
        // branch so it never changes the arm length. Wide fan-outs get a weaker
        // bias so siblings can spread all round.
        if let Some(gp) = dirs[par].parent.filter(|&q| q < par && dirs[q].visible)
            && let Some(out) = (pp - frame.pos[gp]).unit()
            && let Some(e) = (pd - pp).unit()
        {
            let along = out.dot(e) / UNIT as i64;
            let tangent = out - e.scale(along, UNIT as i64);
            let k = fanout[par].max(4);
            let mag = p.straighten_push as i64 * 4 / k;
            sub[d].add(tangent.scale(mag, UNIT as i64));
        }

        if pp != pd {
            let chord = to_parent.length();
            bends[d].xy = bends[d].xy.clamp_len(chord);
            // Arms rest on the floor: the bend may sag down to it, or arch up.
            let (zp, zd) = (frame.pos_z[par], frame.pos_z[d]);
            let sag_floor = 2 * (p.arm_radius as i64 - (zp as i64 + zd as i64) / 2);
            // (Tips raised above the floor, e.g. by files dropping in, can sag
            // further than the arm is long; the floor limit wins.)
            let arch_max = (chord as i64).max(sag_floor);
            bends[d].z = (bends[d].z as i64).clamp(sag_floor, arch_max) as Fx;
            arms.push((
                par,
                d,
                arm_polyline(pp, pd, bends[d].xy),
                arm_heights(zp, zd, bends[d].z),
            ));
            is_arm[d] = true;
        }
    }
    for (b, &arm) in bends.iter_mut().zip(&is_arm) {
        if !arm {
            *b = ArmBend::default();
        }
    }
    let shares_dir = |a: (usize, usize), b: (usize, usize)| -> bool {
        a.0 == b.0 || a.0 == b.1 || a.1 == b.0 || a.1 == b.1
    };
    let mut bend_force = vec![Acc::default(); n];
    let mut bend_force_z = vec![0i64; n];
    let cap = p.max_speed as i64;

    // Arm crossings, seen from above. Arms that cross at clearly different
    // heights are legitimately passing over each other. Arms crossing at the
    // same height roll over: the higher-index child's arm is lifted over the
    // other, and both subtrees are also pushed apart sideways, so the arch
    // can come back down once they no longer cross. The crossed child lies on
    // the far side of the other arm from its own parent: it is pushed back
    // across, the other crossed child the opposite way.
    let mut crossings = 0usize;
    let mut arm_order: Vec<usize> = (0..arms.len()).collect();
    let boxes: Vec<(IVec2, IVec2)> = arms.iter().map(|a| bbox(&a.2)).collect();
    arm_order.sort_unstable_by_key(|&i| (boxes[i].0.x, arms[i].1));
    for x in 0..arm_order.len() {
        let i = arm_order[x];
        let (p1, c1, ref pa, ref za) = arms[i];
        let (lo_a, hi_a) = boxes[i];
        for &j in &arm_order[(x + 1)..] {
            let (lo_b, hi_b) = boxes[j];
            if lo_b.x > hi_a.x {
                break;
            }
            if lo_b.y > hi_a.y || hi_b.y < lo_a.y {
                continue;
            }
            let (p2, c2, ref pb, ref zb) = arms[j];
            if shares_dir((p1, c1), (p2, c2)) {
                continue;
            }
            let Some((si, sj)) = polylines_cross(pa, pb) else {
                continue;
            };
            let ha = (za[si] as i64 + za[si + 1] as i64) / 2;
            let hb = (zb[sj] as i64 + zb[sj + 1] as i64) / 2;
            crossings += 1;
            let push = if c2 > c1 {
                side_push(pa[si], pa[si + 1], pb[sj], pb[sj + 1], p)
            } else {
                -side_push(pb[sj], pb[sj + 1], pa[si], pa[si + 1], p)
            };
            sub[c2].add(push);
            sub[c1].sub(push);
            // The upper arm rests on the lower one (tubes cannot pass through
            // each other). At the same height, the higher-index child's arm
            // is the one that rolls over.
            let a_over = if ha == hb { c1 > c2 } else { ha > hb };
            let (over, w, gap) = if a_over {
                (c1, ARM_W[si].max(ARM_W[si + 1]), ha - hb)
            } else {
                (c2, ARM_W[sj].max(ARM_W[sj + 1]), hb - ha)
            };
            let penetration = 2 * p.arm_radius as i64 - gap;
            if penetration > 0 && w > 0 {
                // Contact force on the arm, converted to its bend (the bend
                // moves the crossing point by `w/8` of itself).
                let lift = (p.collide_k * penetration).min(cap);
                bend_force_z[over] += lift * 8 / w;
            }
        }
    }

    // Accumulate down the tree: every file feels its own and all ancestors' pushes.
    let mut down = vec![IVec2::ZERO; n];
    for d in 0..n {
        let base = dirs[d]
            .parent
            .filter(|&q| q < d)
            .map_or(IVec2::ZERO, |q| down[q]);
        down[d] = base + sub[d].get();
    }

    // 3. Per-file forces: downstream pushes, cohesion and gravity.
    let mut forces = vec![Acc::default(); nf];
    let mut forces_z = vec![-(p.gravity as i64); nf];
    for (i, f) in files.iter().enumerate() {
        if f.dir_idx >= n {
            continue;
        }
        let d = f.dir_idx;
        forces[i].add(down[d]);
        let to_c = frame.centroid[d] - f.pos;
        let dist = to_c.length();
        let free = (frame.pack_radius[d] - f.radius).max(0);
        if dist > free
            && let Some(u) = to_c.unit()
        {
            let excess = (dist - free) as i64 * p.cohesion_k;
            forces[i].add(u.unit_times(excess.min(p.max_speed as i64)));
        }
    }

    let mut order: Vec<usize> = (0..nf).collect();
    order.sort_unstable_by_key(|&i| (files[i].pos.x, files[i].pos.y, files[i].tag));

    // Arms and files repel along the 3D normal: a file meeting an arm rolls
    // over it (arms are thin and rest on the floor), and the arm bends away.
    let arm_reach = max_r.saturating_add(p.arm_clear);
    for (ai, &(ep, ec, ref pts, ref zs)) in arms.iter().enumerate() {
        let (lo, hi) = boxes[ai];
        let start = order.partition_point(|&i| files[i].pos.x < lo.x.saturating_sub(arm_reach));
        let hi_x = hi.x.saturating_add(arm_reach);
        let (lo_y, hi_y) = (
            lo.y.saturating_sub(arm_reach),
            hi.y.saturating_add(arm_reach),
        );
        for &i in &order[start..] {
            let f = files[i];
            if f.pos.x > hi_x {
                break;
            }
            // Files of the arm's own directories, and of the directories it
            // leads into (whose own arms start at its tip), don't push on it.
            if f.pos.y < lo_y
                || f.pos.y > hi_y
                || f.dir_idx == ep
                || f.dir_idx == ec
                || dirs[f.dir_idx].parent == Some(ec)
            {
                continue;
            }
            let touch = f.radius.saturating_add(p.arm_radius) as i64;
            let clear = touch + p.arm_clear as i64;
            let fq = (f.pos, f.z);
            // Closest point over the polyline, with its bend weight (eighths, x64).
            let mut best: Option<(i64, P3, i64)> = None;
            for k in 0..4 {
                let (q, frac) = closest_on_segment3(fq, (pts[k], zs[k]), (pts[k + 1], zs[k + 1]));
                let (d2, ..) = delta3(q, fq);
                if best.is_none_or(|b| d2 < b.0) {
                    let w = ARM_W[k] * 64 + (ARM_W[k + 1] - ARM_W[k]) * frac;
                    best = Some((d2, q, w));
                }
            }
            let Some((d2, q, w)) = best else {
                continue;
            };
            if d2 >= clear * clear {
                continue;
            }
            let dist = isqrt(d2 as u64) as i64;
            let (_, dx, dy, dz) = delta3(q, fq);
            // Hard contact along the 3D normal (the file rolls over the
            // arm); soft clearance sideways only, at about the same height.
            let hard = if dist < touch {
                (p.collide_k * (touch - dist)).min(cap)
            } else {
                0
            };
            let (mut px, mut py, pz) = if dist > 0 {
                (dx * hard / dist, dy * hard / dist, dz * hard / dist)
            } else {
                (0, 0, hard)
            };
            let dxy = isqrt((dx * dx + dy * dy) as u64) as i64;
            if dz.abs() < touch && dxy > 0 {
                let soft = (p.arm_repel_k * (clear - dist)).min(cap);
                px += dx * soft / dxy;
                py += dy * soft / dxy;
            }
            let push = IVec2::new(sat(px), sat(py));
            forces[i].add(push);
            forces_z[i] += pz;
            bend_force[ec].sub(push.scale(w, 8 * 64));
            bend_force_z[ec] -= pz * w / (8 * 64);
        }
    }

    // Arms repel other arms (3D sample points of arms that share no directory).
    let mut samples: Vec<(usize, usize)> = Vec::with_capacity(arms.len() * 3);
    for ai in 0..arms.len() {
        for k in 1..4 {
            samples.push((ai, k));
        }
    }
    let spt = |s: (usize, usize)| -> P3 { (arms[s.0].2[s.1], arms[s.0].3[s.1]) };
    samples.sort_unstable_by_key(|&s| (spt(s).0.x, spt(s).0.y, arms[s.0].1, s.1));
    let range = p.arm_range as i64;
    for x in 0..samples.len() {
        let sa = samples[x];
        let pa = spt(sa);
        let (ap, ac, ..) = arms[sa.0];
        for &sb in &samples[(x + 1)..] {
            let pb = spt(sb);
            if (pb.0.x - pa.0.x) as i64 >= range {
                break;
            }
            if ((pb.0.y - pa.0.y) as i64).abs() >= range || sb.0 == sa.0 {
                continue;
            }
            let (bp, bc, ..) = arms[sb.0];
            if shares_dir((ap, ac), (bp, bc)) {
                continue;
            }
            let (d2, dx, dy, dz) = delta3(pa, pb);
            if d2 >= range * range {
                continue;
            }
            let dist = isqrt(d2 as u64) as i64;
            let mag = (p.arm_arm_k * (range - dist)).min(cap);
            // Sideways only: arms get over each other via the crossing lift.
            let _ = dz;
            let dxy = isqrt((dx * dx + dy * dy) as u64) as i64;
            let (px, py, pz) = if dxy > 0 {
                (dx * mag / dxy, dy * mag / dxy, 0)
            } else {
                let u = direction(GOLDEN_ANGLE.wrapping_mul((ac + bc) as u16 | 1));
                let v = u.unit_times(mag);
                (v.x as i64, v.y as i64, 0)
            };
            let push = IVec2::new(sat(px), sat(py));
            bend_force[ac].sub(push.scale(ARM_W[sa.1], 8));
            bend_force[bc].add(push.scale(ARM_W[sb.1], 8));
            bend_force_z[ac] -= pz * ARM_W[sa.1] / 8;
            bend_force_z[bc] += pz * ARM_W[sb.1] / 8;
        }
    }

    // Pairwise collision + cross-directory repulsion, in 3D (x-sorted sweep
    // and prune). Contacts push along the 3D normal, so a file pressed against
    // a lower one rides up over it instead of jamming.
    let reach = (2 * max_r).saturating_add(p.repel_gap.max(p.same_gap));
    const MAX_NEIGHBOURS: usize = 64;
    for a in 0..nf {
        let i = order[a];
        let fi = files[i];
        let x_limit = fi.pos.x.saturating_add(reach);
        let mut checks = 0usize;
        for &j in &order[(a + 1)..] {
            let fj = files[j];
            if fj.pos.x > x_limit {
                break;
            }
            let same = fi.dir_idx == fj.dir_idx;
            let contact = fi
                .radius
                .saturating_add(fj.radius)
                .saturating_add(p.same_gap);
            let min_d = if same {
                contact
            } else {
                fi.radius
                    .saturating_add(fj.radius)
                    .saturating_add(p.repel_gap)
                    .max(contact)
            };
            if (fi.pos.y - fj.pos.y).abs() >= min_d || (fi.z - fj.z).abs() >= min_d {
                continue;
            }
            checks += 1;
            if checks > MAX_NEIGHBOURS {
                break;
            }
            let (d2, dx, dy, dz) = delta3((fi.pos, fi.z), (fj.pos, fj.z));
            let min_d64 = min_d as i64;
            if d2 >= min_d64 * min_d64 {
                continue;
            }
            let (px, py, pz) = if d2 > 0 {
                let dist = isqrt(d2 as u64) as i64;
                // Hard contact along the 3D normal: the higher file rides up.
                let overlap = contact as i64 - dist;
                let hard = if overlap > 0 {
                    (p.collide_k * overlap).min(cap)
                } else {
                    0
                };
                let (mut px, mut py, pz) = (dx * hard / dist, dy * hard / dist, dz * hard / dist);
                // Soft cross-directory repulsion acts only sideways, between
                // files at about the same height (it must not levitate them).
                let dxy = isqrt((dx * dx + dy * dy) as u64) as i64;
                if !same && dz.abs() < contact as i64 && dxy > 0 && dxy < min_d64 {
                    let soft = (p.repel_k * (min_d64 - dxy)).min(cap);
                    px += dx * soft / dxy;
                    py += dy * soft / dxy;
                }
                (px, py, pz)
            } else {
                let ang = GOLDEN_ANGLE.wrapping_mul(fi.tag.wrapping_add(fj.tag).max(1));
                let v = direction(ang).unit_times(min_d64 * 4);
                (v.x as i64, v.y as i64, 0)
            };
            let push = IVec2::new(sat(px), sat(py));
            forces[i].sub(push);
            forces[j].add(push);
            forces_z[i] -= pz;
            forces_z[j] += pz;
        }
    }

    // 4. Integrate files with friction and light damping. The floor supports
    // every file that is not being deleted.
    let stat2 = (p.static_friction as i64).pow(2);
    for (i, f) in files.iter_mut().enumerate() {
        let acc = forces[i].get();
        let mut az = forces_z[i];
        let on_floor = !f.falling && f.z <= f.radius;
        if on_floor && az < 0 {
            az = 0; // the floor pushes back
        }
        // Friction grips on the floor and on the thin arms lying on it; a
        // file resting higher up, on other files, rolls freely.
        let grips = !f.falling && f.z <= f.radius + 2 * p.arm_radius + ONE;
        let az = sat(az);
        let still = f.vel == IVec2::ZERO && f.vz == 0;
        // Static friction: a file resting on the floor holds against small
        // net forces. One resting on other files has nothing to grip, so it
        // rolls off them.
        if still && grips && acc.len_sq() + (az as i64).pow(2) <= stat2 {
            continue;
        }
        // Sliding friction on the 3D speed (on the floor only: off it, files
        // roll freely over one another), then damping.
        let (v, vz) = (f.vel + acc, f.vz.saturating_add(az));
        let speed2 = v.len_sq() + (vz as i64).pow(2);
        let speed = isqrt(speed2 as u64) as i64;
        let (v, vz) = if !grips {
            (v, vz)
        } else if speed <= p.kinetic_friction as i64 {
            (IVec2::ZERO, 0)
        } else {
            let keep = speed - p.kinetic_friction as i64;
            (v.scale(keep, speed), sat(vz as i64 * keep / speed))
        };
        let (dn, dd) = p.damping;
        let mut v = v.scale(dn, dd).clamp_len(p.max_speed);
        let mut vz = sat(vz as i64 * dn / dd).clamp(-p.max_speed, p.max_speed);
        // Creeping slower than 2 px/s on the floor counts as stopped.
        if grips && v.len_sq() + (vz as i64).pow(2) < (2 * ONE as i64).pow(2) {
            v = IVec2::ZERO;
            vz = 0;
        }
        if v != IVec2::ZERO {
            f.pos = f.pos.step(v);
        }
        f.z = sat(f.z as i64 + crate::kernel::fixed::per_tick(vz as i64));
        if !f.falling && f.z < f.radius {
            f.z = f.radius;
            vz = vz.max(0);
        }
        if f.z < p.bottom {
            f.z = p.bottom;
            vz = 0;
            v = IVec2::ZERO;
        }
        f.vel = v;
        f.vz = vz;
    }

    // Arms: overdamped bending, relaxing straight over time; gravity pulls
    // arches back down to the floor.
    for &(_, d, ..) in &arms {
        let mut f = bend_force[d];
        f.sub(bends[d].xy.scale(p.arm_straighten_k, 1));
        let f = f.get().clamp_len(p.max_speed);
        if f.len_sq() > stat2 {
            bends[d].xy = bends[d].xy.step(f);
        }
        let fz = (bend_force_z[d] - p.gravity as i64).clamp(-cap, cap);
        if fz.abs() > p.static_friction as i64 {
            bends[d].z = sat(bends[d].z as i64 + crate::kernel::fixed::per_tick(fz));
        }
    }

    let mut out = dir_frame(dirs, files, p.same_gap);
    out.crossings = crossings;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn particle(dir_idx: usize, pos: IVec2, tag: u16) -> WorldFileParticle {
        WorldFileParticle {
            dir_idx,
            pos,
            vel: IVec2::ZERO,
            target_pos: IVec2::ZERO,
            size: 8 * ONE,
            target_size: 8 * ONE,
            radius: 4 * ONE,
            tag,
            z: 4 * ONE, // resting on the floor
            vz: 0,
            falling: false,
        }
    }

    fn run(dirs: &[ParticleDir], files: &mut [WorldFileParticle], ticks: usize) -> DirFrame {
        let p = ParticleParams::default();
        let mut bends = vec![ArmBend::default(); dirs.len()];
        let mut frame = dir_frame(dirs, files, p.same_gap);
        for _ in 0..ticks {
            frame = step_tree_particles(dirs, files, &mut bends, &p);
        }
        frame
    }

    fn assert_no_cross_dir_overlap(files: &[WorldFileParticle]) {
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let (a, b) = (files[i], files[j]);
                let (d2, ..) = delta3((a.pos, a.z), (b.pos, b.z));
                let dist = isqrt(d2 as u64) as Fx;
                let min = a.radius + b.radius;
                if a.dir_idx != b.dir_idx {
                    assert!(
                        dist >= min,
                        "files {i},{j} of different dirs overlap: {dist} < {min}"
                    );
                } else {
                    assert!(dist >= min - ONE, "files {i},{j} overlap: {dist} < {min}");
                }
            }
        }
    }

    #[test]
    fn dir_tip_tracks_file_centroid_and_empty_dirs_sit_between() {
        let dirs = [
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(1),
                visible: true,
            },
        ];
        let files = [
            particle(0, IVec2::new(-ONE, 0), 1),
            particle(0, IVec2::new(ONE, 0), 2),
            particle(2, IVec2::new(100 * ONE, 0), 3),
            particle(2, IVec2::new(100 * ONE, 20 * ONE), 4),
        ];
        let f = dir_frame(&dirs, &files, 0);
        assert_eq!(f.pos[0], IVec2::ZERO);
        assert_eq!(f.pos[2], IVec2::new(100 * ONE, 10 * ONE));
        // Empty dir 1 sits halfway between its parent tip and its child's anchor.
        assert_eq!(f.pos[1], IVec2::new(50 * ONE, 5 * ONE));
    }

    #[test]
    fn sibling_clusters_settle_apart_without_overlap() {
        let dirs = [
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
        ];
        // Each directory's files are dropped in a heap around where the
        // directory first appears (children right next to the parent).
        let spots = [
            IVec2::ZERO,
            IVec2::new(12 * ONE, 4 * ONE),
            IVec2::new(-6 * ONE, 12 * ONE),
        ];
        let mut files: Vec<WorldFileParticle> = (0..24)
            .map(|k| {
                let d = k % 3;
                let ang = GOLDEN_ANGLE.wrapping_mul(k as u16 + 1);
                particle(
                    d,
                    spots[d] + direction(ang).unit_times(3 * ONE as i64),
                    k as u16,
                )
            })
            .collect();
        let frame = run(&dirs, &mut files, 1200);
        assert!(
            files.iter().all(|f| f.vel == IVec2::ZERO),
            "did not come to rest"
        );
        assert_no_cross_dir_overlap(&files);
        // Tips are exactly at their own centroids.
        for d in 0..3 {
            assert_eq!(frame.pos[d], frame.centroid[d]);
        }
        // Arms come only from file/file interactions: each child cluster sits
        // just beyond the cross-directory repulsion range of the root's files.
        let p = ParticleParams::default();
        for d in 1..3 {
            let near = files
                .iter()
                .filter(|f| f.dir_idx == d)
                .flat_map(|a| {
                    files
                        .iter()
                        .filter(|f| f.dir_idx == 0)
                        .map(move |b| (a.pos - b.pos).length() - a.radius - b.radius)
                })
                .min()
                .unwrap();
            assert!(
                near <= p.repel_gap + 2 * ONE,
                "dir {d} drifted away: gap={near}"
            );
        }
    }

    #[test]
    fn crossed_edges_are_pushed_apart_downstream() {
        // root(0) -> a(1) -> a1(3), root(0) -> b(2) -> b1(4), with a1/b1 swapped so edges cross.
        let dirs = [
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(1),
                visible: true,
            },
            ParticleDir {
                parent: Some(2),
                visible: true,
            },
        ];
        let mut files = vec![
            particle(0, IVec2::ZERO, 1),
            particle(1, IVec2::new(60 * ONE, 60 * ONE), 2),
            particle(2, IVec2::new(60 * ONE, -60 * ONE), 3),
            particle(3, IVec2::new(140 * ONE, -90 * ONE), 4),
            particle(4, IVec2::new(140 * ONE, 90 * ONE), 5),
        ];
        let f0 = dir_frame(&dirs, &files, ONE / 2);
        assert!(segments_intersect(
            f0.pos[1], f0.pos[3], f0.pos[2], f0.pos[4]
        ));
        let f = run(&dirs, &mut files, 900);
        assert!(!segments_intersect(f.pos[1], f.pos[3], f.pos[2], f.pos[4]));
        assert_no_cross_dir_overlap(&files);
    }

    fn write_svg(
        path: &str,
        dirs: &[ParticleDir],
        files: &[WorldFileParticle],
        f: &DirFrame,
        bends: &[ArmBend],
    ) {
        use std::fmt::Write as _;
        let mut out =
            String::from("<svg xmlns='http://www.w3.org/2000/svg' viewBox='-800 -800 1600 1600'>");
        for d in 0..dirs.len() {
            if let Some(p) = dirs[d].parent {
                let (a, b) = (f.pos[p], f.pos[d]);
                let pts = arm_polyline(a, b, bends[d].xy);
                for k in 0..4 {
                    let _ = write!(
                        out,
                        "<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='black' stroke-width='1'/>",
                        pts[k].x / ONE,
                        pts[k].y / ONE,
                        pts[k + 1].x / ONE,
                        pts[k + 1].y / ONE
                    );
                }
            }
        }
        for fl in files {
            let _ = write!(
                out,
                "<circle cx='{}' cy='{}' r='{}' fill='hsl({},70%,60%)'/>",
                fl.pos.x / ONE,
                fl.pos.y / ONE,
                fl.radius / ONE,
                (fl.dir_idx * 47) % 360
            );
        }
        out.push_str("</svg>");
        std::fs::write(path, out).unwrap();
    }

    pub(super) fn spider() -> (Vec<ParticleDir>, Vec<WorldFileParticle>) {
        let mut dirs = vec![
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(1),
                visible: true,
            },
        ];
        let mut files = Vec::new();
        let mut tag = 0u16;
        for count in [56usize, 42] {
            let hub = dirs.len();
            dirs.push(ParticleDir {
                parent: Some(2),
                visible: true,
            });
            for _ in 0..count {
                let op = dirs.len();
                dirs.push(ParticleDir {
                    parent: Some(hub),
                    visible: true,
                });
                dirs.push(ParticleDir {
                    parent: Some(op),
                    visible: true,
                });
                for (d, n) in [(op, 1), (op + 1, 1)] {
                    for _ in 0..n {
                        tag += 1;
                        let ang = GOLDEN_ANGLE.wrapping_mul(tag);
                        files.push(particle(
                            d,
                            direction(ang).unit_times((tag as i64 % 7 + 1) * ONE as i64),
                            tag,
                        ));
                    }
                }
            }
        }
        // Preorder fix-up is implicit: hubs and ops are appended after their parents.
        (dirs, files)
    }

    #[test]
    fn spider_with_mixed_file_sizes_settles() {
        let (dirs, mut files) = spider();
        for (k, f) in files.iter_mut().enumerate() {
            let size = if k % 2 == 0 {
                6 * ONE
            } else {
                (12 + (k as i32 % 5) * 4) * ONE
            };
            f.size = ONE / 10;
            f.target_size = size;
        }
        let p = ParticleParams::default();
        let mut bends = vec![ArmBend::default(); dirs.len()];
        let mut still = 0;
        let mut settled = false;
        for _ in 0..2400 {
            step_tree_particles(&dirs, &mut files, &mut bends, &p);
            let maxv = files.iter().map(|f| f.vel.length()).max().unwrap_or(0);
            still = if maxv == 0 { still + 1 } else { 0 };
            if still >= 60 {
                settled = true;
                break;
            }
        }
        assert!(settled, "mixed-size spider never came to rest");
        assert_no_cross_dir_overlap(&files);
    }

    /// Every file dropped on one spot at once is a worst-case tangle (5,921
    /// crossings). It untangles to a few arms lying over each other and calms
    /// down to a handful of slowly creeping files, but does not fully come to
    /// rest; files arriving over time, as in the app, do (see
    /// `world::tests::spider_directories_settle_across_layout_engines`).
    #[test]
    fn spider_dropped_at_once_untangles() {
        let (dirs, mut files) = spider();
        let p = ParticleParams::default();
        let mut bends = vec![ArmBend::default(); dirs.len()];
        let mut f = dir_frame(&dirs, &files, p.same_gap);
        for _ in 0..2400 {
            f = step_tree_particles(&dirs, &mut files, &mut bends, &p);
        }
        if let Ok(path) = std::env::var("PARTICLES_SVG") {
            write_svg(&path, &dirs, &files, &f, &bends);
        }
        let moving = files.iter().filter(|f| f.vel != IVec2::ZERO).count();
        let fastest = files.iter().map(|f| f.vel.length()).max().unwrap_or(0);
        eprintln!(
            "spider: crossings={} moving={moving} fastest={} px/s",
            f.crossings,
            fastest / ONE
        );
        assert!(f.crossings <= 12, "{} crossings left", f.crossings);
        assert!(moving <= 8, "{moving} files still moving");
        assert!(
            fastest <= 40 * ONE,
            "fastest file at {} px/s",
            fastest / ONE
        );
        assert_no_cross_dir_overlap(&files);
    }

    fn one_dir() -> [ParticleDir; 1] {
        [ParticleDir {
            parent: None,
            visible: true,
        }]
    }

    #[test]
    fn dropped_file_falls_onto_the_floor_and_rests() {
        let dirs = one_dir();
        let mut files = vec![particle(0, IVec2::ZERO, 1)];
        files[0].z = 80 * ONE;
        run(&dirs, &mut files, 600);
        assert_eq!(files[0].z, files[0].radius, "rests on the floor");
        assert_eq!((files[0].vel, files[0].vz), (IVec2::ZERO, 0));
    }

    #[test]
    fn file_dropped_onto_another_rolls_off_it() {
        let dirs = one_dir();
        let mut files = vec![
            particle(0, IVec2::ZERO, 1),
            particle(0, IVec2::new(ONE / 2, 0), 2),
        ];
        files[1].z = 40 * ONE;
        run(&dirs, &mut files, 900);
        // Both end up side by side on the floor, not stacked or interpenetrating.
        for f in &files {
            assert_eq!(f.z, f.radius);
        }
        assert_no_cross_dir_overlap(&files);
    }

    #[test]
    fn deleted_file_falls_away_and_its_load_comes_down_to_the_floor() {
        let dirs = one_dir();
        let mut files = vec![particle(0, IVec2::ZERO, 1), particle(0, IVec2::ZERO, 2)];
        // File 1 rests on top of file 0.
        files[1].z = files[0].z + 8 * ONE;
        files[0].falling = true;
        let p = ParticleParams::default();
        run(&dirs, &mut files, 600);
        assert_eq!(files[0].z, p.bottom, "the deleted file falls to the bottom");
        assert_eq!(files[1].z, files[1].radius, "its load drops to the floor");
    }

    #[test]
    fn arms_crossing_at_the_same_height_roll_over_each_other() {
        // Same crossed setup as `crossed_edges_are_pushed_apart_downstream`:
        // while crossed, the higher-index arm arches up over the other.
        let dirs = [
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
            ParticleDir {
                parent: Some(1),
                visible: true,
            },
            ParticleDir {
                parent: Some(2),
                visible: true,
            },
        ];
        let mut files = vec![
            particle(0, IVec2::ZERO, 1),
            particle(1, IVec2::new(60 * ONE, 60 * ONE), 2),
            particle(2, IVec2::new(60 * ONE, -60 * ONE), 3),
            particle(3, IVec2::new(140 * ONE, -90 * ONE), 4),
            particle(4, IVec2::new(140 * ONE, 90 * ONE), 5),
        ];
        let p = ParticleParams::default();
        let mut bends = vec![ArmBend::default(); dirs.len()];
        let mut max_arch = 0;
        for _ in 0..30 {
            step_tree_particles(&dirs, &mut files, &mut bends, &p);
            max_arch = max_arch.max(bends[4].z);
        }
        assert!(max_arch > 0, "the higher-index arm arches over");
        assert_eq!(bends[3].z.max(0), 0, "the other arm stays down");
        // Once uncrossed, gravity brings the arch back down to the floor.
        run_with(&dirs, &mut files, &mut bends, 900);
        assert!(bends[4].z <= 0, "arch came back down: {}", bends[4].z);
    }

    fn run_with(
        dirs: &[ParticleDir],
        files: &mut [WorldFileParticle],
        bends: &mut [ArmBend],
        ticks: usize,
    ) {
        let p = ParticleParams::default();
        for _ in 0..ticks {
            step_tree_particles(dirs, files, bends, &p);
        }
    }

    #[test]
    fn deterministic() {
        let dirs = [
            ParticleDir {
                parent: None,
                visible: true,
            },
            ParticleDir {
                parent: Some(0),
                visible: true,
            },
        ];
        let mk = || -> Vec<WorldFileParticle> {
            (0..10)
                .map(|k| particle(k % 2, IVec2::new(k as i32, 0), k as u16))
                .collect()
        };
        let (mut a, mut b) = (mk(), mk());
        run(&dirs, &mut a, 100);
        run(&dirs, &mut b, 100);
        assert_eq!(a, b);
    }
}
