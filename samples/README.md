# Sample modules

This directory will hold example module definitions you can study to author your own. A module is a TOML file describing a named collection of Wikipedia articles, defined by one or more categories (with depth) and/or arbitrary article lists.

The first samples land alongside step 7 (module manager). Planned set:

- `science-basics.toml` — physics, chemistry, biology fundamentals
- `world-history.toml` — broad survey of historical periods and regions
- `philosophy-core.toml` — major philosophical traditions and figures
- `mathematics.toml` — pure and applied math depth 3
- `geography.toml` — countries, capitals, major regions

The module file format is documented in [`crates/tome-modules`](../crates/tome-modules) once that crate's implementation lands.
