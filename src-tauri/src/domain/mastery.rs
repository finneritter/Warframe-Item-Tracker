//! Pure mastery math: cumulative-affinity → Mastery Rank, per-item max rank / mastered
//! state, and (approximate) mastery-point contributions. No I/O. Fed by the scanned
//! `XPInfo` + `item_manifest.max_rank`.
//!
//! DE's thresholds: Mastery Ranks 1..=30 need `2500 * rank^2` cumulative mastery
//! POINTS; Legendary ranks (31+) add `147_500` each on top of MR30's `2_250_000`.
//! Points per item rank — and the per-item affinity curve that sets that rank — follow
//! the item's [`MasteryClass`], not its category.

/// Cumulative mastery POINTS required to reach Mastery Rank `rank`.
pub fn xp_threshold(rank: i64) -> i64 {
    if rank <= 0 {
        0
    } else if rank <= 30 {
        2_500 * rank * rank
    } else {
        2_250_000 + 147_500 * (rank - 30)
    }
}

/// Mastery Rank from total accumulated mastery points: the highest rank whose
/// threshold is met.
pub fn mr_from_total_xp(total: i64) -> i64 {
    if total < xp_threshold(1) {
        return 0;
    }
    // Walk up while the next threshold is still covered. Bounded (MR is small).
    let mut rank = 0;
    while xp_threshold(rank + 1) <= total {
        rank += 1;
        if rank > 5000 {
            break; // defensive: never loop unbounded on absurd input
        }
    }
    rank
}

/// MR progress as `(current_rank, affinity_into_current, affinity_needed_for_next)`.
pub fn mr_progress(total: i64) -> (i64, i64, i64) {
    let current = mr_from_total_xp(total);
    let base = xp_threshold(current);
    let next = xp_threshold(current + 1);
    (current, (total - base).max(0), (next - base).max(1))
}

/// A gear item's max rank: the manifest value when known, else 30 (the common cap).
pub fn gear_max_rank(manifest_max: Option<i64>) -> i64 {
    manifest_max.unwrap_or(30)
}

/// Which affinity curve an item follows. Frame-likes (Warframes, Necramechs,
/// Archwing frames, sentinels/beasts/Moas) need 1000×R² total affinity to reach
/// rank R and grant 200 mastery points per rank; every weapon — including Archwing
/// guns/melee and sentinel weapons — needs 500×R² and grants 100. The DE inventory
/// array an entry came from names the class exactly, so it is never guessed from
/// the item path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasteryClass {
    Frame,
    Weapon,
}

impl MasteryClass {
    /// The `account_gear.mastery_class` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Weapon => "weapon",
        }
    }

    /// Read back from the DB. Unknown text falls back to `Weapon`, the cheaper
    /// curve — an under-reported rank beats a fabricated one.
    pub fn from_db(s: &str) -> Self {
        if s == "frame" {
            Self::Frame
        } else {
            Self::Weapon
        }
    }

    fn affinity_coefficient(self) -> i64 {
        match self {
            Self::Frame => 1_000,
            Self::Weapon => 500,
        }
    }

    pub fn points_per_rank(self) -> i64 {
        match self {
            Self::Frame => 200,
            Self::Weapon => 100,
        }
    }
}

/// Cumulative affinity required to REACH `rank` on one item.
pub fn affinity_threshold(rank: i64, class: MasteryClass) -> i64 {
    if rank <= 0 {
        0
    } else {
        class.affinity_coefficient() * rank * rank
    }
}

/// An owned copy's rank from the affinity banked on it, capped at the item's
/// ceiling. The inverse of `affinity_threshold`.
pub fn rank_from_affinity(affinity: i64, class: MasteryClass, max_rank: i64) -> i64 {
    if affinity <= 0 || max_rank <= 0 {
        return 0;
    }
    // The largest r with coefficient×r² <= affinity. Seeded by a float sqrt and then
    // corrected in whole steps, so a rank boundary can never land on the wrong side
    // of a rounding error. (`i64::isqrt` says this in one call but needs a newer
    // toolchain than the crate's declared MSRV.)
    let target = affinity / class.affinity_coefficient();
    let mut rank = (target as f64).sqrt() as i64;
    while rank > 0 && rank * rank > target {
        rank -= 1;
    }
    while (rank + 1) * (rank + 1) <= target {
        rank += 1;
    }
    rank.min(max_rank)
}

/// Whether an item counts as mastered. Takes LIFETIME affinity (the XPInfo ledger),
/// not the current copy's — mastery survives a Forma, the copy's rank does not.
pub fn is_mastered(lifetime_affinity: i64, class: MasteryClass, max_rank: i64) -> bool {
    max_rank > 0 && lifetime_affinity >= affinity_threshold(max_rank, class)
}

/// Approximate mastery points one item contributes at `rank`.
pub fn mastery_points(class: MasteryClass, rank: i64) -> i64 {
    class.points_per_rank() * rank.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mr_thresholds_at_boundaries() {
        assert_eq!(mr_from_total_xp(0), 0);
        assert_eq!(mr_from_total_xp(2_499), 0);
        assert_eq!(mr_from_total_xp(2_500), 1); // 2500 * 1^2
        assert_eq!(mr_from_total_xp(9_999), 1);
        assert_eq!(mr_from_total_xp(10_000), 2); // 2500 * 2^2
        assert_eq!(mr_from_total_xp(2_250_000), 30); // 2500 * 30^2
        assert_eq!(mr_from_total_xp(2_250_000 - 1), 29);
        assert_eq!(mr_from_total_xp(2_250_000 + 147_500), 31); // first Legendary
    }

    #[test]
    fn progress_splits_into_next() {
        // Exactly at MR2: 0 into current, full bar to MR3.
        let (cur, into, needed) = mr_progress(10_000);
        assert_eq!(cur, 2);
        assert_eq!(into, 0);
        assert_eq!(needed, xp_threshold(3) - xp_threshold(2));
    }

    #[test]
    fn affinity_curve_matches_the_game() {
        // Total affinity to REACH rank R: 1000*R^2 frame-likes, 500*R^2 weapons.
        assert_eq!(affinity_threshold(30, MasteryClass::Frame), 900_000);
        assert_eq!(affinity_threshold(30, MasteryClass::Weapon), 450_000);
        assert_eq!(affinity_threshold(40, MasteryClass::Weapon), 800_000);
        assert_eq!(affinity_threshold(0, MasteryClass::Frame), 0);
        assert_eq!(affinity_threshold(-3, MasteryClass::Weapon), 0);
    }

    #[test]
    fn rank_from_affinity_is_the_inverse_and_caps() {
        assert_eq!(rank_from_affinity(0, MasteryClass::Weapon, 30), 0);
        assert_eq!(rank_from_affinity(499, MasteryClass::Weapon, 30), 0);
        assert_eq!(rank_from_affinity(500, MasteryClass::Weapon, 30), 1);
        assert_eq!(rank_from_affinity(449_999, MasteryClass::Weapon, 30), 29);
        assert_eq!(rank_from_affinity(450_000, MasteryClass::Weapon, 30), 30);
        assert_eq!(rank_from_affinity(899_999, MasteryClass::Frame, 30), 29);
        assert_eq!(rank_from_affinity(900_000, MasteryClass::Frame, 30), 30);
        // A Kuva weapon keeps ranking past 30.
        assert_eq!(rank_from_affinity(578_000, MasteryClass::Weapon, 40), 34);
        // Never past the item's ceiling, however much affinity it has banked.
        assert_eq!(rank_from_affinity(378_075_187, MasteryClass::Frame, 30), 30);
        // Nonsense inputs stay at 0 rather than panicking.
        assert_eq!(rank_from_affinity(-1, MasteryClass::Weapon, 30), 0);
        assert_eq!(rank_from_affinity(1_000_000, MasteryClass::Weapon, 0), 0);
    }

    #[test]
    fn mastered_uses_lifetime_affinity_not_the_current_copy() {
        // A Forma'd frame reads rank 0 but stays mastered: lifetime affinity is the test.
        assert!(is_mastered(900_000, MasteryClass::Frame, 30));
        assert!(is_mastered(378_075_187, MasteryClass::Frame, 30));
        assert!(!is_mastered(899_999, MasteryClass::Frame, 30));
        assert!(is_mastered(450_000, MasteryClass::Weapon, 30));
        assert!(!is_mastered(450_000, MasteryClass::Weapon, 40));
        assert!(!is_mastered(1_000_000, MasteryClass::Weapon, 0));
    }

    #[test]
    fn points_follow_the_class_not_the_category() {
        // A sentinel WEAPON is a weapon (100/rank) even though its category is "companion".
        assert_eq!(mastery_points(MasteryClass::Weapon, 30), 3_000);
        assert_eq!(mastery_points(MasteryClass::Frame, 30), 6_000);
        assert_eq!(mastery_points(MasteryClass::Frame, -2), 0);
        assert_eq!(MasteryClass::from_db("frame"), MasteryClass::Frame);
        assert_eq!(MasteryClass::from_db("weapon"), MasteryClass::Weapon);
        assert_eq!(MasteryClass::from_db("nonsense"), MasteryClass::Weapon);
        assert_eq!(MasteryClass::Frame.as_str(), "frame");
        assert_eq!(gear_max_rank(None), 30);
        assert_eq!(gear_max_rank(Some(40)), 40);
    }
}
