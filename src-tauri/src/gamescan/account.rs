//! Second parse pass over the SAME `inventory.php` blob the item scan fetches: it
//! extracts the Account data (profile, arsenal, resources, mastery record, intrinsics,
//! syndicates) that `map.rs` discards. Pure over the `Value` — no I/O, fully tested.
//! Tolerant in the `map.rs` style (`or_else` casing, `unwrap_or` defaults).
use super::fingerprint::entry_affinity;
use crate::domain::mastery::MasteryClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DE arsenal array name → (normalized category, affinity curve). The array name
/// equals WFCD's `productCategory`, so the categories agree with
/// `db::account::category_for`. Category and curve differ on purpose: a sentinel
/// weapon is categorised with companions but ranks like a weapon, and the same goes
/// for Archwing guns/melee against Archwing frames.
const GEAR_ARRAYS: &[(&str, &str, MasteryClass)] = &[
    ("Suits", "warframe", MasteryClass::Frame),
    ("MechSuits", "necramech", MasteryClass::Frame),
    ("LongGuns", "primary", MasteryClass::Weapon),
    ("Pistols", "secondary", MasteryClass::Weapon),
    ("Melee", "melee", MasteryClass::Weapon),
    ("SpaceSuits", "archwing", MasteryClass::Frame),
    ("SpaceGuns", "archwing", MasteryClass::Weapon),
    ("SpaceMelee", "archwing", MasteryClass::Weapon),
    ("Sentinels", "companion", MasteryClass::Frame),
    ("SentinelWeapons", "companion", MasteryClass::Weapon),
    ("KubrowPets", "companion", MasteryClass::Frame),
    ("MoaPets", "companion", MasteryClass::Frame),
    ("OperatorAmps", "amp", MasteryClass::Weapon),
    ("SpecialItems", "special", MasteryClass::Weapon),
    ("CrewShipWeapons", "railjack", MasteryClass::Weapon),
];

/// DE resource-ish array name → resource `kind`.
const RESOURCE_ARRAYS: &[(&str, &str)] = &[
    ("MiscItems", "resource"),
    ("Consumables", "consumable"),
    ("Boosters", "booster"),
    ("FusionTreasures", "fusion_treasure"),
];

/// The parsed Account snapshot (pre name/icon resolution). Mirrors nothing on the
/// frontend — it's persisted to the `account_*` tables, then read back as finished rows.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub account_id: Option<String>,
    pub profile: ProfileRaw,
    pub gear: Vec<OwnedGearRaw>,
    pub resources: Vec<OwnedStackRaw>,
    pub mastery: Vec<XpRow>,
    pub lore_scans: Vec<LoreScanRaw>,
    pub intrinsics: Vec<IntrinsicRaw>,
    pub syndicates: Vec<SyndicateRaw>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileRaw {
    pub mastery_rank: Option<i64>,
    pub equipped_glyph: Option<String>,
    pub created: Option<String>, // RFC3339, derived from the Mongo $date
    pub credits: Option<i64>,
    pub platinum: Option<i64>,
    pub regal_aya: Option<i64>,
    pub endo: Option<i64>,
    pub trades_remaining: Option<i64>,
    pub gifts_remaining: Option<i64>,
    pub nodes_completed: Option<i64>,
    pub total_missions: Option<i64>,
    pub daily_focus: Option<i64>,
    pub focus_xp: Option<i64>,
    pub login_streak: Option<i64>, // best-effort: count of login milestones reached
    pub guild_id: Option<String>,
    pub alignment: Option<String>,
    pub training_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedGearRaw {
    pub unique_name: String,
    pub category: String,
    /// Which affinity curve this array's items follow.
    pub class: MasteryClass,
    /// Affinity on the owned copy — resets to 0 on a Forma.
    pub xp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedStackRaw {
    pub unique_name: String,
    pub kind: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XpRow {
    pub unique_name: String,
    pub xp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreScanRaw {
    pub unique_name: String,
    pub scans: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntrinsicRaw {
    pub skill_key: String,
    pub rank: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyndicateRaw {
    pub tag: String,
    pub standing: i64,
    pub title: Option<String>,
}

/// Read a top-level scalar tolerant of camelCase aliasing.
fn num(json: &Value, key: &str) -> Option<i64> {
    json.get(key).and_then(|v| v.as_i64())
}
fn text(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
fn array<'a>(json: &'a Value, key: &str) -> &'a [Value] {
    json.get(key).and_then(|v| v.as_array()).map_or(&[], |a| a)
}

/// `ItemType`/`itemType` of an entry.
fn item_type(entry: &Value) -> Option<String> {
    entry
        .get("ItemType")
        .or_else(|| entry.get("itemType"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Milliseconds since epoch from a DE Mongo `{"$date":{"$numberLong":"…"}}` value.
fn mongo_ms(v: &Value) -> Option<i64> {
    let d = v.get("$date")?;
    d.get("$numberLong")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| d.as_i64())
}

/// RFC3339 string from a Mongo `$date` value, or None.
fn mongo_rfc3339(v: &Value) -> Option<String> {
    let ms = mongo_ms(v)?;
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

/// Parse the full Account snapshot from the inventory blob.
pub fn parse_account(json: &Value) -> AccountSnapshot {
    let account_id = json
        .get("AccountId")
        .or_else(|| json.get("accountId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Profile scalars.
    let focus_xp = json
        .get("FocusXP")
        .and_then(|v| v.as_object())
        .map(|o| o.values().filter_map(|v| v.as_i64()).sum::<i64>());
    let login_streak = json
        .get("LoginMilestoneRewards")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as i64);
    let guild_id = json
        .get("GuildId")
        .and_then(|g| g.get("$oid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let alignment = json.get("Alignment").and_then(|a| {
        a.get("Wisdom")
            .and_then(|v| v.as_i64())
            .map(|w| format!("Wisdom {w}"))
    });
    let created = json.get("Created").and_then(mongo_rfc3339);
    let training_date = json.get("TrainingDate").and_then(mongo_rfc3339);

    // Star chart: nodes completed at least once + total mission runs.
    let missions = array(json, "Missions");
    let nodes_completed = (!missions.is_empty()).then(|| {
        missions
            .iter()
            .filter(|m| m.get("Completes").and_then(|v| v.as_i64()).unwrap_or(0) > 0)
            .count() as i64
    });
    let total_missions = (!missions.is_empty()).then(|| {
        missions
            .iter()
            .filter_map(|m| m.get("Completes").and_then(|v| v.as_i64()))
            .sum::<i64>()
    });

    let profile = ProfileRaw {
        mastery_rank: num(json, "PlayerLevel"),
        equipped_glyph: text(json, "ActiveAvatarImageType"),
        created,
        credits: num(json, "RegularCredits"),
        platinum: num(json, "PremiumCredits"),
        regal_aya: num(json, "PrimeTokens"),
        endo: num(json, "FusionPoints"),
        trades_remaining: num(json, "TradesRemaining"),
        gifts_remaining: num(json, "GiftsRemaining"),
        nodes_completed,
        total_missions,
        daily_focus: num(json, "DailyFocus"),
        focus_xp,
        login_streak,
        guild_id,
        alignment,
        training_date,
    };

    // Arsenal: each array's entries → owned gear with the copy's own affinity.
    // Rank is derived at store time, where item_manifest supplies the rank ceiling.
    let mut gear = Vec::new();
    for (key, category, class) in GEAR_ARRAYS {
        for entry in array(json, key) {
            let Some(unique_name) = item_type(entry) else {
                continue;
            };
            gear.push(OwnedGearRaw {
                unique_name,
                category: category.to_string(),
                class: *class,
                xp: entry_affinity(entry),
            });
        }
    }

    // Resources/consumables/boosters: ItemType + ItemCount, kept unresolved.
    let mut resources = Vec::new();
    for (key, kind) in RESOURCE_ARRAYS {
        for entry in array(json, key) {
            let Some(unique_name) = item_type(entry) else {
                continue;
            };
            let count = entry
                .get("ItemCount")
                .or_else(|| entry.get("itemCount"))
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            resources.push(OwnedStackRaw {
                unique_name,
                kind: kind.to_string(),
                count,
            });
        }
    }

    // Mastery record (XPInfo).
    let mastery = array(json, "XPInfo")
        .iter()
        .filter_map(|e| {
            Some(XpRow {
                unique_name: item_type(e)?,
                xp: e.get("XP").and_then(|v| v.as_i64()).unwrap_or(0),
            })
        })
        .collect();

    // Cephalon Fragment / lore scans.
    let lore_scans = array(json, "LoreFragmentScans")
        .iter()
        .filter_map(|e| {
            Some(LoreScanRaw {
                unique_name: item_type(e)?,
                scans: e.get("Scans").and_then(|v| v.as_i64()).unwrap_or(0),
            })
        })
        .collect();

    // Intrinsics (PlayerSkills): every numeric key/value.
    let intrinsics = json
        .get("PlayerSkills")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| {
                    Some(IntrinsicRaw {
                        skill_key: k.clone(),
                        rank: v.as_i64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Syndicate standing (Affiliations).
    let syndicates = array(json, "Affiliations")
        .iter()
        .filter_map(|e| {
            Some(SyndicateRaw {
                tag: e.get("Tag").and_then(|v| v.as_str())?.to_string(),
                standing: e.get("Standing").and_then(|v| v.as_i64()).unwrap_or(0),
                title: e
                    .get("Title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        })
        .collect();

    AccountSnapshot {
        account_id,
        profile,
        gear,
        resources,
        mastery,
        lore_scans,
        intrinsics,
        syndicates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "AccountId": "abc123",
            "PlayerLevel": 22,
            "ActiveAvatarImageType": "/Lotus/Types/StoreItems/AvatarImages/SeaGlyph",
            "RegularCredits": 6779296,
            "PremiumCredits": 3387,
            "PrimeTokens": 0,
            "FusionPoints": 84190,
            "TradesRemaining": 19,
            "GiftsRemaining": 22,
            "DailyFocus": 352376,
            "FocusXP": { "AttackFocus": 100, "DefenseFocus": 50 },
            "LoginMilestoneRewards": [1, 2, 3, 4],
            "GuildId": { "$oid": "clan123" },
            "Alignment": { "Wisdom": 7, "Alignment": 1.0 },
            "Created": { "$date": { "$numberLong": "1500000000000" } },
            "Missions": [
                { "Tag": "NodeA", "Completes": 5 },
                { "Tag": "NodeB", "Completes": 0 },
                { "Tag": "NodeC", "Completes": 2 }
            ],
            "Suits": [
                { "ItemType": "/Lotus/Powersuits/Ninja/Ninja", "XP": 900000 }
            ],
            "LongGuns": [
                { "ItemType": "/Lotus/Weapons/Tenno/Rifle/Boltor", "XP": 72000 }
            ],
            "SentinelWeapons": [
                { "ItemType": "/Lotus/Types/Sentinels/SentinelWeapons/DethMachineRifle", "XP": 450000 }
            ],
            "SpaceGuns": [
                { "ItemType": "/Lotus/Weapons/Tenno/Archwing/Primary/FoldingMachineGun/ArchMachineGun", "XP": 125000 }
            ],
            "MiscItems": [
                { "ItemType": "/Lotus/Types/Items/MiscItems/Ferrite", "ItemCount": 4210 }
            ],
            "Boosters": [
                { "ItemType": "/Lotus/Types/Boosters/AffinityBooster", "ItemCount": 1 }
            ],
            "XPInfo": [
                { "ItemType": "/Lotus/Powersuits/Ninja/Ninja", "XP": 900000 }
            ],
            "LoreFragmentScans": [
                { "ItemType": "/Lotus/Types/Lore/FragmentA", "Scans": 3 }
            ],
            "PlayerSkills": { "LPP_SPACE": 9, "LPP_DRIFTER": 5, "LPS_NONNUMERIC": "x" },
            "Affiliations": [
                { "Tag": "NewLokaSyndicate", "Standing": 12000, "Title": "Acolyte" },
                { "Tag": "CephalonSudaSyndicate", "Standing": -5000 }
            ]
        })
    }

    #[test]
    fn parses_profile_scalars() {
        let s = parse_account(&sample());
        assert_eq!(s.account_id.as_deref(), Some("abc123"));
        assert_eq!(s.profile.mastery_rank, Some(22));
        assert_eq!(s.profile.platinum, Some(3387));
        assert_eq!(s.profile.endo, Some(84190));
        assert_eq!(s.profile.focus_xp, Some(150)); // 100 + 50
        assert_eq!(s.profile.login_streak, Some(4));
        assert_eq!(s.profile.guild_id.as_deref(), Some("clan123"));
        assert_eq!(s.profile.alignment.as_deref(), Some("Wisdom 7"));
        assert!(s.profile.created.as_deref().unwrap().starts_with("2017-"));
    }

    #[test]
    fn star_chart_counts() {
        let s = parse_account(&sample());
        assert_eq!(s.profile.nodes_completed, Some(2)); // NodeA + NodeC
        assert_eq!(s.profile.total_missions, Some(7)); // 5 + 0 + 2
    }

    #[test]
    fn arsenal_reads_affinity_and_class() {
        let s = parse_account(&sample());
        let frame = s.gear.iter().find(|g| g.category == "warframe").unwrap();
        assert_eq!((frame.xp, frame.class), (900_000, MasteryClass::Frame));
        let gun = s.gear.iter().find(|g| g.category == "primary").unwrap();
        assert_eq!((gun.xp, gun.class), (72_000, MasteryClass::Weapon));
        // A sentinel weapon lands in the companion CATEGORY on the weapon CURVE —
        // the whole reason class is tracked separately from category.
        let sw = s
            .gear
            .iter()
            .find(|g| g.unique_name.ends_with("DethMachineRifle"))
            .unwrap();
        assert_eq!(
            (sw.category.as_str(), sw.class),
            ("companion", MasteryClass::Weapon)
        );
        // Same for Archwing guns against Archwing frames.
        let ag = s
            .gear
            .iter()
            .find(|g| g.unique_name.ends_with("ArchMachineGun"))
            .unwrap();
        assert_eq!(
            (ag.category.as_str(), ag.class),
            ("archwing", MasteryClass::Weapon)
        );
    }

    /// Live check against a real captured blob (set WF_BLOB=/tmp/wf_blob.json). Confirms
    /// the nested field shapes (Missions/Affiliations/XPInfo/fingerprints) match reality.
    /// `cargo test -- --ignored gamescan::account::tests::live_blob --nocapture`.
    #[test]
    #[ignore]
    fn live_blob_parses_nonempty() {
        let path = std::env::var("WF_BLOB").expect("set WF_BLOB to a captured inventory.php json");
        let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let s = parse_account(&json);
        eprintln!(
            "MR={:?} plat={:?} endo={:?} credits={:?} glyph={:?}",
            s.profile.mastery_rank,
            s.profile.platinum,
            s.profile.endo,
            s.profile.credits,
            s.profile.equipped_glyph
        );
        eprintln!(
            "gear={} resources={} mastery={} intrinsics={} syndicates={} lore={} nodes_completed={:?} missions={:?}",
            s.gear.len(), s.resources.len(), s.mastery.len(), s.intrinsics.len(),
            s.syndicates.len(), s.lore_scans.len(), s.profile.nodes_completed, s.profile.total_missions
        );
        let frames = s.gear.iter().filter(|g| g.category == "warframe").count();
        eprintln!("warframes owned (scan)={frames}");
        assert!(s.profile.mastery_rank.is_some(), "PlayerLevel must parse");
        assert!(!s.gear.is_empty(), "arsenal arrays must parse to gear");
        assert!(!s.resources.is_empty(), "MiscItems must parse to resources");
        assert!(!s.syndicates.is_empty(), "Affiliations must parse");
        assert!(
            s.profile.nodes_completed.unwrap_or(0) > 0,
            "Missions must parse"
        );
    }

    #[test]
    fn resources_mastery_intrinsics_syndicates() {
        let s = parse_account(&sample());
        assert!(s
            .resources
            .iter()
            .any(|r| r.kind == "resource" && r.count == 4210));
        assert!(s.resources.iter().any(|r| r.kind == "booster"));
        assert_eq!(s.mastery.len(), 1);
        assert_eq!(s.lore_scans[0].scans, 3);
        // Only numeric PlayerSkills entries are kept.
        assert_eq!(s.intrinsics.len(), 2);
        // Syndicate without a Title still parses.
        assert_eq!(s.syndicates.len(), 2);
        assert!(s.syndicates.iter().any(|x| x.title.is_none()));
    }
}
