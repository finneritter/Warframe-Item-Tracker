-- Gear rank was always 0: gamescan read it from UpgradeFingerprint.lvl, which
-- frames and weapons don't carry (mods/arcanes/rivens do). Rank now comes from the
-- entry's own XP affinity, and mastery_class records which curve the item follows —
-- frame-likes need 1000*R^2 affinity per rank and grant 200 mastery points, weapons
-- 500*R^2 and 100. The class comes from the DE array an entry was read from, so a
-- sentinel weapon (category 'companion') correctly sits on the weapon curve.
ALTER TABLE account_gear ADD COLUMN mastery_class TEXT NOT NULL DEFAULT 'weapon';

-- Existing rows predate the class. Approximate it from the item path so the
-- mastered badges are right before the user's next scan; the scan overwrites this
-- with the authoritative array-derived value.
UPDATE account_gear
   SET mastery_class = 'frame'
 WHERE category IN ('warframe', 'necramech')
    OR (category IN ('archwing', 'companion') AND unique_name LIKE '%PowerSuit%');
