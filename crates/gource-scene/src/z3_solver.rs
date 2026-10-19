//! Z3 SMT/Optimization-backed directory tree layout solver (`--layout-engine z3`).
//!
//! Formulates directory tree compaction as a Quantifier-Free Linear Integer Arithmetic
//! (`QF_LIA`) optimization problem in Z3 (`Z3_optimize`), minimizing the weighted $L_1$
//! sum of tree edge lengths and radial distances from the root subject to:
//! - Supporting-hyperplane pairwise disc non-overlap constraints between directories.
//! - Convex ray-cone constraints on tree edges to prevent sibling edge crossings.
//! - Post-solve geometric certification via [`optimize_settled_layout`].

use crate::kernel::fixed::Fx;
#[cfg(any(test, not(target_arch = "wasm32")))]
use crate::kernel::fixed::{IVec2, ONE, mul_fx};
use crate::kernel::layout::{
    TreeDirIn, TreeDirOut, optimize_settled_layout, solve_settled_tree_initial,
};

#[cfg(not(target_arch = "wasm32"))]
mod ffi {
    use std::ffi::{c_char, c_int, c_uint, c_void};

    pub type Z3Config = *mut c_void;
    pub type Z3Context = *mut c_void;
    pub type Z3Sort = *mut c_void;
    pub type Z3Symbol = *mut c_void;
    pub type Z3Ast = *mut c_void;
    pub type Z3Params = *mut c_void;
    pub type Z3Optimize = *mut c_void;
    pub type Z3Model = *mut c_void;
    pub type Z3Lbool = c_int;

    pub const Z3_L_FALSE: Z3Lbool = -1;

    #[link(name = "z3")]
    unsafe extern "C" {
        pub fn Z3_mk_config() -> Z3Config;
        pub fn Z3_del_config(c: Z3Config);
        pub fn Z3_mk_context(c: Z3Config) -> Z3Context;
        pub fn Z3_del_context(c: Z3Context);
        pub fn Z3_mk_int_sort(c: Z3Context) -> Z3Sort;
        pub fn Z3_mk_int_symbol(c: Z3Context, i: c_int) -> Z3Symbol;
        pub fn Z3_mk_string_symbol(c: Z3Context, s: *const c_char) -> Z3Symbol;
        pub fn Z3_mk_const(c: Z3Context, s: Z3Symbol, ty: Z3Sort) -> Z3Ast;
        pub fn Z3_mk_int64(c: Z3Context, v: i64, ty: Z3Sort) -> Z3Ast;
        pub fn Z3_mk_add(c: Z3Context, num_args: c_uint, args: *const Z3Ast) -> Z3Ast;
        pub fn Z3_mk_sub(c: Z3Context, num_args: c_uint, args: *const Z3Ast) -> Z3Ast;
        pub fn Z3_mk_mul(c: Z3Context, num_args: c_uint, args: *const Z3Ast) -> Z3Ast;
        pub fn Z3_mk_ge(c: Z3Context, t1: Z3Ast, t2: Z3Ast) -> Z3Ast;
        pub fn Z3_mk_eq(c: Z3Context, l: Z3Ast, r: Z3Ast) -> Z3Ast;
        pub fn Z3_mk_params(c: Z3Context) -> Z3Params;
        pub fn Z3_params_inc_ref(c: Z3Context, p: Z3Params);
        pub fn Z3_params_dec_ref(c: Z3Context, p: Z3Params);
        pub fn Z3_params_set_uint(c: Z3Context, p: Z3Params, k: Z3Symbol, v: c_uint);
        pub fn Z3_mk_optimize(c: Z3Context) -> Z3Optimize;
        pub fn Z3_optimize_inc_ref(c: Z3Context, d: Z3Optimize);
        pub fn Z3_optimize_dec_ref(c: Z3Context, d: Z3Optimize);
        pub fn Z3_optimize_set_params(c: Z3Context, o: Z3Optimize, p: Z3Params);
        pub fn Z3_optimize_assert(c: Z3Context, o: Z3Optimize, a: Z3Ast);
        pub fn Z3_optimize_minimize(c: Z3Context, o: Z3Optimize, t: Z3Ast) -> c_uint;
        pub fn Z3_optimize_check(
            c: Z3Context,
            o: Z3Optimize,
            num_assumptions: c_uint,
            assumptions: *const Z3Ast,
        ) -> Z3Lbool;
        pub fn Z3_optimize_get_model(c: Z3Context, o: Z3Optimize) -> Z3Model;
        pub fn Z3_model_inc_ref(c: Z3Context, m: Z3Model);
        pub fn Z3_model_dec_ref(c: Z3Context, m: Z3Model);
        pub fn Z3_model_eval(
            c: Z3Context,
            m: Z3Model,
            t: Z3Ast,
            model_completion: bool,
            v: *mut Z3Ast,
        ) -> bool;
        pub fn Z3_get_numeral_int64(c: Z3Context, v: Z3Ast, i: *mut i64) -> bool;
    }

    pub unsafe fn set_optimize_timeout_ms(ctx: Z3Context, opt: Z3Optimize, timeout_ms: u32) {
        unsafe {
            let params = Z3_mk_params(ctx);
            Z3_params_inc_ref(ctx, params);
            let sym = Z3_mk_string_symbol(ctx, c"timeout".as_ptr());
            Z3_params_set_uint(ctx, params, sym, timeout_ms);
            Z3_optimize_set_params(ctx, opt, params);
            Z3_params_dec_ref(ctx, params);
        }
    }
}

/// Solve a crossing-free, non-overlapping directory layout using Z3's `QF_LIA`
/// optimizing SMT solver (`Z3_optimize`), starting from [`solve_settled_tree_initial`]
/// and finishing with [`optimize_settled_layout`].
pub fn solve_z3_tree(dirs: &[TreeDirIn], padding: Fx, min_dir_size: Fx) -> Vec<TreeDirOut> {
    let mut out = solve_settled_tree_initial(dirs, padding, min_dir_size);
    #[cfg(not(target_arch = "wasm32"))]
    {
        z3_optimize_positions(dirs, &mut out, padding, min_dir_size);
    }
    optimize_settled_layout(dirs, &mut out, padding, min_dir_size);
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn z3_optimize_positions(
    dirs: &[TreeDirIn],
    out: &mut [TreeDirOut],
    padding: Fx,
    min_dir_size: Fx,
) {
    use ffi::*;

    let n = dirs.len();
    if n <= 1 || out.len() < n {
        return;
    }
    let vis_nodes: Vec<usize> = (0..n).filter(|&i| dirs[i].visible).collect();
    if vis_nodes.len() <= 1 || vis_nodes.len() > 256 {
        return;
    }

    // Solve in Q4 coordinates (1 unit = 1/16 px = 16 Q8 units) with unit vectors scaled to U = 256.
    const SCALE_Q8: i64 = 16;
    const U: i64 = 256;

    let gap = mul_fx(4 * ONE, padding).max(3 * ONE);
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

    unsafe {
        let cfg = Z3_mk_config();
        let ctx = Z3_mk_context(cfg);
        Z3_del_config(cfg);

        let int_sort = Z3_mk_int_sort(ctx);
        let opt = Z3_mk_optimize(ctx);
        Z3_optimize_inc_ref(ctx, opt);
        set_optimize_timeout_ms(ctx, opt, 120);

        let mk_int = |val: i64| -> Z3Ast { Z3_mk_int64(ctx, val, int_sort) };
        let mk_var = |id: i32| -> Z3Ast { Z3_mk_const(ctx, Z3_mk_int_symbol(ctx, id), int_sort) };
        let mk_add2 = |a: Z3Ast, b: Z3Ast| -> Z3Ast {
            let args = [a, b];
            Z3_mk_add(ctx, 2, args.as_ptr())
        };
        let mk_sub2 = |a: Z3Ast, b: Z3Ast| -> Z3Ast {
            let args = [a, b];
            Z3_mk_sub(ctx, 2, args.as_ptr())
        };
        let mk_mul_c = |coeff: i64, var: Z3Ast| -> Z3Ast {
            let args = [mk_int(coeff), var];
            Z3_mk_mul(ctx, 2, args.as_ptr())
        };

        // Coordinate variables (x_i, y_i) for each directory index.
        let mut x_vars = vec![std::ptr::null_mut(); n];
        let mut y_vars = vec![std::ptr::null_mut(); n];
        let mut x0 = vec![0i64; n];
        let mut y0 = vec![0i64; n];

        for &i in &vis_nodes {
            x_vars[i] = mk_var((i * 2) as i32);
            y_vars[i] = mk_var((i * 2 + 1) as i32);
            x0[i] = (out[i].target_pos.x as i64) / SCALE_Q8;
            y0[i] = (out[i].target_pos.y as i64) / SCALE_Q8;
        }

        // Pin root directory at origin (0, 0).
        let zero = mk_int(0);
        Z3_optimize_assert(ctx, opt, Z3_mk_eq(ctx, x_vars[0], zero));
        Z3_optimize_assert(ctx, opt, Z3_mk_eq(ctx, y_vars[0], zero));

        let mut obj_terms: Vec<Z3Ast> = Vec::with_capacity(vis_nodes.len() * 4);
        let mut next_sym = (n * 2 + 10) as i32;

        // 1. Tree edge ray-cone constraints + L1 edge/origin objective terms.
        for &c in &vis_nodes {
            if c == 0 {
                continue;
            }
            let Some(p) = dirs[c].parent else {
                continue;
            };
            if !dirs[p].visible {
                continue;
            }

            let dv = out[c].target_pos - out[p].target_pos;
            let Some(u_q14) = dv.unit() else {
                continue;
            };
            // Convert UNIT (16384) to U (256) by dividing by 64.
            let ux = (u_q14.x as i64) / 64;
            let uy = (u_q14.y as i64) / 64;
            let nx = -uy;
            let ny = ux;

            let dx_ast = mk_sub2(x_vars[c], x_vars[p]);
            let dy_ast = mk_sub2(y_vars[c], y_vars[p]);

            let long_ast = mk_add2(mk_mul_c(ux, dx_ast), mk_mul_c(uy, dy_ast));
            let lat_ast = mk_add2(mk_mul_c(nx, dx_ast), mk_mul_c(ny, dy_ast));

            let min_edge_q4 = ((disc_r[p] + disc_r[c] + gap) as i64 + SCALE_Q8 - 1) / SCALE_Q8;
            let init_dx = x0[c] - x0[p];
            let init_dy = y0[c] - y0[p];
            let init_long = ux * init_dx + uy * init_dy;
            let rhs_long = (min_edge_q4 * U).min(init_long);
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, long_ast, mk_int(rhs_long)));

            // Cone constraint: -long <= 3 * lat <= long (keeps edge within ~18 deg of ray).
            let lat3 = mk_mul_c(3, lat_ast);
            let neg_lat3 = mk_mul_c(-3, lat_ast);
            let init_lat3 = 3 * (nx * init_dx + ny * init_dy);
            if init_long >= init_lat3.abs() {
                Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, long_ast, lat3));
                Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, long_ast, neg_lat3));
            }

            // Auxiliary L1 variables:
            // ed_x >= |x_c - x_p|, ed_y >= |y_c - y_p|, ax >= |x_c|, ay >= |y_c|
            let ed_x = mk_var(next_sym);
            let ed_y = mk_var(next_sym + 1);
            let ax = mk_var(next_sym + 2);
            let ay = mk_var(next_sym + 3);
            next_sym += 4;

            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ed_x, dx_ast));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ed_x, mk_sub2(x_vars[p], x_vars[c])));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ed_y, dy_ast));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ed_y, mk_sub2(y_vars[p], y_vars[c])));

            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ax, x_vars[c]));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ax, mk_sub2(zero, x_vars[c])));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ay, y_vars[c]));
            Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, ay, mk_sub2(zero, y_vars[c])));

            obj_terms.push(mk_mul_c(2, ed_x));
            obj_terms.push(mk_mul_c(2, ed_y));
            obj_terms.push(ax);
            obj_terms.push(ay);
        }

        // 2. Supporting-hyperplane pairwise disc non-overlap constraints.
        for a in 0..vis_nodes.len() {
            let i = vis_nodes[a];
            for &j in &vis_nodes[(a + 1)..] {
                let d_vec = out[j].target_pos - out[i].target_pos;
                let min_sep = disc_r[i] + disc_r[j] + gap;
                // Only add hyperplane constraints for pairs within 6x their minimum separation
                // so the LP remains sparse on spider trees.
                let reach = (min_sep * 6).max(240 * ONE);
                if (d_vec.x).abs() > reach || (d_vec.y).abs() > reach {
                    continue;
                }
                let Some(w_q14) = d_vec.unit() else {
                    continue;
                };
                let wx = (w_q14.x as i64) / 64;
                let wy = (w_q14.y as i64) / 64;
                if wx == 0 && wy == 0 {
                    continue;
                }
                let min_sep_q4 = (min_sep as i64 + SCALE_Q8 - 1) / SCALE_Q8;
                let init_sep = wx * (x0[j] - x0[i]) + wy * (y0[j] - y0[i]);
                let rhs = (min_sep_q4 * U).min(init_sep);

                let sep_ast = mk_add2(
                    mk_mul_c(wx, mk_sub2(x_vars[j], x_vars[i])),
                    mk_mul_c(wy, mk_sub2(y_vars[j], y_vars[i])),
                );
                Z3_optimize_assert(ctx, opt, Z3_mk_ge(ctx, sep_ast, mk_int(rhs)));
            }
        }

        if !obj_terms.is_empty() {
            let total_obj = Z3_mk_add(ctx, obj_terms.len() as u32, obj_terms.as_ptr());
            Z3_optimize_minimize(ctx, opt, total_obj);
        }

        let status = Z3_optimize_check(ctx, opt, 0, std::ptr::null());
        if status != Z3_L_FALSE {
            let model = Z3_optimize_get_model(ctx, opt);
            if !model.is_null() {
                Z3_model_inc_ref(ctx, model);
                for &i in &vis_nodes {
                    let mut xv: Z3Ast = std::ptr::null_mut();
                    let mut yv: Z3Ast = std::ptr::null_mut();
                    let mut xi: i64 = x0[i];
                    let mut yi: i64 = y0[i];
                    if Z3_model_eval(ctx, model, x_vars[i], true, &mut xv)
                        && Z3_get_numeral_int64(ctx, xv, &mut xi)
                        && Z3_model_eval(ctx, model, y_vars[i], true, &mut yv)
                        && Z3_get_numeral_int64(ctx, yv, &mut yi)
                    {
                        out[i].target_pos = IVec2::new(
                            (xi * SCALE_Q8).clamp(i32::MIN as i64 / 2, i32::MAX as i64 / 2) as i32,
                            (yi * SCALE_Q8).clamp(i32::MIN as i64 / 2, i32::MAX as i64 / 2) as i32,
                        );
                    }
                }
                Z3_model_dec_ref(ctx, model);
            }
        }

        Z3_optimize_dec_ref(ctx, opt);
        Z3_del_context(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::layout::segments_intersect;

    #[test]
    fn z3_solver_compacts_tree_with_zero_overlaps_and_crossings() {
        let dirs = vec![
            TreeDirIn {
                parent: None,
                children: vec![1, 2, 3],
                own_file_radius: 0,
                visible_files: 0,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: vec![4, 5],
                own_file_radius: 6 * ONE,
                visible_files: 2,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: vec![6],
                own_file_radius: 8 * ONE,
                visible_files: 3,
                visible: true,
            },
            TreeDirIn {
                parent: Some(0),
                children: Vec::new(),
                own_file_radius: 5 * ONE,
                visible_files: 1,
                visible: true,
            },
            TreeDirIn {
                parent: Some(1),
                children: Vec::new(),
                own_file_radius: 6 * ONE,
                visible_files: 2,
                visible: true,
            },
            TreeDirIn {
                parent: Some(1),
                children: Vec::new(),
                own_file_radius: 6 * ONE,
                visible_files: 2,
                visible: true,
            },
            TreeDirIn {
                parent: Some(2),
                children: Vec::new(),
                own_file_radius: 7 * ONE,
                visible_files: 2,
                visible: true,
            },
        ];

        let initial = solve_settled_tree_initial(&dirs, 384, 15 * ONE);
        let z3_out = solve_z3_tree(&dirs, 384, 15 * ONE);
        assert_eq!(z3_out.len(), dirs.len());
        assert_eq!(z3_out[0].target_pos, IVec2::ZERO);

        // Total edge length under Z3 should be strictly smaller than the unoptimized initial placement.
        let edge_sum = |layout: &[TreeDirOut]| -> i64 {
            let mut sum = 0i64;
            for i in 1..dirs.len() {
                let p = dirs[i].parent.unwrap();
                sum += (layout[i].target_pos - layout[p].target_pos).length() as i64;
            }
            sum
        };
        assert!(
            edge_sum(&z3_out) < edge_sum(&initial),
            "Z3 layout should reduce total edge length: z3={} vs initial={}",
            edge_sum(&z3_out),
            edge_sum(&initial)
        );

        // Verify zero disc overlaps and zero edge crossings.
        for i in 0..dirs.len() {
            for j in (i + 1)..dirs.len() {
                let dist = (z3_out[i].target_pos - z3_out[j].target_pos).length();
                let min_sep = z3_out[i].radii.parent_radius + z3_out[j].radii.parent_radius;
                assert!(
                    dist >= min_sep,
                    "Z3 dirs {i} and {j} overlap: dist={dist} < min_sep={min_sep}"
                );
            }
        }
        for c1 in 1..dirs.len() {
            let p1 = dirs[c1].parent.unwrap();
            for c2 in (c1 + 1)..dirs.len() {
                let p2 = dirs[c2].parent.unwrap();
                if p1 == p2 || p1 == c2 || c1 == p2 {
                    continue;
                }
                assert!(!segments_intersect(
                    z3_out[p1].target_pos,
                    z3_out[c1].target_pos,
                    z3_out[p2].target_pos,
                    z3_out[c2].target_pos,
                ));
            }
        }
    }
}
