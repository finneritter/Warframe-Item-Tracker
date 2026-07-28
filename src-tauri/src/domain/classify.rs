//! warframe.market tag/slug → WFIT taxonomy.
//!
//! Verified counts (2026-05-30): arcanes = tag `arcane_enhancement` (166, NOT
//! `arcane` which is 0); mods = tag `mod` (~1388, full scope); sets = tag `set`;
//! warframe parts = tag `warframe`; weapon parts = tag `weapon`. Weapon PARTS
//! carry `component`/`blueprint` + `weapon`; the weapon-class tag (primary/melee)
//! appears only on the `_set` item — so classify by the `weapon` tag, not the
//! class tag. Sets are matched first so every `_set` item lands in `set`.

/// The five catalog categories. `None` = an item WFIT does not track (skip it).
pub fn category_of(tags: &[String]) -> Option<&'static str> {
    let has = |t: &str| tags.iter().any(|x| x == t);
    if has("arcane_enhancement") {
        Some("arcane")
    } else if has("mod") {
        Some("mod")
    } else if has("set") {
        Some("set")
    } else if has("warframe") {
        Some("warframe")
    } else if has("weapon") {
        Some("weapon")
    } else if has("prime") {
        // Companion / archwing prime COMPONENTS carry no gear tag at all — only
        // `sentinel` / `archwing` / `skin` (+ `blueprint`) next to `prime`, while
        // their sibling parts carry `weapon`. Dropping them cost all six prime
        // sentinel sets their blueprint (Set premium was inflated by the missing
        // part) and hid Odonata Prime / Kavasa Prime entirely. Anything still
        // tagged `prime` here is a tradeable prime part: treat it as `weapon`,
        // matching the siblings it shares a set with. Non-prime oddments
        // (necramech, hound, kubrow imprints) stay untracked.
        Some("weapon")
    } else {
        None
    }
}

/// Human part-type from the slug suffix + tags. Ported from `partTypeOf`.
pub fn part_type_of(slug: &str, tags: &[String]) -> String {
    let has = |t: &str| tags.iter().any(|x| x == t);
    if has("set") {
        return "Set".into();
    }
    if has("blueprint") || slug.ends_with("_blueprint") {
        return "Blueprint".into();
    }
    const SUFFIXES: &[(&str, &str)] = &[
        ("_systems", "Systems"),
        ("_chassis", "Chassis"),
        ("_neuroptics", "Neuroptics"),
        ("_blade", "Blade"),
        ("_blades", "Blades"),
        ("_handle", "Handle"),
        ("_grip", "Handle"),
        ("_barrel", "Barrel"),
        ("_receiver", "Receiver"),
        ("_stock", "Stock"),
        ("_string", "String"),
        ("_link", "Link"),
        ("_pouch", "Pouch"),
        ("_disc", "Disc"),
        ("_lower_limb", "Lower limb"),
        ("_upper_limb", "Upper limb"),
        ("_head", "Head"),
        ("_carapace", "Carapace"),
        ("_cerebrum", "Cerebrum"),
        ("_ornament", "Ornament"),
        ("_wings", "Wings"),
        ("_gauntlet", "Gauntlet"),
        ("_buckle", "Buckle"),
        ("_guard", "Guard"),
        ("_hilt", "Hilt"),
        ("_boot", "Boot"),
    ];
    for (suf, ty) in SUFFIXES {
        if slug.ends_with(suf) {
            return (*ty).into();
        }
    }
    if has("component") {
        return "Component".into();
    }
    "Other".into()
}

/// Derive a part's parent set slug. `mesa_prime_systems` → `mesa_prime_set`.
/// Returns `None` for set items and anything without a `_prime` segment.
pub fn derive_set_slug(slug: &str) -> Option<String> {
    if slug.ends_with("_set") {
        return None;
    }
    let idx = slug.find("_prime")?;
    let stem = &slug[..idx + "_prime".len()];
    Some(format!("{stem}_set"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn categories() {
        assert_eq!(category_of(&tags(&["arcane_enhancement"])), Some("arcane"));
        assert_eq!(category_of(&tags(&["mod"])), Some("mod"));
        assert_eq!(category_of(&tags(&["set", "warframe"])), Some("set"));
        assert_eq!(category_of(&tags(&["warframe", "prime"])), Some("warframe"));
        assert_eq!(category_of(&tags(&["weapon", "component"])), Some("weapon"));
        assert_eq!(category_of(&tags(&["sentinel"])), None);
    }

    /// Prime components whose only type tag is `sentinel`/`archwing`/`skin` must
    /// still land in the catalog — otherwise their set is short a part.
    #[test]
    fn prime_components_without_a_gear_tag_are_kept() {
        // Dethcube/Carrier/Helios/Nautilus/Shade/Wyrm Prime Blueprint
        assert_eq!(
            category_of(&tags(&["blueprint", "prime", "sentinel"])),
            Some("weapon")
        );
        // Odonata Prime Harness/Systems/Wings Blueprint
        assert_eq!(
            category_of(&tags(&["archwing", "blueprint", "component", "prime"])),
            Some("weapon")
        );
        // Kavasa Prime Kubrow Collar Blueprint / Band / Buckle
        assert_eq!(
            category_of(&tags(&["blueprint", "prime", "skin"])),
            Some("weapon")
        );
        assert_eq!(category_of(&tags(&["prime"])), Some("weapon"));
        // The real tags still win over the fallback.
        assert_eq!(
            category_of(&tags(&["prime", "sentinel", "set"])),
            Some("set")
        );
        assert_eq!(category_of(&tags(&["warframe", "prime"])), Some("warframe"));
        assert_eq!(
            category_of(&tags(&["weapon", "component", "prime"])),
            Some("weapon")
        );
        // Non-prime oddments stay untracked.
        assert_eq!(category_of(&tags(&["component", "necramech"])), None);
        assert_eq!(
            category_of(&tags(&["blueprint", "hound", "secondary"])),
            None
        );
        assert_eq!(category_of(&tags(&["imprint", "kubrow", "pet"])), None);
    }

    #[test]
    fn set_slugs() {
        assert_eq!(
            derive_set_slug("mesa_prime_systems"),
            Some("mesa_prime_set".into())
        );
        assert_eq!(
            derive_set_slug("boltor_prime_receiver"),
            Some("boltor_prime_set".into())
        );
        assert_eq!(derive_set_slug("mesa_prime_set"), None);
        assert_eq!(derive_set_slug("serration"), None);
    }
}
