# Stress Tests PostgreSQL — QoreDB

Requêtes complexes pour tester les limites de QoreDB, basées sur le schéma `tcg_nexus`.

## Résumé des tests

| Requête | Stress test |
|---------|-------------|
| 1 | CTE récursif, `UNION ALL`, window functions (`RANK`, `PERCENT_RANK`) |
| 2 | `CROSS JOIN LATERAL`, sous-requêtes corrélées multiples, `jsonb_object_agg` |
| 3 | Multi-CTE, `LAG()`, `jsonb_agg(jsonb_build_object(...))` imbriqué, sous-requêtes dans SELECT |
| 4 | `GROUPING SETS`, `FILTER (WHERE ...)`, `EXTRACT`, `GROUPING()` |
| 5 | `generate_series`, `CROSS JOIN`, `LATERAL`, sous-requête corrélée dans LATERAL |
| 6 | Sous-requêtes scalaires JSONB massives, jointures multi-niveaux, `FILTER`, agrégations imbriquées |

---

## 1. CTE récursif + window functions — Classement ELO simulé

```sql
WITH RECURSIVE match_chain AS (
    SELECT m.id, m."playerAId", m."playerBId", m."winnerId",
           m."round", m."tournamentId", m."finishedAt",
           1 AS depth
    FROM "match" m
    WHERE m."round" = 1 AND m.status = 'finished'
    UNION ALL
    SELECT m2.id, m2."playerAId", m2."playerBId", m2."winnerId",
           m2."round", m2."tournamentId", m2."finishedAt",
           mc.depth + 1
    FROM "match" m2
    JOIN match_chain mc ON mc."winnerId" IN (m2."playerAId", m2."playerBId")
        AND mc."tournamentId" = m2."tournamentId"
        AND m2."round" = mc."round" + 1
    WHERE m2.status = 'finished'
),
player_streaks AS (
    SELECT "winnerId" AS player_id,
           "tournamentId",
           COUNT(*) AS consecutive_wins,
           MAX(depth) AS deepest_round
    FROM match_chain
    WHERE "winnerId" IS NOT NULL
    GROUP BY "winnerId", "tournamentId"
)
SELECT
    u."firstName" || ' ' || u."lastName" AS player_name,
    ps."tournamentId",
    t.name AS tournament_name,
    ps.consecutive_wins,
    ps.deepest_round,
    r.points,
    r."winRate",
    RANK() OVER (PARTITION BY ps."tournamentId" ORDER BY ps.consecutive_wins DESC, r.points DESC) AS streak_rank,
    PERCENT_RANK() OVER (ORDER BY r.points DESC) AS percentile
FROM player_streaks ps
JOIN player p ON p.id = ps.player_id
JOIN "user" u ON u.id = p."userId"
JOIN tournament t ON t.id = ps."tournamentId"
LEFT JOIN ranking r ON r."playerId" = ps.player_id AND r."tournamentId" = ps."tournamentId"
ORDER BY ps.consecutive_wins DESC, r.points DESC NULLS LAST;
```

---

## 2. Sous-requêtes corrélées + JSONB deep access — Analyse des decks meta

```sql
SELECT
    d.id AS deck_id,
    d.name AS deck_name,
    u."firstName" || ' ' || u."lastName" AS owner,
    df."type" AS format,
    card_stats.total_cards,
    card_stats.unique_cards,
    card_stats.avg_hp,
    card_stats.type_distribution,
    (SELECT COUNT(*) FROM casual_match_session cms
     WHERE (cms."playerADeckId" = d.id OR cms."playerBDeckId" = d.id)
       AND cms.status = 'FINISHED') AS casual_games,
    (SELECT COUNT(*) FROM casual_match_session cms
     WHERE cms."winnerUserId" = d."userId"
       AND (cms."playerADeckId" = d.id OR cms."playerBDeckId" = d.id)
       AND cms.status = 'FINISHED') AS casual_wins,
    CASE
        WHEN (SELECT COUNT(*) FROM casual_match_session cms
              WHERE (cms."playerADeckId" = d.id OR cms."playerBDeckId" = d.id)
                AND cms.status = 'FINISHED') > 0
        THEN ROUND(
            (SELECT COUNT(*) FROM casual_match_session cms
             WHERE cms."winnerUserId" = d."userId"
               AND (cms."playerADeckId" = d.id OR cms."playerBDeckId" = d.id)
               AND cms.status = 'FINISHED')::numeric
            /
            (SELECT COUNT(*) FROM casual_match_session cms
             WHERE (cms."playerADeckId" = d.id OR cms."playerBDeckId" = d.id)
               AND cms.status = 'FINISHED') * 100, 2)
        ELSE 0
    END AS win_rate_pct
FROM deck d
JOIN "user" u ON u.id = d."userId"
LEFT JOIN deck_format df ON df.id = d."formatId"
CROSS JOIN LATERAL (
    SELECT
        SUM(dc.qty) AS total_cards,
        COUNT(DISTINCT dc."cardId") AS unique_cards,
        AVG(pcd.hp) FILTER (WHERE pcd.hp IS NOT NULL) AS avg_hp,
        jsonb_object_agg(
            COALESCE(c.category, 'unknown'),
            cat_count
        ) AS type_distribution
    FROM deck_card dc
    JOIN card c ON c.id = dc."cardId"
    LEFT JOIN pokemon_card_details pcd ON pcd.card_id = c.id
    LEFT JOIN LATERAL (
        SELECT c.category, COUNT(*) AS cat_count
        FROM deck_card dc2
        JOIN card c2 ON c2.id = dc2."cardId"
        WHERE dc2."deckId" = d.id
        GROUP BY c2.category
    ) cat ON TRUE
    WHERE dc."deckId" = d.id
) card_stats
WHERE d."isPublic" = true
ORDER BY win_rate_pct DESC NULLS LAST
LIMIT 50;
```

---

## 3. Multi-CTE + agrégation JSONB — Dashboard marketplace complet

```sql
WITH daily_sales AS (
    SELECT
        DATE_TRUNC('day', o."createdAt") AS sale_date,
        o.currency,
        SUM(o."totalAmount") AS daily_revenue,
        COUNT(DISTINCT o.buyer_id) AS unique_buyers,
        COUNT(DISTINCT oi.listing_id) AS listings_sold
    FROM "order" o
    JOIN order_item oi ON oi.order_id = o.id
    WHERE o.status = 'completed'
      AND o."createdAt" >= NOW() - INTERVAL '90 days'
    GROUP BY DATE_TRUNC('day', o."createdAt"), o.currency
),
price_volatility AS (
    SELECT
        ph.card_id,
        c.name AS card_name,
        ps2.name AS set_name,
        ph.currency,
        AVG(ph.price) AS avg_price,
        STDDEV(ph.price) AS price_stddev,
        MIN(ph.price) AS min_price,
        MAX(ph.price) AS max_price,
        MAX(ph.price) - MIN(ph.price) AS price_spread,
        CASE WHEN AVG(ph.price) > 0
             THEN (STDDEV(ph.price) / AVG(ph.price)) * 100
             ELSE 0
        END AS coefficient_of_variation
    FROM price_history ph
    JOIN card c ON c.id = ph.card_id
    LEFT JOIN pokemon_set ps2 ON ps2.id = c."setId"
    WHERE ph."recordedAt" >= NOW() - INTERVAL '30 days'
    GROUP BY ph.card_id, c.name, ps2.name, ph.currency
    HAVING COUNT(*) >= 5
),
top_sellers AS (
    SELECT
        u.id AS seller_id,
        u."firstName" || ' ' || u."lastName" AS seller_name,
        COUNT(DISTINCT l.id) AS active_listings,
        SUM(oi."unitPrice" * oi.quantity) AS total_sold_value,
        COUNT(DISTINCT oi.order_id) AS orders_fulfilled,
        AVG(l.price) AS avg_listing_price
    FROM "user" u
    JOIN listing l ON l.seller_id = u.id AND l."deletedAt" IS NULL
    LEFT JOIN order_item oi ON oi.listing_id = l.id
    LEFT JOIN "order" o ON o.id = oi.order_id AND o.status = 'completed'
    GROUP BY u.id, u."firstName", u."lastName"
)
SELECT
    ds.sale_date,
    ds.currency,
    ds.daily_revenue,
    ds.unique_buyers,
    ds.listings_sold,
    LAG(ds.daily_revenue) OVER (PARTITION BY ds.currency ORDER BY ds.sale_date) AS prev_day_revenue,
    ROUND(
        (ds.daily_revenue - LAG(ds.daily_revenue) OVER (PARTITION BY ds.currency ORDER BY ds.sale_date))
        / NULLIF(LAG(ds.daily_revenue) OVER (PARTITION BY ds.currency ORDER BY ds.sale_date), 0) * 100,
    2) AS revenue_change_pct,
    (SELECT jsonb_agg(jsonb_build_object(
        'card', pv.card_name,
        'set', pv.set_name,
        'spread', ROUND(pv.price_spread::numeric, 2),
        'cv', ROUND(pv.coefficient_of_variation::numeric, 2)
    ))
    FROM price_volatility pv
    WHERE pv.currency = ds.currency
    ORDER BY pv.coefficient_of_variation DESC
    LIMIT 5) AS most_volatile_cards,
    (SELECT jsonb_agg(jsonb_build_object(
        'name', ts.seller_name,
        'listings', ts.active_listings,
        'sold', ROUND(ts.total_sold_value::numeric, 2)
    ))
    FROM top_sellers ts
    ORDER BY ts.total_sold_value DESC NULLS LAST
    LIMIT 3) AS top_sellers_snapshot
FROM daily_sales ds
ORDER BY ds.sale_date DESC, ds.currency;
```

---

## 4. GROUPING SETS + FILTER — Stats multi-dimensions tournois

```sql
SELECT
    COALESCE(t.name, '** ALL TOURNAMENTS **') AS tournament,
    COALESCE(t."type"::text, '** ALL TYPES **') AS tournament_type,
    COALESCE(EXTRACT(YEAR FROM t."startDate")::text, '** ALL YEARS **') AS year,
    COUNT(DISTINCT m.id) AS total_matches,
    COUNT(DISTINCT m.id) FILTER (WHERE m.status = 'finished') AS finished_matches,
    COUNT(DISTINCT m."playerAId") + COUNT(DISTINCT m."playerBId") AS unique_player_slots,
    AVG(s."damageDealt") FILTER (WHERE s."isWinner" = true) AS avg_winner_damage,
    AVG(s."damageDealt") FILTER (WHERE s."isWinner" = false) AS avg_loser_damage,
    AVG(s."cardsPlayed") AS avg_cards_played,
    SUM(s.aces) AS total_aces,
    ROUND(AVG(EXTRACT(EPOCH FROM (m."finishedAt" - m."startedAt")) / 60.0)::numeric, 1)
        FILTER (WHERE m."finishedAt" IS NOT NULL AND m."startedAt" IS NOT NULL) AS avg_match_duration_min,
    ROUND(
        COUNT(*) FILTER (WHERE m."playerAScore" > m."playerBScore")::numeric
        / NULLIF(COUNT(*) FILTER (WHERE m.status = 'finished'), 0) * 100
    , 1) AS player_a_win_pct
FROM tournament t
JOIN "match" m ON m."tournamentId" = t.id
LEFT JOIN statistics s ON s."matchId" = m.id
GROUP BY GROUPING SETS (
    (t.name, t."type", EXTRACT(YEAR FROM t."startDate")),
    (t.name, t."type"),
    (t."type", EXTRACT(YEAR FROM t."startDate")),
    (t."type"),
    ()
)
ORDER BY
    GROUPING(t.name, t."type", EXTRACT(YEAR FROM t."startDate")),
    tournament, year;
```

---

## 5. LATERAL join + génération de séries — Heatmap popularité cartes

```sql
SELECT
    gs.day::date AS date,
    ps.name AS set_name,
    serie.name AS serie_name,
    COALESCE(metrics.total_views, 0) AS views,
    COALESCE(metrics.total_searches, 0) AS searches,
    COALESCE(metrics.total_favorites, 0) AS favorites,
    COALESCE(metrics.avg_popularity, 0) AS avg_popularity_score,
    COALESCE(metrics.avg_trend, 0) AS avg_trend_score,
    COALESCE(metrics.top_card_name, 'N/A') AS top_card_of_day,
    COALESCE(metrics.card_count, 0) AS cards_tracked
FROM generate_series(
    CURRENT_DATE - INTERVAL '30 days',
    CURRENT_DATE,
    '1 day'::interval
) gs(day)
CROSS JOIN (
    SELECT DISTINCT ps.id, ps.name, ps."serieId"
    FROM pokemon_set ps
    WHERE ps."legalStandard" = true
) ps
JOIN pokemon_serie serie ON serie.id = ps."serieId"
LEFT JOIN LATERAL (
    SELECT
        SUM(cpm.views) AS total_views,
        SUM(cpm.searches) AS total_searches,
        SUM(cpm.favorites) AS total_favorites,
        AVG(cpm."popularityScore") AS avg_popularity,
        AVG(cpm."trendScore") AS avg_trend,
        COUNT(DISTINCT cpm.card_id) AS card_count,
        (SELECT c2.name
         FROM card_popularity_metrics cpm2
         JOIN card c2 ON c2.id = cpm2.card_id
         WHERE cpm2.date = gs.day::date AND c2."setId" = ps.id
         ORDER BY cpm2."popularityScore" DESC
         LIMIT 1) AS top_card_name
    FROM card_popularity_metrics cpm
    JOIN card c ON c.id = cpm.card_id AND c."setId" = ps.id
    WHERE cpm.date = gs.day::date
) metrics ON TRUE
ORDER BY gs.day DESC, metrics.avg_popularity DESC NULLS LAST;
```

---

## 6. Requête "monstre" — Full user profile avec toutes les relations

```sql
SELECT
    u.id,
    u.email,
    u."firstName",
    u."lastName",
    u.role,
    u."isPro",
    u."createdAt" AS member_since,
    -- Collections
    (SELECT jsonb_agg(jsonb_build_object(
        'name', col.name,
        'items', col.item_count,
        'estimated_value', col.total_value
    ))
    FROM (
        SELECT c.name, COUNT(ci.id) AS item_count,
               SUM(COALESCE(
                   (SELECT ph.price FROM price_history ph
                    WHERE ph.card_id = ci."pokemonCardId"
                    ORDER BY ph."recordedAt" DESC LIMIT 1), 0
               ) * ci.quantity) AS total_value
        FROM collection c
        LEFT JOIN collection_item ci ON ci."collectionId" = c.id
        WHERE c."userId" = u.id
        GROUP BY c.id, c.name
    ) col) AS collections,
    -- Decks
    (SELECT jsonb_agg(jsonb_build_object(
        'name', d.name, 'format', df."type", 'cards', d.card_count
    ))
    FROM (
        SELECT d.*, COUNT(dc.id) AS card_count
        FROM deck d
        LEFT JOIN deck_card dc ON dc."deckId" = d.id
        WHERE d."userId" = u.id
        GROUP BY d.id
    ) d
    LEFT JOIN deck_format df ON df.id = d."formatId") AS decks,
    -- Badges
    (SELECT jsonb_agg(jsonb_build_object(
        'badge', b.name, 'code', b.code, 'unlocked', ub."unlockedAt"
    ))
    FROM user_badge ub
    JOIN badge b ON b.id = ub.badge_id
    WHERE ub.user_id = u.id) AS badges,
    -- Tournament history
    (SELECT jsonb_agg(jsonb_build_object(
        'tournament', t.name,
        'status', tr.status,
        'rank', r."rank",
        'points', r.points,
        'win_rate', r."winRate"
    ))
    FROM player p
    JOIN tournament_registration tr ON tr."playerId" = p.id
    JOIN tournament t ON t.id = tr."tournamentId"
    LEFT JOIN ranking r ON r."playerId" = p.id AND r."tournamentId" = t.id
    WHERE p."userId" = u.id) AS tournament_history,
    -- Marketplace activity
    (SELECT jsonb_build_object(
        'active_listings', COUNT(*) FILTER (WHERE l."deletedAt" IS NULL AND (l."expiresAt" IS NULL OR l."expiresAt" > NOW())),
        'total_listings', COUNT(*),
        'total_revenue', SUM(oi."unitPrice" * oi.quantity),
        'avg_price', AVG(l.price)
    )
    FROM listing l
    LEFT JOIN order_item oi ON oi.listing_id = l.id
    LEFT JOIN "order" o ON o.id = oi.order_id AND o.status = 'completed'
    WHERE l.seller_id = u.id) AS marketplace_stats,
    -- Purchase history
    (SELECT jsonb_build_object(
        'total_orders', COUNT(DISTINCT o.id),
        'total_spent', SUM(o."totalAmount"),
        'last_order', MAX(o."createdAt")
    )
    FROM "order" o
    WHERE o.buyer_id = u.id AND o.status = 'completed') AS purchase_stats,
    -- Match stats
    (SELECT jsonb_build_object(
        'casual_played', COUNT(*),
        'casual_wins', COUNT(*) FILTER (WHERE cms."winnerUserId" = u.id)
    )
    FROM casual_match_session cms
    WHERE cms."playerAId" = u.id OR cms."playerBId" = u.id) AS casual_stats
FROM "user" u
WHERE u."isActive" = true
ORDER BY u."createdAt" DESC
LIMIT 20;
```
