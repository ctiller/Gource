//! A uniform spatial hash for axis-aligned box queries.
//!
//! Query results are sorted and deduplicated, so they never depend on hash
//! map iteration order.

use crate::kernel::fixed::IVec2;
use std::collections::HashMap;

/// Items spanning more cells than this per axis go in an always-returned
/// list instead of being rasterised into the grid.
const MAX_SPAN: i64 = 16;

/// A spatial hash of boxes keyed by `u32` item ids.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    shift: u32,
    cells: HashMap<(i32, i32), Vec<u32>>,
    large: Vec<(u32, IVec2, IVec2)>,
}

impl Grid {
    /// A cell-size shift suited to boxes of the given half-extents: the
    /// power of two at or above twice the median half-extent.
    pub fn auto_shift(half_extents: impl IntoIterator<Item = i32>) -> u32 {
        let mut v: Vec<i32> = half_extents.into_iter().map(|e| e.max(1)).collect();
        if v.is_empty() {
            return 12;
        }
        let mid = v.len() / 2;
        let (_, m, _) = v.select_nth_unstable(mid);
        let size = (*m as u64 * 2).max(1);
        (64 - (size - 1).leading_zeros()).clamp(4, 30)
    }

    /// Build a grid of `(id, min, max)` boxes with cells of `1 << shift`.
    pub fn build(items: impl IntoIterator<Item = (u32, IVec2, IVec2)>, shift: u32) -> Grid {
        let mut g = Grid {
            shift,
            cells: HashMap::new(),
            large: Vec::new(),
        };
        for (id, min, max) in items {
            let (c0, c1) = (g.cell(min), g.cell(max));
            if (c1.0 as i64 - c0.0 as i64) >= MAX_SPAN || (c1.1 as i64 - c0.1 as i64) >= MAX_SPAN {
                g.large.push((id, min, max));
                continue;
            }
            for cx in c0.0..=c1.0 {
                for cy in c0.1..=c1.1 {
                    g.cells.entry((cx, cy)).or_default().push(id);
                }
            }
        }
        g
    }

    #[inline]
    fn cell(&self, p: IVec2) -> (i32, i32) {
        (p.x >> self.shift, p.y >> self.shift)
    }

    /// Ids of boxes whose cells touch `[min, max]` (a superset of the
    /// overlapping boxes), sorted and deduplicated, appended to `out` after
    /// clearing it.
    pub fn query(&self, min: IVec2, max: IVec2, out: &mut Vec<u32>) {
        out.clear();
        let (c0, c1) = (self.cell(min), self.cell(max));
        let span = (c1.0 as i64 - c0.0 as i64 + 1) * (c1.1 as i64 - c0.1 as i64 + 1);
        if span > (self.cells.len() as i64).max(1) {
            // The query covers more cells than are populated: scan them all.
            for (&(cx, cy), ids) in &self.cells {
                if cx >= c0.0 && cx <= c1.0 && cy >= c0.1 && cy <= c1.1 {
                    out.extend_from_slice(ids);
                }
            }
        } else {
            for cx in c0.0..=c1.0 {
                for cy in c0.1..=c1.1 {
                    if let Some(ids) = self.cells.get(&(cx, cy)) {
                        out.extend_from_slice(ids);
                    }
                }
            }
        }
        for &(id, lmin, lmax) in &self.large {
            if lmin.x <= max.x && lmax.x >= min.x && lmin.y <= max.y && lmax.y >= min.y {
                out.push(id);
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(id: u32, x: i32, y: i32, r: i32) -> (u32, IVec2, IVec2) {
        (id, IVec2::new(x - r, y - r), IVec2::new(x + r, y + r))
    }

    #[test]
    fn finds_neighbours_sorted() {
        let g = Grid::build(
            [
                bx(3, 0, 0, 10),
                bx(1, 15, 0, 10),
                bx(2, 1000, 1000, 10),
                bx(4, -5, -5, 100_000),
            ],
            5,
        );
        let mut out = Vec::new();
        g.query(IVec2::new(-10, -10), IVec2::new(10, 10), &mut out);
        assert!(out.contains(&3) && out.contains(&4));
        assert!(!out.contains(&2));
        assert!(out.windows(2).all(|w| w[0] < w[1]));
        // A huge query scans populated cells.
        g.query(IVec2::splat(-1 << 28), IVec2::splat(1 << 28), &mut out);
        assert_eq!(out, vec![1, 2, 3, 4]);
        // Far away: only the large box.
        g.query(IVec2::splat(50_000), IVec2::splat(50_001), &mut out);
        assert_eq!(out, vec![4]);
    }

    #[test]
    fn shift_heuristic() {
        assert_eq!(Grid::auto_shift([]), 12);
        assert_eq!(Grid::auto_shift([64, 64, 64]), 7);
        assert_eq!(Grid::auto_shift([1]), 4);
    }
}
