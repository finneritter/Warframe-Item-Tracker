//! warframe.market profile slugs — the identifier every `/v2/.../user/...` call
//! actually takes. See `migrations/0022_wfm_account_slug.sql` for the why.
//!
//! Everything here is a *candidate generator*, never an authority: warframe.market
//! appends an undiscoverable numeric suffix when a slug collides, and the obvious
//! guess belongs to somebody else (`Deepsea_` is `deepsea-0265`; the slug `deepsea`
//! is a different real account, `-DeepSea-`). Every candidate this module produces
//! must be resolved against `GET /v2/user/<candidate>` and accepted only when the
//! profile's own `ingameName` matches what the user typed.

/// Is `s` already in warframe.market slug shape — ASCII lowercase alphanumerics
/// with single `-` separators and no leading/trailing `-`?
pub fn is_slug_shaped(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    let mut prev_dash = false;
    for c in s.chars() {
        match c {
            'a'..='z' | '0'..='9' => prev_dash = false,
            '-' if !prev_dash => prev_dash = true,
            _ => return false,
        }
    }
    true
}

/// Fold `name` the way warframe.market appears to: lowercase, every run of
/// characters outside `[a-z0-9]` collapses to one `-`, leading/trailing `-` trimmed.
/// Returns `""` when nothing survives.
///
/// Non-ASCII characters are dropped, not transliterated — what warframe.market does
/// with a non-Latin in-game name is unverified, and guessing would just produce a
/// candidate that 404s. Those users paste their profile URL instead.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for c in name.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c);
        } else {
            pending_dash = true;
        }
    }
    out
}

/// The slug out of a pasted profile URL, if the input is one. Accepts the
/// address-bar forms a user is likely to paste:
/// `https://warframe.market/profile/<slug>`, without the scheme, with a locale
/// prefix (`/en/profile/…`), with a trailing slash, and with a query/fragment.
/// Returns `None` for anything else — including item URLs and bare names.
pub fn slug_from_profile_url(input: &str) -> Option<String> {
    let s = input.trim();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let rest = s.strip_prefix("warframe.market/")?;
    // Optional locale segment: /en/profile/…, /ru/profile/…
    let rest = match rest.split_once('/') {
        Some((head, tail)) if head.len() == 2 && head.chars().all(|c| c.is_ascii_alphabetic()) => {
            tail
        }
        _ => rest,
    };
    let slug = rest.strip_prefix("profile/")?;
    let slug = slug
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug.to_string())
}

/// Ordered, de-duplicated slugs to probe for a user-typed identifier: the input
/// itself when it's already slug-shaped, then [`slugify`].
///
/// Candidates that aren't slug-shaped are never emitted — they can only 404, and
/// each probe costs a slot on the global 400 ms market throttle. (There's no
/// separate lowercase candidate for the same reason: `slugify` lowercases first,
/// so `slugify(x.to_lowercase()) == slugify(x)`.)
pub fn slug_candidates(input: &str) -> Vec<String> {
    let typed = input.trim();
    let mut out: Vec<String> = Vec::with_capacity(2);
    let mut push = |c: String| {
        if is_slug_shaped(&c) && !out.contains(&c) {
            out.push(c);
        }
    };
    push(typed.to_string());
    push(slugify(typed));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real (ingameName, slug) pairs read off live warframe.market order books on
    /// 2026-08-03. These are the cases `slugify` is inferred from.
    const OBSERVED: &[(&str, &str)] = &[
        ("Nadarejin", "nadarejin"),
        ("Ray Selby", "ray-selby"),
        ("Mr.Schmoopie", "mr-schmoopie"),
        ("Illuminatus_", "illuminatus"),
        ("Spider__Sense", "spider-sense"),
        ("-Pixxy", "pixxy"),
        ("__lGallant", "lgallant"),
        ("grabnar.luka", "grabnar-luka"),
        ("rott_60", "rott-60"),
        ("-AoD-FunwithGun", "aod-funwithgun"),
    ];

    #[test]
    fn slugify_matches_observed_profiles() {
        for (name, slug) in OBSERVED {
            assert_eq!(&slugify(name), slug, "slugify({name})");
        }
    }

    /// The load-bearing test: a colliding name gets a numeric suffix that cannot be
    /// derived, and the slug we *would* derive belongs to a DIFFERENT real account
    /// (`deepsea` is "-DeepSea-", not "Deepsea_"). This is why every candidate must
    /// be verified against the profile's own ingameName before anything is stored —
    /// do not "simplify" the resolver back into a derivation.
    #[test]
    fn slugify_cannot_derive_a_suffixed_slug() {
        assert_eq!(slugify("Deepsea_"), "deepsea");
        assert_ne!(slugify("Deepsea_"), "deepsea-0265");
        assert_eq!(slugify("---T_T---"), "t-t");
        assert_ne!(slugify("---T_T---"), "t-t-4617");
    }

    #[test]
    fn candidates_are_slug_shaped_and_deduped() {
        assert_eq!(slug_candidates("Nadarejin"), vec!["nadarejin"]);
        // Already a slug: emitted once, not twice.
        assert_eq!(slug_candidates("nadarejin"), vec!["nadarejin"]);
        assert_eq!(slug_candidates("Deepsea_"), vec!["deepsea"]);
        assert_eq!(slug_candidates(" ray selby "), vec!["ray-selby"]);
        assert!(slug_candidates("!!!").is_empty());
        assert!(slug_candidates("").is_empty());
        // A suffixed slug typed verbatim is a legitimate candidate.
        assert_eq!(slug_candidates("deepsea-0265"), vec!["deepsea-0265"]);
    }

    #[test]
    /// Non-ASCII is dropped rather than guessed at. The candidate that survives is
    /// probably wrong for such a name, which is fine — it gets verified against the
    /// profile's ingameName and rejected, and the error points at the URL paste.
    fn non_ascii_names_fall_back_to_the_ascii_skeleton() {
        assert_eq!(slugify("Ryū"), "ry");
        assert_eq!(slugify("Ryū Sakamoto"), "ry-sakamoto");
    }

    #[test]
    fn profile_urls_are_recognised() {
        for url in [
            "https://warframe.market/profile/ray-selby",
            "http://www.warframe.market/profile/ray-selby/",
            "warframe.market/profile/ray-selby",
            "https://warframe.market/en/profile/ray-selby?tab=stats",
            "  https://warframe.market/profile/ray-selby#reviews  ",
        ] {
            assert_eq!(
                slug_from_profile_url(url).as_deref(),
                Some("ray-selby"),
                "{url}"
            );
        }
        for other in [
            "https://warframe.market/items/mesa_prime_set",
            "https://warframe.market/profile/",
            "ray-selby",
            "Ray Selby",
        ] {
            assert_eq!(slug_from_profile_url(other), None, "{other}");
        }
    }

    #[test]
    fn slug_shape_rejects_the_forms_the_api_404s_on() {
        assert!(is_slug_shaped("nadarejin"));
        assert!(is_slug_shaped("deepsea-0265"));
        assert!(!is_slug_shaped("Nadarejin"));
        assert!(!is_slug_shaped("Deepsea_"));
        assert!(!is_slug_shaped("-pixxy"));
        assert!(!is_slug_shaped("pixxy-"));
        assert!(!is_slug_shaped("ray--selby"));
        assert!(!is_slug_shaped(""));
    }
}
