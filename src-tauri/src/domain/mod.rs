//! Pure domain logic: classification + name splitting. No I/O, no DB — ported
//! from reference/market-proxy/index.ts and reference/domain-logic/partname.ts.
//! The frontend never re-derives any of this; Rust hands it finished objects.

pub mod arcane;
pub mod classify;
pub mod coda;
pub mod mastery;
pub mod mod_rarity;
pub mod nightwave;
pub mod partname;
pub mod relic;
pub mod reward_match;
pub mod vendors;
pub mod wfm_slug;
