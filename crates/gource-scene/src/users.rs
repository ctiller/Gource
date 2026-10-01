//! User (committer avatar) dynamics.

use crate::fixed::{Acc, Fx, IVec2, ONE, TICK_HZ};
use crate::grid::Grid;
use crate::rng::{pair_dir, random_dir, salt};

/// Tunables, already fixed point.
#[derive(Clone, Copy, Debug)]
pub struct UserParams {
    /// C++ `gGourceActionDist`, Q8 (default 50 units).
    pub action_dist: Fx,
    /// C++ `gGourceBeamDist`, Q8 (default 100 units).
    pub beam_dist: Fx,
    pub seed: u64,
}

/// One user as seen by the force pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserIn {
    /// Stable entity id (RNG key).
    pub id: u64,
    pub pos: IVec2,
    /// Half extents of the user's box (`size / 2`, `size * ratio / 2`), Q8;
    /// users interact only when their boxes overlap.
    pub half: IVec2,
    /// Desired personal space (Q8), chosen by the caller from the action
    /// state (C++ `applyForceUser`: full / 0.1 / 0.5 of personal space).
    pub personal_space: Fx,
    /// Absolute positions of the files the user is drawn to this tick (at
    /// most 3 active targets, or the next pending one).
    pub targets: Vec<IVec2>,
}

/// Each user's acceleration increment (Q8 units/s) for this tick. C++
/// `interactUsers` + `applyForceToActions`; the caller adds it to the user's
/// persistent `accel`. Integer sums make the result independent of order.
///
/// - **Personal space**, against every other user whose box overlaps (use
///   [`crate::grid::Grid`]): `v = other - me`, `dist = |v|`; if `dist == 0`
///   add one unit (`ONE`) along [`crate::rng::pair_dir`]`(seed, tick,
///   other.id, me.id, salt::USER_COINCIDENT)`; else if `dist <
///   personal_space` subtract `(personal_space - dist) * v / dist`.
/// - **Actions**, per target: `v = target - me`, `dist = |v|`; if `dist ==
///   0` add one unit along [`crate::rng::random_dir`]`(seed, tick, me.id,
///   salt::USER_ACTION)`; else if `dist < action_dist` subtract
///   `(action_dist - dist) * v / dist`; else if `dist > beam_dist` add
///   `(dist - beam_dist) * v / dist`.
pub fn user_accels(users: &[UserIn], params: &UserParams, tick: u64) -> Vec<IVec2> {
    if users.is_empty() {
        return Vec::new();
    }

    let shift = Grid::auto_shift(users.iter().flat_map(|u| [u.half.x, u.half.y]));
    let grid = Grid::build(
        users.iter().enumerate().map(|(i, u)| {
            let min = u.pos - u.half;
            let max = u.pos + u.half;
            (i as u32, min, max)
        }),
        shift,
    );

    let mut query_buf = Vec::new();
    let mut accels = Vec::with_capacity(users.len());

    for (i, me) in users.iter().enumerate() {
        let mut acc = Acc::default();
        let my_min = me.pos - me.half;
        let my_max = me.pos + me.half;

        grid.query(my_min, my_max, &mut query_buf);

        for &other_idx in &query_buf {
            let j = other_idx as usize;
            if j == i {
                continue;
            }
            let other = &users[j];

            let other_min = other.pos - other.half;
            let other_max = other.pos + other.half;

            // Strict box overlap test (AABB intersection)
            if my_min.x > other_max.x
                || my_max.x < other_min.x
                || my_min.y > other_max.y
                || my_max.y < other_min.y
            {
                continue;
            }

            let v = other.pos - me.pos;
            let dist = v.length();

            if dist == 0 {
                let dir = pair_dir(params.seed, tick, other.id, me.id, salt::USER_COINCIDENT);
                acc.add(dir.unit_times(ONE as i64));
            } else if dist < me.personal_space {
                let k = (me.personal_space - dist) as i64;
                acc.sub(v.scale(k, dist as i64));
            }
        }

        for &target in &me.targets {
            let v = target - me.pos;
            let dist = v.length();

            if dist == 0 {
                let dir = random_dir(params.seed, tick, me.id, salt::USER_ACTION);
                acc.add(dir.unit_times(ONE as i64));
            } else if dist < params.action_dist {
                let k = (params.action_dist - dist) as i64;
                acc.sub(v.scale(k, dist as i64));
            } else if dist > params.beam_dist {
                let k = (dist - params.beam_dist) as i64;
                acc.add(v.scale(k, dist as i64));
            }
        }

        accels.push(acc.get());
    }

    accels
}

/// One tick of movement (C++ `RUser::logic` tail): clamp `|accel|` to
/// `speed` (Q8 units/s), `pos += accel / TICK_HZ`, then `accel *= max(0, 1 -
/// friction / TICK_HZ)` where `friction` is Q8 per second. Returns `(pos,
/// accel)`.
pub fn move_user(pos: IVec2, accel: IVec2, speed: Fx, friction: Fx) -> (IVec2, IVec2) {
    let clamped_accel = accel.clamp_len(speed);
    let new_pos = pos.step(clamped_accel);

    let friction_per_tick = crate::fixed::div_round(friction as i64, TICK_HZ as i64);
    let factor = (ONE as i64 - friction_per_tick).max(0);
    let new_accel = clamped_accel.scale(factor, ONE as i64);

    (new_pos, new_accel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_space_push_inside_outside() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };

        // User A at (0, 0), User B at (10 * ONE, 0).
        // Personal space = 20 * ONE.
        // Box half extents = (15 * ONE, 15 * ONE), so boxes overlap (distance 10 <= 15 + 15).
        let u1 = UserIn {
            id: 1,
            pos: IVec2::new(0, 0),
            half: IVec2::new(15 * ONE, 15 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };
        let u2 = UserIn {
            id: 2,
            pos: IVec2::new(10 * ONE, 0),
            half: IVec2::new(15 * ONE, 15 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };

        let accels = user_accels(&[u1.clone(), u2.clone()], &params, 0);
        // dist = 10 * ONE < personal_space (20 * ONE).
        // v = B - A = (10 * ONE, 0).
        // u1 subtracts (20 - 10)*ONE * (10*ONE, 0) / (10*ONE) = (10 * ONE, 0)
        // so u1 accel should be (-10 * ONE, 0).
        // u2: v = A - B = (-10 * ONE, 0).
        // u2 subtracts (20 - 10)*ONE * (-10*ONE, 0) / (10*ONE) = (-10 * ONE, 0)
        // so u2 accel should be (10 * ONE, 0).
        assert_eq!(accels[0], IVec2::new(-10 * ONE, 0));
        assert_eq!(accels[1], IVec2::new(10 * ONE, 0));

        // Now move User B outside personal space but still overlapping boxes:
        // User B at (25 * ONE, 0). Half extents = 15 * ONE (overlap up to 30 * ONE).
        // dist = 25 * ONE >= personal_space (20 * ONE).
        // No personal space force applied.
        let u2_outside = UserIn {
            id: 2,
            pos: IVec2::new(25 * ONE, 0),
            half: IVec2::new(15 * ONE, 15 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };
        let accels2 = user_accels(&[u1, u2_outside], &params, 0);
        assert_eq!(accels2[0], IVec2::ZERO);
        assert_eq!(accels2[1], IVec2::ZERO);
    }

    #[test]
    fn non_overlapping_boxes_dont_interact() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };
        // Distance is 10 * ONE, personal space is 20 * ONE, but half extents are only 2 * ONE.
        // Box 1: [-2, 2], Box 2: [8, 12]. Disjoint!
        let u1 = UserIn {
            id: 1,
            pos: IVec2::new(0, 0),
            half: IVec2::new(2 * ONE, 2 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };
        let u2 = UserIn {
            id: 2,
            pos: IVec2::new(10 * ONE, 0),
            half: IVec2::new(2 * ONE, 2 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };
        let accels = user_accels(&[u1, u2], &params, 0);
        assert_eq!(accels[0], IVec2::ZERO);
        assert_eq!(accels[1], IVec2::ZERO);
    }

    #[test]
    fn coincident_users_antisymmetry() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 12345,
        };
        let u1 = UserIn {
            id: 10,
            pos: IVec2::new(5 * ONE, 5 * ONE),
            half: IVec2::new(10 * ONE, 10 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };
        let u2 = UserIn {
            id: 20,
            pos: IVec2::new(5 * ONE, 5 * ONE),
            half: IVec2::new(10 * ONE, 10 * ONE),
            personal_space: 20 * ONE,
            targets: vec![],
        };

        let accels = user_accels(&[u1, u2], &params, 1);
        // Both should have non-zero accels of length ~ONE (or exactly unit_times(ONE)),
        // and equal & opposite due to pair_dir antisymmetry.
        assert_ne!(accels[0], IVec2::ZERO);
        assert_eq!(accels[0] + accels[1], IVec2::ZERO);
    }

    #[test]
    fn action_pull_beyond_beam_dist() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };
        // User at origin, target at (150 * ONE, 0).
        // dist = 150 * ONE > beam_dist.
        // k = dist - beam_dist = 50 * ONE.
        // v = target - me = (150 * ONE, 0).
        // add (50 * ONE) * (150 * ONE, 0) / (150 * ONE) = (50 * ONE, 0).
        let u = UserIn {
            id: 1,
            pos: IVec2::new(0, 0),
            half: IVec2::new(5 * ONE, 5 * ONE),
            personal_space: 20 * ONE,
            targets: vec![IVec2::new(150 * ONE, 0)],
        };
        let accels = user_accels(&[u], &params, 0);
        assert_eq!(accels[0], IVec2::new(50 * ONE, 0));
    }

    #[test]
    fn action_push_inside_action_dist() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };
        // User at origin, target at (20 * ONE, 0).
        // dist = 20 * ONE < action_dist.
        // k = action_dist - dist = 30 * ONE.
        // subtract v.scale(30*ONE, 20*ONE) = (30 * ONE, 0).
        // accel = (-30 * ONE, 0).
        let u = UserIn {
            id: 1,
            pos: IVec2::new(0, 0),
            half: IVec2::new(5 * ONE, 5 * ONE),
            personal_space: 20 * ONE,
            targets: vec![IVec2::new(20 * ONE, 0)],
        };
        let accels = user_accels(&[u], &params, 0);
        assert_eq!(accels[0], IVec2::new(-30 * ONE, 0));
    }

    #[test]
    fn neutral_band() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };
        // Between action_dist and beam_dist (e.g. 75 * ONE).
        let u = UserIn {
            id: 1,
            pos: IVec2::new(0, 0),
            half: IVec2::new(5 * ONE, 5 * ONE),
            personal_space: 20 * ONE,
            targets: vec![IVec2::new(75 * ONE, 0)],
        };
        let accels = user_accels(std::slice::from_ref(&u), &params, 0);
        assert_eq!(accels[0], IVec2::ZERO);

        // Exactly at action_dist and beam_dist
        let u_at_action = UserIn {
            targets: vec![IVec2::new(50 * ONE, 0)],
            ..u.clone()
        };
        assert_eq!(user_accels(&[u_at_action], &params, 0)[0], IVec2::ZERO);

        let u_at_beam = UserIn {
            targets: vec![IVec2::new(100 * ONE, 0)],
            ..u
        };
        assert_eq!(user_accels(&[u_at_beam], &params, 0)[0], IVec2::ZERO);
    }

    #[test]
    fn coincident_target_uses_random_dir() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 777,
        };
        let u = UserIn {
            id: 99,
            pos: IVec2::new(10 * ONE, 20 * ONE),
            half: IVec2::new(5 * ONE, 5 * ONE),
            personal_space: 20 * ONE,
            targets: vec![IVec2::new(10 * ONE, 20 * ONE)],
        };
        let tick = 5;
        let accels = user_accels(&[u], &params, tick);
        let expected_dir = random_dir(params.seed, tick, 99, salt::USER_ACTION);
        assert_eq!(accels[0], expected_dir.unit_times(ONE as i64));
    }

    #[test]
    fn multiple_targets_sum() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 42,
        };
        // User at origin. Target 1 at (150 * ONE, 0) -> force +50 in x.
        // Target 2 at (0, 150 * ONE) -> force +50 in y.
        // Target 3 at (-20 * ONE, 0) -> dist = 20 < 50, subtract (30)*(-20,0)/20 = -(-30,0) = (+30, 0).
        let u = UserIn {
            id: 1,
            pos: IVec2::ZERO,
            half: IVec2::splat(5 * ONE),
            personal_space: 10 * ONE,
            targets: vec![
                IVec2::new(150 * ONE, 0),
                IVec2::new(0, 150 * ONE),
                IVec2::new(-20 * ONE, 0),
            ],
        };
        let accels = user_accels(&[u], &params, 0);
        assert_eq!(accels[0], IVec2::new(80 * ONE, 50 * ONE));
    }

    #[test]
    fn permutation_invariance() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 999,
        };
        let u1 = UserIn {
            id: 101,
            pos: IVec2::new(10 * ONE, 10 * ONE),
            half: IVec2::splat(15 * ONE),
            personal_space: 30 * ONE,
            targets: vec![IVec2::new(200 * ONE, 200 * ONE)],
        };
        let u2 = UserIn {
            id: 202,
            pos: IVec2::new(15 * ONE, 12 * ONE),
            half: IVec2::splat(20 * ONE),
            personal_space: 25 * ONE,
            targets: vec![IVec2::new(0, 0)],
        };
        let u3 = UserIn {
            id: 303,
            pos: IVec2::new(12 * ONE, 18 * ONE),
            half: IVec2::splat(10 * ONE),
            personal_space: 15 * ONE,
            targets: vec![IVec2::new(50 * ONE, 50 * ONE)],
        };

        let list_orig = vec![u1.clone(), u2.clone(), u3.clone()];
        let list_perm = vec![u3, u1, u2]; // indices: orig [2, 0, 1]

        let accels_orig = user_accels(&list_orig, &params, 10);
        let accels_perm = user_accels(&list_perm, &params, 10);

        assert_eq!(accels_perm[0], accels_orig[2]);
        assert_eq!(accels_perm[1], accels_orig[0]);
        assert_eq!(accels_perm[2], accels_orig[1]);
    }

    #[test]
    fn determinism() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 1234567,
        };
        let users = vec![
            UserIn {
                id: 1,
                pos: IVec2::new(100, 200),
                half: IVec2::splat(500),
                personal_space: 1000,
                targets: vec![IVec2::new(500, 600)],
            },
            UserIn {
                id: 2,
                pos: IVec2::new(100, 200),
                half: IVec2::splat(500),
                personal_space: 1000,
                targets: vec![IVec2::new(100, 200)],
            },
        ];

        let out1 = user_accels(&users, &params, 42);
        let out2 = user_accels(&users, &params, 42);
        assert_eq!(out1, out2);

        let out_diff_tick = user_accels(&users, &params, 43);
        assert_ne!(out1, out_diff_tick);
    }

    #[test]
    fn move_user_clamping() {
        // speed = 60 * ONE.
        // accel = (100 * ONE, 0).
        // Clamped accel is (60 * ONE, 0).
        // pos.step(60 * ONE) -> moves 1 * ONE.
        // friction = 0 -> factor = ONE -> accel remains 60 * ONE.
        let pos = IVec2::ZERO;
        let accel = IVec2::new(100 * ONE, 0);
        let (new_pos, new_accel) = move_user(pos, accel, 60 * ONE, 0);
        assert_eq!(new_pos, IVec2::new(ONE, 0));
        assert_eq!(new_accel, IVec2::new(60 * ONE, 0));
    }

    #[test]
    fn move_user_friction_decay() {
        let pos = IVec2::ZERO;
        let accel = IVec2::new(60 * ONE, 0);
        // friction = 30 * ONE per second.
        // TICK_HZ = 60.
        // friction / TICK_HZ = 0.5 * ONE = 128.
        // factor = ONE - 128 = 128 (0.5).
        // clamped accel = (60 * ONE, 0).
        // new accel = 60 * ONE * 128 / 256 = 30 * ONE.
        let (_new_pos, new_accel) = move_user(pos, accel, 100 * ONE, 30 * ONE);
        assert_eq!(new_accel, IVec2::new(30 * ONE, 0));
    }

    #[test]
    fn move_user_friction_zeroes_accel() {
        let pos = IVec2::ZERO;
        let accel = IVec2::new(60 * ONE, 60 * ONE);
        // friction >= TICK_HZ * ONE (e.g. 60 * ONE).
        // friction / TICK_HZ >= ONE -> factor = max(0, ONE - ...) = 0.
        let (_new_pos, new_accel) = move_user(pos, accel, 100 * ONE, 60 * ONE);
        assert_eq!(new_accel, IVec2::ZERO);

        let (_new_pos2, new_accel2) = move_user(pos, accel, 100 * ONE, 120 * ONE);
        assert_eq!(new_accel2, IVec2::ZERO);
    }

    #[test]
    fn empty_users() {
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 0,
        };
        assert!(user_accels(&[], &params, 0).is_empty());
    }

    #[test]
    fn integration_two_users_converge_to_band() {
        // Two users with one target each converge into the [action_dist, beam_dist] band within a few hundred ticks.
        let params = UserParams {
            action_dist: 50 * ONE,
            beam_dist: 100 * ONE,
            seed: 1234,
        };
        let target1 = IVec2::new(300 * ONE, 0);
        let target2 = IVec2::new(-300 * ONE, 0);

        let mut pos1 = IVec2::ZERO;
        let mut accel1 = IVec2::ZERO;

        let mut pos2 = IVec2::ZERO;
        let mut accel2 = IVec2::ZERO;

        let speed = 200 * ONE;
        let friction = 2 * ONE; // moderate friction

        for tick in 0..600 {
            let users = vec![
                UserIn {
                    id: 1,
                    pos: pos1,
                    half: IVec2::splat(5 * ONE),
                    personal_space: 10 * ONE,
                    targets: vec![target1],
                },
                UserIn {
                    id: 2,
                    pos: pos2,
                    half: IVec2::splat(5 * ONE),
                    personal_space: 10 * ONE,
                    targets: vec![target2],
                },
            ];

            let incs = user_accels(&users, &params, tick);
            accel1 += incs[0];
            accel2 += incs[1];

            let (p1, a1) = move_user(pos1, accel1, speed, friction);
            pos1 = p1;
            accel1 = a1;

            let (p2, a2) = move_user(pos2, accel2, speed, friction);
            pos2 = p2;
            accel2 = a2;
        }

        let dist1 = (target1 - pos1).length();
        let dist2 = (target2 - pos2).length();

        assert!(
            dist1 >= params.action_dist && dist1 <= params.beam_dist,
            "dist1 = {} not in [{}, {}]",
            dist1,
            params.action_dist,
            params.beam_dist
        );
        assert!(
            dist2 >= params.action_dist && dist2 <= params.beam_dist,
            "dist2 = {} not in [{}, {}]",
            dist2,
            params.action_dist,
            params.beam_dist
        );
    }
}
