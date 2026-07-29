//! Shared parse for DE's `UpgradeFingerprint` — a JSON string embedded in ranked
//! inventory entries (mods, arcanes, and every arsenal item) carrying the instance's
//! rank (`lvl`) and affinity (`xp`). Used by both the item scan (`map.rs`) and the
//! account scan (`account.rs`).
use serde_json::Value;

/// `(lvl, xp)` from an entry's `UpgradeFingerprint`. Missing or malformed → `(0, 0)`.
pub fn parse_fingerprint(entry: &Value) -> (i64, i64) {
    let Some(fp) = entry
        .get("UpgradeFingerprint")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
    else {
        return (0, 0);
    };
    let lvl = fp.get("lvl").and_then(|v| v.as_i64()).unwrap_or(0);
    let xp = fp.get("xp").and_then(|v| v.as_i64()).unwrap_or(0);
    (lvl, xp)
}

/// Affinity banked on one inventory entry. Gear (`Suits`, `LongGuns`, …) carries it
/// directly in `XP`; mods/arcanes/rivens carry a serialized `UpgradeFingerprint`
/// instead. Reading only the fingerprint is what pinned every gear rank at 0.
pub fn entry_affinity(entry: &Value) -> i64 {
    entry
        .get("XP")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| parse_fingerprint(entry).1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_lvl_and_xp() {
        let e = json!({ "UpgradeFingerprint": "{\"lvl\":5,\"xp\":12345}" });
        assert_eq!(parse_fingerprint(&e), (5, 12345));
    }

    #[test]
    fn gear_entries_use_the_xp_field() {
        // Frames/weapons carry affinity directly; they have no UpgradeFingerprint.
        assert_eq!(entry_affinity(&json!({ "XP": 900000 })), 900_000);
        // Mods/arcanes/rivens only have the fingerprint — fall back to it.
        assert_eq!(
            entry_affinity(&json!({ "UpgradeFingerprint": "{\"lvl\":5,\"xp\":12345}" })),
            12_345
        );
        // XP wins when both are present.
        assert_eq!(
            entry_affinity(&json!({ "XP": 7, "UpgradeFingerprint": "{\"xp\":9}" })),
            7
        );
        assert_eq!(entry_affinity(&json!({})), 0);
    }

    #[test]
    fn missing_or_garbage_is_zero() {
        assert_eq!(parse_fingerprint(&json!({})), (0, 0));
        assert_eq!(
            parse_fingerprint(&json!({ "UpgradeFingerprint": "not json" })),
            (0, 0)
        );
        // lvl present, xp absent → xp defaults to 0.
        assert_eq!(
            parse_fingerprint(&json!({ "UpgradeFingerprint": "{\"lvl\":3}" })),
            (3, 0)
        );
    }
}
