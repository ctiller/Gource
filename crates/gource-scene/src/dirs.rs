//! Directory dynamics: forces, integration, spline points, radii and
//! initial placement.
//!
//! The caller (gource-sim `World`) gathers a [`DirFrame`] each tick, calls
//! [`dir_accels`], then applies [`integrate`] and [`spline_point`] per
//! directory. All units are described in [`crate::fixed`].

use crate::fixed::{Acc, Fx, IVec2, ONE, PI_Q16, TICK_HZ, UNIT, div_round, mul_fx, sqrt_area};
use crate::grid::Grid;
use crate::rng::{pair_dir, salt};

/// Tunables, already converted to fixed point by the caller.
#[derive(Clone, Copy, Debug)]
pub struct DirParams {
    /// Gravity multiplier, Q8 (C++ `gGourceForceGravity`, default 10.0 → 2560).
    pub gravity: Fx,
    /// Whether gravity is applied at all (C++ `gGourceGravity`).
    pub gravity_on: bool,
    /// RNG seed.
    pub seed: u64,
}

/// One directory as seen by the force pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirIn {
    /// Stable entity id (RNG key); unique per directory.
    pub id: u64,
    pub pos: IVec2,
    /// `dir_radius`: the whole subtree's radius.
    pub radius: Fx,
    /// `parent_radius`: the radius of this directory's own file cluster.
    pub parent_radius: Fx,
    /// Index of the parent in [`DirFrame::dirs`], `None` for the root.
    pub parent: Option<u32>,
    /// C++ `isVisible()`: this directory or any descendant has a visible file.
    pub visible: bool,
    /// C++ `empty()`: no visible files and no child directories. Empty
    /// directories neither push nor are pushed by non-relatives.
    pub empty: bool,
}

/// A tick's directory tree in a flat layout.
#[derive(Clone, Debug, Default)]
pub struct DirFrame {
    /// Directories in pre-order (every parent precedes its children).
    pub dirs: Vec<DirIn>,
    /// Children of directory `i` are `children[child_range[i].0 .. child_range[i].1]`.
    pub children: Vec<u32>,
    pub child_range: Vec<(u32, u32)>,
    /// Euler-tour interval `[enter, exit)` of each directory over the
    /// pre-order indices: `a` is an ancestor-or-self of `b` iff
    /// `enter[a] <= enter[b] && exit[b] <= exit[a]`.
    pub enter: Vec<u32>,
    pub exit: Vec<u32>,
}

impl DirFrame {
    /// Build a frame from pre-order directories. Fills `children`,
    /// `child_range`, `enter` and `exit` from the `parent` links. Panics if
    /// a parent does not precede its child.
    pub fn from_preorder(dirs: Vec<DirIn>) -> DirFrame {
        let n = dirs.len();
        let mut child_counts = vec![0u32; n];

        for (i, d) in dirs.iter().enumerate() {
            if let Some(p) = d.parent {
                assert!(
                    (p as usize) < i,
                    "parent {p} must strictly precede child {i} in pre-order"
                );
                child_counts[p as usize] += 1;
            }
        }

        let mut child_range = Vec::with_capacity(n);
        let mut offset = 0u32;
        for &count in &child_counts {
            child_range.push((offset, offset + count));
            offset += count;
        }

        let total_children = offset as usize;
        let mut children = vec![0u32; total_children];
        let mut current_offset: Vec<u32> = child_range.iter().map(|&(start, _)| start).collect();

        for (i, d) in dirs.iter().enumerate() {
            if let Some(p) = d.parent {
                let slot = &mut current_offset[p as usize];
                children[*slot as usize] = i as u32;
                *slot += 1;
            }
        }

        let enter: Vec<u32> = (0..n as u32).collect();
        let mut exit: Vec<u32> = (1..=n as u32).collect();

        // In pre-order, descendants of node i form a contiguous slice [i, exit[i]).
        // If a node has children, its exit is the exit of its last descendant (or last child).
        // Traversed in reverse pre-order:
        for i in (0..n).rev() {
            let (start, end) = child_range[i];
            if start < end {
                let last_child = children[(end - 1) as usize] as usize;
                exit[i] = exit[last_child];
            }
        }

        DirFrame {
            dirs,
            children,
            child_range,
            enter,
            exit,
        }
    }

    /// True if `a` is a strict ancestor of `b`.
    #[inline]
    pub fn is_ancestor(&self, a: u32, b: u32) -> bool {
        if a == b {
            return false;
        }
        let (au, bu) = (a as usize, b as usize);
        if au >= self.enter.len() || bu >= self.enter.len() {
            return false;
        }
        self.enter[au] <= self.enter[bu] && self.exit[bu] <= self.exit[au]
    }
}

/// Compute acceleration for a single directory `d_idx`.
fn compute_dir_accel(
    d_idx: usize,
    frame: &DirFrame,
    params: &DirParams,
    tick: u64,
    grid: &Grid,
    query_buf: &mut Vec<u32>,
) -> IVec2 {
    let d = &frame.dirs[d_idx];
    let Some(p_idx) = d.parent else {
        return IVec2::ZERO;
    };
    let p = &frame.dirs[p_idx as usize];

    let mut acc = Acc::default();

    // 1. Overlap push from non-empty o (not d, not p, not relative)
    if !d.empty {
        let d_min = d.pos - IVec2::splat(d.radius);
        let d_max = d.pos + IVec2::splat(d.radius);
        grid.query(d_min, d_max, query_buf);

        for &o_idx in query_buf.iter() {
            let o_u = o_idx as usize;
            if o_u == d_idx || o_idx == p_idx {
                continue;
            }
            if frame.is_ancestor(o_idx, d_idx as u32) || frame.is_ancestor(d_idx as u32, o_idx) {
                continue;
            }
            let o = &frame.dirs[o_u];
            if o.empty {
                continue;
            }

            let o_min = o.pos - IVec2::splat(o.radius);
            let o_max = o.pos + IVec2::splat(o.radius);
            if d_min.x > o_max.x || d_max.x < o_min.x || d_min.y > o_max.y || d_max.y < o_min.y {
                continue;
            }

            let v = o.pos - d.pos;
            let dist = v.length();
            let sum = d.radius + o.radius;

            if dist == 0 {
                let dir = pair_dir(params.seed, tick, o.id, d.id, salt::DIR_COINCIDENT);
                acc.add(dir.unit_times(ONE as i64));
            } else if dist < sum {
                let k = (dist - sum) as i64;
                acc.add(v.scale(k, dist as i64));
            }
        }
    }

    // 2. Parent overlap
    {
        let v = p.pos - d.pos;
        let dist = v.length();
        let sum = d.radius + p.radius;
        if dist == 0 {
            let dir = pair_dir(params.seed, tick, p.id, d.id, salt::DIR_COINCIDENT);
            acc.add(dir.unit_times(ONE as i64));
        } else if dist < sum {
            let k = (dist - sum) as i64;
            acc.add(v.scale(k, dist as i64));
        }
    }

    // 3. Gravity
    if params.gravity_on {
        let v = p.pos - d.pos;
        if let Some(u) = v.unit() {
            let parent_dist = v.length() as i64 - (d.radius as i64 + p.parent_radius as i64);
            let mag = div_round(params.gravity as i64 * parent_dist, ONE as i64);
            acc.add(u.unit_times(mag));
        }
    }

    // 4. Grandparent push
    if let Some(g_idx) = p.parent {
        let g = &frame.dirs[g_idx as usize];
        let e = (p.pos - g.pos).unit().unwrap_or(IVec2::ZERO);
        let arm = e.unit_times(p.radius as i64 + d.radius as i64);
        let push = p.pos + arm - d.pos;
        acc.add(push);
    }

    // 5. Sibling spread
    {
        let (start, end) = frame.child_range[p_idx as usize];
        let mut sib_acc = Acc::default();
        let mut visible_siblings_count = 0u32;

        for &s_idx in &frame.children[start as usize..end as usize] {
            if s_idx as usize == d_idx {
                continue;
            }
            let s = &frame.dirs[s_idx as usize];
            if s.visible {
                visible_siblings_count += 1;
                let v = s.pos - d.pos;
                if let Some(u) = v.unit() {
                    sib_acc.sub(u);
                }
            }
        }

        let n = 1 + visible_siblings_count as i64;
        if n > 1 {
            // Q8 radius * Q16 pi / Q16 = Q8.
            let slice = div_round(p.radius as i64 * PI_Q16, (n + 1) << 16);
            let s_unit = sib_acc.get();
            acc.add(s_unit.scale(slice, UNIT as i64));
        }
    }

    acc.get()
}

/// Each directory's acceleration (Q8 units/s) for this tick; zero for the
/// root. The result is identical for any `threads` value (integer sums).
///
/// Per non-root directory `d` with parent `p` (C++ `applyForces`):
/// 1. **Overlap push** from every non-empty `o` that is not `d`, not `p`,
///    and not an ancestor or descendant of `d`, whose box `pos ± radius`
///    overlaps `d`'s box: with `v = o.pos - d.pos`, `dist = |v|`,
///    `sum = d.radius + o.radius`, if `dist < sum`, add
///    `(dist - sum) * v / dist` (pushes apart). If `dist == 0`, add one
///    unit (`ONE`) along [`crate::rng::pair_dir`]`(seed, tick, o.id, d.id,
///    salt::DIR_COINCIDENT)`, i.e. away from `o`.
/// 2. **Parent overlap**: the same rule against `p` (radius `p.radius`),
///    always considered.
/// 3. **Gravity** (if `gravity_on`): `parent_dist = |p.pos - d.pos| -
///    (d.radius + p.parent_radius)`; add `gravity * parent_dist * unit(p.pos
///    - d.pos)` (Q8 * Q8 → Q8: divide by `ONE`).
/// 4. **Grandparent push**: if `p` has a parent `g`, with `e = unit(p.pos -
///    g.pos)` (zero if coincident): add `p.pos + (p.radius + d.radius) * e -
///    d.pos`.
/// 5. **Sibling spread**: over `p`'s other children `s` with `s.visible`:
///    `acc -= unit(s.pos - d.pos)` (skipping coincident ones, which still
///    count), `n` = 1 + number of such siblings. If `n > 1`, add
///    `acc * slice` where `slice = p.radius * PI / (n + 1)`.
///
/// Use [`crate::grid::Grid`] over non-empty directories for step 1, and
/// split the work over `threads` scoped threads (`std::thread::scope`; on
/// `wasm32` always serial).
pub fn dir_accels(frame: &DirFrame, params: &DirParams, tick: u64, threads: usize) -> Vec<IVec2> {
    let n = frame.dirs.len();
    if n == 0 {
        return Vec::new();
    }

    let non_empty_items: Vec<(u32, IVec2, IVec2)> = frame
        .dirs
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.empty)
        .map(|(i, d)| {
            let r = IVec2::splat(d.radius);
            (i as u32, d.pos - r, d.pos + r)
        })
        .collect();

    let shift = Grid::auto_shift(frame.dirs.iter().filter(|d| !d.empty).map(|d| d.radius));
    let grid = Grid::build(non_empty_items, shift);

    let mut accels = vec![IVec2::ZERO; n];

    #[cfg(not(target_arch = "wasm32"))]
    {
        if threads <= 1 || n < 2 {
            let mut query_buf = Vec::new();
            for (i, out) in accels.iter_mut().enumerate() {
                *out = compute_dir_accel(i, frame, params, tick, &grid, &mut query_buf);
            }
        } else {
            let chunk_size = n.div_ceil(threads);
            std::thread::scope(|s| {
                for (chunk_idx, chunk) in accels.chunks_mut(chunk_size).enumerate() {
                    let start_idx = chunk_idx * chunk_size;
                    let grid_ref = &grid;
                    s.spawn(move || {
                        let mut query_buf = Vec::new();
                        for (offset, out) in chunk.iter_mut().enumerate() {
                            let i = start_idx + offset;
                            *out =
                                compute_dir_accel(i, frame, params, tick, grid_ref, &mut query_buf);
                        }
                    });
                }
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = threads;
        let mut query_buf = Vec::new();
        for (i, out) in accels.iter_mut().enumerate() {
            *out = compute_dir_accel(i, frame, params, tick, &grid, &mut query_buf);
        }
    }

    accels
}

/// The new position after one tick at `accel`: `pos + accel / TICK_HZ`.
pub fn integrate(pos: IVec2, accel: IVec2) -> IVec2 {
    pos.step(accel)
}

/// C++ `updateSplinePoint` for one tick (`dt = 1/TICK_HZ`): `td = (parent -
/// pos) / 2`, `mid = pos + td`, `delta = mid - spos`; if `|delta| > |td|`,
/// first move `spos` by `unit(delta) * (|delta| - |td|)`; then (recomputing
/// nothing) `spos += delta * 2 / TICK_HZ`.
pub fn spline_point(mut spos: IVec2, pos: IVec2, parent: IVec2) -> IVec2 {
    let td = (parent - pos).scale(1, 2);
    let mid = pos + td;
    let delta = mid - spos;
    let delta_len = delta.length();
    let td_len = td.length();

    if delta_len > td_len
        && let Some(u) = delta.unit()
    {
        spos += u.unit_times((delta_len - td_len) as i64);
    }

    spos += delta.scale(2, TICK_HZ as i64);
    spos
}

/// C++ `setInitialPosition`. `hash_dir` is the directory's hash direction
/// (UNIT-scaled, see [`hash_direction`]). Without a grandparent:
/// `parent + hash_dir` scaled to one unit. With one: `parent + unit(2 *
/// unit(parent - grandparent) + hash_dir)` scaled to one unit.
pub fn initial_position(parent: IVec2, grandparent: Option<IVec2>, hash_dir: IVec2) -> IVec2 {
    let offset_dir = match grandparent {
        None => hash_dir,
        Some(g) => {
            let pg_dir = (parent - g).unit().unwrap_or(IVec2::ZERO);
            let combined = pg_dir * 2 + hash_dir;
            combined.unit().unwrap_or(IVec2::ZERO)
        }
    };
    parent + offset_dir.unit_times(ONE as i64)
}

/// C++ `vec2Hash` from the integer string hash: `x = (h / 7) % 255 - 127`,
/// `y = (h / 3) % 255 - 127`, normalised to [`UNIT`](crate::UNIT) (zero if
/// both are zero).
pub fn hash_direction(h: i32) -> IVec2 {
    let x = (h / 7) % 255 - 127;
    let y = (h / 3) % 255 - 127;
    IVec2::new(x, y).unit().unwrap_or(IVec2::ZERO)
}

/// Radii of a directory (C++ `calcRadius`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Radii {
    /// Subtree area, Q16.
    pub area: i64,
    /// `dir_radius`: `max(ONE, sqrt(area)) * padding`.
    pub radius: Fx,
    /// `parent_radius`: `max(ONE, sqrt(file_area * visible) * padding)`.
    pub parent_radius: Fx,
}

/// `file_area` is one file's area (Q16), `visible` the visible file count,
/// `children_area` the sum of the children's [`Radii::area`], `padding` Q8
/// (C++ `gGourceDirPadding`, default 1.5 → 384).
pub fn radii(file_area: i64, visible: u32, children_area: i64, padding: Fx) -> Radii {
    let total_file_area = file_area.saturating_mul(visible as i64);
    let area = total_file_area.saturating_add(children_area);

    let dir_r = sqrt_area(area).max(ONE);
    let radius = mul_fx(dir_r, padding);

    let parent_radius = mul_fx(sqrt_area(total_file_area), padding).max(ONE);

    Radii {
        area,
        radius,
        parent_radius,
    }
}

/// One file's area (Q16) for a file diameter (Q8): `PI * (d / 2)^2`.
pub fn file_area(diameter: Fx) -> i64 {
    let r = diameter / 2;
    div_round(
        PI_Q16 * (r as i64) * (r as i64),
        (ONE as i64) * (ONE as i64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_from_preorder_and_is_ancestor() {
        // Tree structure:
        // 0 (root)
        // |-- 1
        // |   |-- 2
        // |   `-- 3
        // `-- 4
        let dirs = vec![
            DirIn {
                id: 0,
                parent: None,
                ..Default::default()
            },
            DirIn {
                id: 1,
                parent: Some(0),
                ..Default::default()
            },
            DirIn {
                id: 2,
                parent: Some(1),
                ..Default::default()
            },
            DirIn {
                id: 3,
                parent: Some(1),
                ..Default::default()
            },
            DirIn {
                id: 4,
                parent: Some(0),
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        assert_eq!(frame.children, vec![1, 4, 2, 3]);
        assert_eq!(
            frame.child_range,
            vec![(0, 2), (2, 4), (4, 4), (4, 4), (4, 4)]
        );
        assert_eq!(frame.enter, vec![0, 1, 2, 3, 4]);
        assert_eq!(frame.exit, vec![5, 4, 3, 4, 5]);

        // Ancestry tests
        assert!(!frame.is_ancestor(0, 0)); // Strict: self is not ancestor
        assert!(frame.is_ancestor(0, 1));
        assert!(frame.is_ancestor(0, 2));
        assert!(frame.is_ancestor(0, 3));
        assert!(frame.is_ancestor(0, 4));

        assert!(frame.is_ancestor(1, 2));
        assert!(frame.is_ancestor(1, 3));
        assert!(!frame.is_ancestor(1, 4));
        assert!(!frame.is_ancestor(1, 0));

        assert!(!frame.is_ancestor(2, 3));
        assert!(!frame.is_ancestor(3, 2));
        assert!(!frame.is_ancestor(4, 1));

        // Out of bounds handling
        assert!(!frame.is_ancestor(10, 1));
        assert!(!frame.is_ancestor(1, 10));
    }

    #[test]
    #[should_panic(expected = "parent 2 must strictly precede child 1 in pre-order")]
    fn frame_panics_on_invalid_preorder() {
        let dirs = vec![
            DirIn {
                id: 0,
                parent: None,
                ..Default::default()
            },
            DirIn {
                id: 1,
                parent: Some(2), // invalid: parent 2 >= child 1
                ..Default::default()
            },
            DirIn {
                id: 2,
                parent: Some(0),
                ..Default::default()
            },
        ];
        DirFrame::from_preorder(dirs);
    }

    #[test]
    fn overlap_push_isolated() {
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 42,
        };
        // To isolate overlap push completely, let p have NO parent (i.e. p is root)
        // and be far away so parent overlap does not apply.
        // d=1 at (10*ONE, 0), o=2 at (14*ONE, 0), radii = 5*ONE.
        // v = o - d = (4*ONE, 0), dist = 4*ONE, sum = 10*ONE.
        // Overlap push = (4 - 10)*ONE * (4*ONE, 0) / (4*ONE) = (-6*ONE, 0).
        let isolated_dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::new(0, 100 * ONE), // root parent far away
                radius: 10 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(10 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(14 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
        ];
        let iso_frame = DirFrame::from_preorder(isolated_dirs);
        let iso_accels = dir_accels(&iso_frame, &params, 0, 1);
        // Dir 1:
        // Overlap with 2: v = (4*ONE, 0), dist = 4*ONE, sum = 10*ONE.
        // Push = (-6*ONE, 0).
        // Parent overlap with 0 at (0, 100*ONE): dist = sqrt(100 + 10000) > 15 -> 0.
        // Sibling spread with 2:
        // visible siblings = 0 (since visible defaults to false)!
        assert_eq!(iso_accels[1], IVec2::new(-6 * ONE, 0));
        assert_eq!(iso_accels[2], IVec2::new(6 * ONE, 0));
    }

    #[test]
    fn coincident_push_uses_pair_dir() {
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 999,
        };
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::new(0, 100 * ONE),
                radius: 10 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(10 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(10 * ONE, 0), // coincident with 1
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let tick = 42;
        let accels = dir_accels(&frame, &params, tick, 1);

        // Expected push on 1: away from 2 along pair_dir(seed, tick, 2.id, 1.id, salt::DIR_COINCIDENT) * ONE
        let expected_1 =
            pair_dir(params.seed, tick, 2, 1, salt::DIR_COINCIDENT).unit_times(ONE as i64);
        let expected_2 =
            pair_dir(params.seed, tick, 1, 2, salt::DIR_COINCIDENT).unit_times(ONE as i64);
        assert_eq!(accels[1], expected_1);
        assert_eq!(accels[2], expected_2);
        assert_eq!(accels[1], -accels[2]);
    }

    #[test]
    fn parent_overlap_isolated() {
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 1,
        };
        // Root at (0, 0), radius 10*ONE.
        // Child 1 at (6*ONE, 0), radius 6*ONE.
        // sum = 16*ONE. dist = 6*ONE < 16*ONE.
        // v = p - d = (-6*ONE, 0).
        // Push = (dist - sum) * v / dist = (6 - 16)*ONE * (-6*ONE, 0) / (6*ONE) = (10*ONE, 0).
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(6 * ONE, 0),
                radius: 6 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let accels = dir_accels(&frame, &params, 0, 1);
        assert_eq!(accels[0], IVec2::ZERO);
        assert_eq!(accels[1], IVec2::new(10 * ONE, 0));
    }

    #[test]
    fn parent_coincident_push() {
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 77,
        };
        // Child coincident with parent
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::ZERO,
                radius: 10 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::ZERO,
                radius: 6 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let accels = dir_accels(&frame, &params, 5, 1);
        let expected = pair_dir(params.seed, 5, 0, 1, salt::DIR_COINCIDENT).unit_times(ONE as i64);
        assert_eq!(accels[1], expected);
    }

    #[test]
    fn gravity_force_on_and_off() {
        // p at (0, 0), p.parent_radius = 5*ONE.
        // d at (20*ONE, 0), d.radius = 5*ONE.
        // parent_dist = |p - d| - (d.radius + p.parent_radius) = 20*ONE - 10*ONE = 10*ONE.
        // v = p - d = (-20*ONE, 0). unit(v) = (-UNIT, 0).
        // gravity = 2560 (10.0 in Q8).
        // mag = div_round(2560 * 10*ONE, ONE) = 25600.
        // push = unit(v).unit_times(mag) = (-100 * ONE, 0)
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::ZERO,
                radius: 50 * ONE,
                parent_radius: 5 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(20 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);

        // With gravity_on = false
        let p_off = DirParams {
            gravity: 2560,
            gravity_on: false,
            seed: 0,
        };
        let acc_off = dir_accels(&frame, &p_off, 0, 1);
        // p.radius = 50, d.radius = 5. sum = 55. dist = 20 < 55.
        // Parent overlap push = (20 - 55)*ONE * (-20*ONE, 0) / (20*ONE) = (35*ONE, 0).
        assert_eq!(acc_off[1], IVec2::new(35 * ONE, 0));

        // With gravity_on = true
        let p_on = DirParams {
            gravity: 2560,
            gravity_on: true,
            seed: 0,
        };
        let acc_on = dir_accels(&frame, &p_on, 0, 1);
        // Parent overlap (35*ONE) + gravity (-100*ONE) = (-65*ONE, 0)
        assert_eq!(acc_on[1], IVec2::new((35 - 100) * ONE, 0));
    }

    #[test]
    fn grandparent_push_isolated() {
        // g at (0, 0).
        // p at (10*ONE, 0), p.radius = 5*ONE.
        // d at (12*ONE, 0), d.radius = 3*ONE.
        // e = unit(p - g) = unit(10*ONE, 0) = (UNIT, 0).
        // arm = (p.radius + d.radius) * e = 8*ONE * (UNIT, 0) = (8*ONE, 0).
        // push = p.pos + arm - d.pos = (10 + 8 - 12)*ONE = (6*ONE, 0).
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::ZERO,
                radius: 50 * ONE,
                parent: None,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(10 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(12 * ONE, 0),
                radius: 3 * ONE,
                parent: Some(1),
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 0,
        };
        let acc = dir_accels(&frame, &params, 0, 1);

        // On 2:
        // Parent is 1 at (10, 0). dist = 2*ONE. sum = 5 + 3 = 8*ONE.
        // Parent overlap = (2 - 8)*ONE * (-2*ONE, 0) / (2*ONE) = (6*ONE, 0).
        // Grandparent push = (6*ONE, 0).
        // Total = 12*ONE.
        assert_eq!(acc[2], IVec2::new(12 * ONE, 0));
    }

    #[test]
    fn sibling_spread_and_invisible_siblings_ignored() {
        // Parent at (0, 100*ONE), radius = 7 * ONE.
        // Children of p:
        // Child 1 at (0, 0), visible = true.
        // Child 2 at (10*ONE, 0), visible = true.
        // Child 3 at (0, 10*ONE), visible = false (invisible, must be ignored!).
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::new(0, 100 * ONE),
                radius: 7 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::ZERO,
                radius: ONE,
                parent: Some(0),
                visible: true,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(10 * ONE, 0),
                radius: ONE,
                parent: Some(0),
                visible: true,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 3,
                pos: IVec2::new(0, 10 * ONE),
                radius: ONE,
                parent: Some(0),
                visible: false,
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 0,
        };
        let acc = dir_accels(&frame, &params, 0, 1);

        // On Child 1:
        // Sibling 2 is visible: v = (10*ONE, 0), unit = (UNIT, 0). sib_acc -= (UNIT, 0) => sib_acc = (-UNIT, 0).
        // Sibling 3 is invisible: ignored.
        // n = 1 + 1 = 2.
        // slice = div_round(p.radius * PI_Q16, (n + 1) << 16) = div_round(7 * 205887, 3 * 256) = 1877
        // (7 * pi / 3 = 7.33 units).
        // acc.add(s_unit.scale(slice, UNIT)):
        // s_unit = (-UNIT, 0). scale(slice, UNIT) = (-1877, 0).
        // Since dist to 2 is 10*ONE > r1 + r2 (2*ONE), overlap push = 0.
        // Dist to parent is 100*ONE > 8*ONE, parent overlap = 0.
        assert_eq!(acc[1].x, -1877);
        assert_eq!(acc[1].y, 0);

        // On Child 2:
        // Sibling 1 is visible: v = (-10*ONE, 0), unit = (-UNIT, 0). sib_acc -= (-UNIT, 0) = (UNIT, 0).
        // Spread force = (+1877, 0).
        assert_eq!(acc[2].x, 1877);
        assert_eq!(acc[2].y, 0);

        // If only 1 visible child exists total:
        let dirs_one_vis = vec![
            DirIn {
                id: 0,
                pos: IVec2::new(0, 100 * ONE),
                radius: 7 * ONE,
                parent: None,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::ZERO,
                radius: ONE,
                parent: Some(0),
                visible: true,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(10 * ONE, 0),
                radius: ONE,
                parent: Some(0),
                visible: false,
                ..Default::default()
            },
        ];
        let frame_one_vis = DirFrame::from_preorder(dirs_one_vis);
        let acc_one = dir_accels(&frame_one_vis, &params, 0, 1);
        // n = 1, so n > 1 is false: sibling spread is 0!
        assert_eq!(acc_one[1], IVec2::ZERO);
    }

    #[test]
    fn ancestors_and_descendants_excluded_from_overlap() {
        // Line of dirs: 0 -> 1 -> 2.
        // 1 is child of 0. 2 is child of 1.
        // 2 and 0 overlap in space, but 0 is ancestor of 2.
        // 2's parent is 1. Overlap push with 0 MUST NOT happen!
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::ZERO,
                radius: 20 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(100 * ONE, 0), // parent of 2 is far away
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(5 * ONE, 0), // 2 is inside 0's radius (20*ONE)!
                radius: 5 * ONE,
                parent: Some(1),
                empty: false,
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 0,
        };
        let acc = dir_accels(&frame, &params, 0, 1);

        // 2 has parent 1 at (100, 0), grandparent 0 at (0, 0).
        // e = unit(p - g) = (UNIT, 0).
        // arm = (r_p + r_d) * e = (5 + 5) * (UNIT, 0) = (10*ONE, 0).
        // Grandparent push on 2 = p + arm - d = (100 + 10 - 5)*ONE = (105*ONE, 0).
        // Overlap push from 0 must be 0!
        assert_eq!(acc[2], IVec2::new(105 * ONE, 0));
    }

    #[test]
    fn empty_dirs_excluded_from_overlap() {
        let params = DirParams {
            gravity: 0,
            gravity_on: false,
            seed: 0,
        };
        // 0 -> 1, 0 -> 2. 2 is empty.
        // 1 and 2 overlap. 1 is non-empty, 2 is empty.
        // Empty dirs neither push nor are pushed by non-relatives.
        let dirs = vec![
            DirIn {
                id: 0,
                pos: IVec2::new(0, 100 * ONE),
                radius: 10 * ONE,
                parent: None,
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 1,
                pos: IVec2::new(5 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: false,
                ..Default::default()
            },
            DirIn {
                id: 2,
                pos: IVec2::new(6 * ONE, 0),
                radius: 5 * ONE,
                parent: Some(0),
                empty: true, // empty!
                ..Default::default()
            },
        ];
        let frame = DirFrame::from_preorder(dirs);
        let acc = dir_accels(&frame, &params, 0, 1);
        assert_eq!(acc[1], IVec2::ZERO);
        assert_eq!(acc[2], IVec2::ZERO);
    }

    #[test]
    fn multithread_identical_results_500_dirs() {
        // Construct a pseudo-random tree of ~500 dirs
        let n = 500;
        let mut dirs = Vec::with_capacity(n);
        dirs.push(DirIn {
            id: 0,
            pos: IVec2::ZERO,
            radius: 50 * ONE,
            parent_radius: 20 * ONE,
            parent: None,
            visible: true,
            empty: false,
        });

        // Deterministic pseudo-random generation without external crates
        let mut lcg = 123456789u64;
        let mut next_u64 = || {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1);
            lcg
        };

        for i in 1..n {
            let p = (next_u64() % (i as u64)) as u32;
            let x = ((next_u64() % 400) as i32 - 200) * ONE;
            let y = ((next_u64() % 400) as i32 - 200) * ONE;
            let radius = ((next_u64() % 20) as i32 + 5) * ONE;
            let parent_radius = ((next_u64() % 10) as i32 + 2) * ONE;
            let visible = (next_u64() % 3) != 0;
            let empty = (next_u64() % 5) == 0;
            dirs.push(DirIn {
                id: i as u64,
                pos: IVec2::new(x, y),
                radius,
                parent_radius,
                parent: Some(p),
                visible,
                empty,
            });
        }

        let frame = DirFrame::from_preorder(dirs);
        let params = DirParams {
            gravity: 2560,
            gravity_on: true,
            seed: 987654321,
        };

        let res_1 = dir_accels(&frame, &params, 100, 1);
        for &t in &[2, 3, 8] {
            let res_t = dir_accels(&frame, &params, 100, t);
            assert_eq!(res_1, res_t, "results differ for thread count {t}");
        }
    }

    #[test]
    fn spline_point_tests() {
        let parent = IVec2::new(100 * ONE, 0);
        let pos = IVec2::new(0, 0);
        // td = (parent - pos) / 2 = (50*ONE, 0).
        // mid = pos + td = (50*ONE, 0).
        // If spos == mid, delta = 0.
        assert_eq!(
            spline_point(IVec2::new(50 * ONE, 0), pos, parent),
            IVec2::new(50 * ONE, 0)
        );

        // If spos is far away: spos = (0, 0).
        // delta = mid - spos = (50*ONE, 0).
        // |delta| = 50*ONE, |td| = 50*ONE. |delta| == |td|, not strictly greater.
        // Move spos: no.
        // spos += delta * 2 / TICK_HZ = (0, 0) + (50*ONE * 2 / 60) = (100*ONE / 60)
        let s_out = spline_point(IVec2::ZERO, pos, parent);
        assert_eq!(
            s_out,
            IVec2::new(div_round(50 * ONE as i64 * 2, 60) as i32, 0)
        );

        // When |delta| > |td|:
        // spos at (-50*ONE, 0). delta = mid - spos = (100*ONE, 0).
        // |delta| = 100*ONE > |td| = 50*ONE.
        // First move spos by unit(delta) * (100 - 50)*ONE = (50*ONE, 0).
        // spos becomes (-50*ONE, 0) + (50*ONE, 0) = (0, 0).
        // Then spos += delta * 2 / TICK_HZ = (0, 0) + (100*ONE * 2 / 60).
        let s_far = spline_point(IVec2::new(-50 * ONE, 0), pos, parent);
        assert_eq!(
            s_far,
            IVec2::new(div_round(100 * ONE as i64 * 2, 60) as i32, 0)
        );
    }

    #[test]
    fn initial_position_tests() {
        let parent = IVec2::new(10 * ONE, 20 * ONE);
        let h_dir = IVec2::new(UNIT, 0); // +x direction

        // Without grandparent
        let p_no_g = initial_position(parent, None, h_dir);
        assert_eq!(p_no_g, parent + IVec2::new(ONE, 0));

        // With grandparent: grandparent at (10*ONE, 10*ONE).
        // parent - grandparent = (0, 10*ONE). unit = (0, UNIT).
        // combined = 2 * (0, UNIT) + (UNIT, 0) = (UNIT, 2 * UNIT).
        // combined.unit() * ONE:
        let g = IVec2::new(10 * ONE, 10 * ONE);
        let p_with_g = initial_position(parent, Some(g), h_dir);
        let expected_offset = IVec2::new(UNIT, 2 * UNIT)
            .unit()
            .unwrap()
            .unit_times(ONE as i64);
        assert_eq!(p_with_g, parent + expected_offset);
    }

    #[test]
    fn hash_direction_tests() {
        // Zero cases: h = 2676, 2677, 2678 give x = 0, y = 0
        for &h in &[2676, 2677, 2678, 8031] {
            let h_zero = hash_direction(h);
            assert_eq!(h_zero, IVec2::ZERO, "failed for h = {h}");
        }

        // Non-zero hash
        let d = hash_direction(42);
        assert_ne!(d, IVec2::ZERO);
        let len = d.length();
        assert!((len - UNIT).abs() <= 2);

        // Test formula directly against manual calculation
        // h = 100:
        // 100 / 7 = 14; 14 % 255 = 14; x = 14 - 127 = -113
        // 100 / 3 = 33; 33 % 255 = 33; y = 33 - 127 = -94
        let expected_100 = IVec2::new(-113, -94).unit().unwrap();
        assert_eq!(hash_direction(100), expected_100);
    }

    #[test]
    fn radii_and_file_area_tests() {
        // file_area(8 * ONE): diameter = 8 * ONE, radius = 4 * ONE = 1024
        // area = div_round(PI_Q16 * 1024 * 1024, 256 * 256) = 16 * PI_Q16 = 3294192
        let fa = file_area(8 * ONE);
        assert_eq!(fa, 16 * PI_Q16);

        // Empty dir with padding 1.5 (384):
        let r_empty = radii(fa, 0, 0, 384);
        assert_eq!(r_empty.area, 0);
        assert_eq!(r_empty.radius, 384); // max(ONE, 0) * 1.5 = 1.5 * ONE = 384
        assert_eq!(r_empty.parent_radius, ONE); // max(ONE, 0 * 1.5) = ONE = 256

        // Visible files:
        let r_vis = radii(fa, 2, 0, 384);
        assert_eq!(r_vis.area, 2 * 16 * PI_Q16);
        // dir_radius = sqrt(32 * PI_Q16) * 1.5
        let expected_sqrt = sqrt_area(2 * 16 * PI_Q16);
        assert_eq!(r_vis.radius, mul_fx(expected_sqrt, 384));
        assert_eq!(r_vis.parent_radius, mul_fx(expected_sqrt, 384));
    }

    #[test]
    fn convergence_sanity_test() {
        // A parent with 6 children starting coincident-ish settles after ~2000 ticks
        // without overlaps beyond tolerance.
        let parent_pos = IVec2::ZERO;
        let parent_radius = 20 * ONE;
        let parent_pradius = 5 * ONE;
        let child_radius = 5 * ONE;

        let mut child_positions = [
            IVec2::new(1, 0),
            IVec2::new(-1, 0),
            IVec2::new(0, 1),
            IVec2::new(0, -1),
            IVec2::new(1, 1),
            IVec2::new(-1, -1),
        ];

        let params = DirParams {
            gravity: 2560,
            gravity_on: true,
            seed: 12345,
        };

        let mut last_moves = [0i32; 6];

        for tick in 0..2000 {
            let mut dirs = Vec::with_capacity(7);
            dirs.push(DirIn {
                id: 0,
                pos: parent_pos,
                radius: parent_radius,
                parent_radius: parent_pradius,
                parent: None,
                visible: true,
                empty: false,
            });
            for (i, &pos) in child_positions.iter().enumerate() {
                dirs.push(DirIn {
                    id: (i + 1) as u64,
                    pos,
                    radius: child_radius,
                    parent_radius: child_radius,
                    parent: Some(0),
                    visible: true,
                    empty: false,
                });
            }

            let frame = DirFrame::from_preorder(dirs);
            let accels = dir_accels(&frame, &params, tick, 2);

            for (i, cpos) in child_positions.iter_mut().enumerate() {
                let next = integrate(*cpos, accels[i + 1]);
                last_moves[i] = (next - *cpos).length();
                *cpos = next;
            }
        }

        // Positions stop changing or oscillate within a few Q8 units:
        for (i, &move_len) in last_moves.iter().enumerate() {
            assert!(
                move_len <= 4,
                "child {i} still moving fast at tick 2000: move_len = {move_len}"
            );
        }

        // Without overlaps beyond tolerance (tolerance = 0, fully separated):
        for i in 0..6 {
            for j in (i + 1)..6 {
                let dist = (child_positions[i] - child_positions[j]).length();
                let sum_r = child_radius * 2;
                assert!(
                    dist >= sum_r,
                    "overlap between children {i} and {j}: dist = {dist}, sum_r = {sum_r}"
                );
            }
        }
    }
}
