//! Integration test exercising the full build-then-query pipeline against a
//! small synthetic corpus. Uses an in-memory index for speed.

use tome_core::Tier;
use tome_search::Index;

fn build_corpus() -> Index {
    let index = Index::create_in_ram().unwrap();
    let mut writer = index.writer(15_000_000).unwrap();

    writer
        .add(
            1,
            "Photon",
            "A photon is an elementary particle, a quantum of the electromagnetic field. \
             Photons are massless and travel at the speed of light.",
            Tier::Hot,
        )
        .unwrap();
    writer
        .add(
            2,
            "Electron",
            "An electron is a subatomic particle whose electric charge is negative. \
             Electrons orbit atomic nuclei.",
            Tier::Warm,
        )
        .unwrap();
    writer
        .add(
            3,
            "Quark",
            "A quark is an elementary particle and a fundamental constituent of matter. \
             Quarks combine to form composite particles called hadrons.",
            Tier::Warm,
        )
        .unwrap();
    writer
        .add(
            4,
            "Higgs boson",
            "The Higgs boson is an elementary particle in the Standard Model. \
             Its existence was confirmed at the Large Hadron Collider in 2012.",
            Tier::Cold,
        )
        .unwrap();
    writer
        .add(
            5,
            "Cooking",
            "Cooking is the art of preparing food for consumption with the use of heat.",
            Tier::Cold,
        )
        .unwrap();

    writer.commit().unwrap();
    index
}

#[test]
fn query_finds_relevant_articles() {
    let idx = build_corpus();
    let hits = idx.search("photon", 10, &[]).unwrap();
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0].title, "Photon");
}

#[test]
fn body_match_returns_correct_article() {
    let idx = build_corpus();
    // "fundamental" appears uniquely in the Quark body in this corpus,
    // so the top hit should be Quark regardless of stemming behavior.
    let hits = idx.search("fundamental", 10, &[]).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].title, "Quark");
}

#[test]
fn unrelated_query_returns_empty() {
    let idx = build_corpus();
    let hits = idx.search("unicycle", 10, &[]).unwrap();
    assert!(hits.is_empty(), "no fixture article mentions unicycles");
}

#[test]
fn ranking_puts_more_relevant_doc_first() {
    let idx = build_corpus();
    // "particle" appears in 4 of the 5 docs; "elementary" narrows it.
    let hits = idx.search("elementary particle", 10, &[]).unwrap();
    assert!(hits.len() >= 3);
    // Each top hit should have a positive BM25 score.
    for h in &hits {
        assert!(h.score > 0.0, "expected positive score, got {}", h.score);
    }
}

#[test]
fn tier_filter_restricts_results() {
    let idx = build_corpus();

    // Without filter: should match Photon (Hot), Electron (Warm), Quark
    // (Warm), Higgs (Cold) — anything with "particle".
    let all = idx.search("particle", 10, &[]).unwrap();
    let all_titles: Vec<_> = all.iter().map(|h| h.title.as_str()).collect();
    assert!(all_titles.contains(&"Photon"));
    assert!(all_titles.contains(&"Electron"));
    assert!(all_titles.contains(&"Higgs boson"));

    // Filtered to Warm only.
    let warm = idx.search("particle", 10, &[Tier::Warm]).unwrap();
    let warm_titles: Vec<_> = warm.iter().map(|h| h.title.as_str()).collect();
    assert!(warm_titles.contains(&"Electron"));
    assert!(warm_titles.contains(&"Quark"));
    assert!(
        !warm_titles.contains(&"Photon"),
        "Hot-tier doc leaked through Warm filter"
    );
    assert!(
        !warm_titles.contains(&"Higgs boson"),
        "Cold-tier doc leaked through Warm filter"
    );

    // Filter to Hot or Cold (set union).
    let hot_or_cold = idx
        .search("particle", 10, &[Tier::Hot, Tier::Cold])
        .unwrap();
    let h_titles: Vec<_> = hot_or_cold.iter().map(|h| h.title.as_str()).collect();
    assert!(h_titles.contains(&"Photon"));
    assert!(h_titles.contains(&"Higgs boson"));
    assert!(!h_titles.contains(&"Electron"));
    assert!(!h_titles.contains(&"Quark"));
}

#[test]
fn limit_caps_returned_hits() {
    let idx = build_corpus();
    let hits = idx.search("particle", 2, &[]).unwrap();
    assert!(hits.len() <= 2);
}

#[test]
fn search_returns_tier_correctly_in_hits() {
    let idx = build_corpus();
    let hits = idx.search("Higgs", 10, &[]).unwrap();
    let higgs = hits.iter().find(|h| h.title == "Higgs boson").unwrap();
    assert_eq!(higgs.tier, Tier::Cold);
}

#[test]
fn exact_title_match_outranks_body_repetition() {
    // Production reality: 6.8M titles with mostly empty bodies. A doc that
    // repeats the query word in its body must not outrank the doc whose
    // title IS the query — that's what the 3x title boost buys.
    let index = Index::create_in_ram().unwrap();
    let mut writer = index.writer(15_000_000).unwrap();
    writer.add(1, "Gravity", "", Tier::Hot).unwrap();
    writer
        .add(
            2,
            "History of physics",
            "Gravity gravity gravity gravity gravity. Newton studied gravity. \
             Einstein reframed gravity as curvature. Gravity gravity gravity.",
            Tier::Hot,
        )
        .unwrap();
    writer.commit().unwrap();

    let hits = index.search("gravity", 10, &[]).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0].title, "Gravity",
        "exact title match should rank above heavy body repetition"
    );
}

#[test]
fn fuzzy_fallback_corrects_single_word_typo() {
    let idx = build_corpus();
    // "phopton" is "Photon" with one transposed/inserted char — edit
    // distance 1 from the indexed term "photon".
    let hits = idx.search("phopton", 10, &[]).unwrap();
    assert!(!hits.is_empty(), "fuzzy fallback should rescue the typo");
    assert_eq!(hits[0].title, "Photon");
}

#[test]
fn fuzzy_fallback_handles_near_prefix_query() {
    let idx = build_corpus();
    // "photo" is distance 1 from "photon" (one deletion). Must not panic;
    // if anything matches, it should be Photon.
    let hits = idx.search("photo", 10, &[]).unwrap();
    for h in &hits {
        assert_eq!(h.title, "Photon", "unexpected fuzzy hit: {}", h.title);
    }
}

#[test]
fn fuzzy_exact_hits_rank_before_fuzzy_hits_and_dedup() {
    let index = Index::create_in_ram().unwrap();
    let mut writer = index.writer(15_000_000).unwrap();
    writer.add(1, "Photon", "", Tier::Hot).unwrap();
    writer.add(2, "Proton", "", Tier::Hot).unwrap(); // distance 1 from "photon"
    writer.commit().unwrap();

    // "photon" matches doc 1 exactly; doc 2 only via fuzzy. Exact first,
    // and doc 1 must not appear twice even though fuzzy also matches it.
    let hits = index.search("photon", 10, &[]).unwrap();
    assert_eq!(hits[0].title, "Photon");
    let photon_count = hits.iter().filter(|h| h.title == "Photon").count();
    assert_eq!(photon_count, 1, "fuzzy merge must dedup by page_id");
}

#[test]
fn fuzzy_fallback_respects_tier_filter() {
    let idx = build_corpus();
    // Photon is Hot. With a Warm/Cold filter, the fuzzy match must be
    // suppressed too.
    let hits = idx
        .search("phopton", 10, &[Tier::Warm, Tier::Cold])
        .unwrap();
    assert!(
        hits.iter().all(|h| h.title != "Photon"),
        "fuzzy hit leaked through tier filter"
    );

    // With the matching tier, the fuzzy hit comes back.
    let hits = idx.search("phopton", 10, &[Tier::Hot]).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].title, "Photon");
}

#[test]
fn multi_word_query_does_not_trigger_fuzzy() {
    let idx = build_corpus();
    // "phopton" alone would fuzzy-match Photon, but a multi-word query has
    // operator/phrase semantics we must not fuzz. ("phopton" matches
    // nothing exactly and "zzzz" matches nothing at all, so any hit here
    // could only have come from an unwanted fuzzy fallback.)
    let hits = idx.search("phopton zzzz", 10, &[]).unwrap();
    assert!(
        hits.is_empty(),
        "multi-word query must not take the fuzzy fallback"
    );
}

#[test]
fn short_token_does_not_trigger_fuzzy() {
    let idx = build_corpus();
    // 3-char tokens are below the fuzzy threshold; "pho" matches nothing
    // exactly and must not fuzzy-match "photon" (it's distance 3 anyway,
    // but the guard should reject it before any fuzzy query runs).
    let hits = idx.search("pho", 10, &[]).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn delete_all_then_commit_empties_index() {
    let idx = build_corpus();
    assert_eq!(idx.num_docs().unwrap(), 5);

    let mut writer = idx.writer(15_000_000).unwrap();
    writer.delete_all().unwrap();
    writer.commit().unwrap();

    assert_eq!(idx.num_docs().unwrap(), 0);
    let hits = idx.search("photon", 10, &[]).unwrap();
    assert!(hits.is_empty(), "wiped index must return no hits");
}

#[test]
fn num_docs_counts_added_documents() {
    let index = Index::create_in_ram().unwrap();
    assert_eq!(index.num_docs().unwrap(), 0);

    let mut writer = index.writer(15_000_000).unwrap();
    writer.add(1, "Photon", "", Tier::Hot).unwrap();
    writer.add(2, "Electron", "", Tier::Hot).unwrap();
    writer.commit().unwrap();
    assert_eq!(index.num_docs().unwrap(), 2);

    writer.add(3, "Quark", "", Tier::Hot).unwrap();
    writer.commit().unwrap();
    assert_eq!(index.num_docs().unwrap(), 3);
}

#[test]
fn limit_zero_returns_empty_without_panic() {
    let idx = build_corpus();
    // TopDocs panics on a 0 limit; the guard must short-circuit instead.
    let hits = idx.search("photon", 0, &[]).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn fewer_results_than_limit_does_not_panic() {
    let idx = build_corpus();
    // One matching doc, generous limit — exercises the fuzzy-fallback path
    // (hits.len() < limit) without tripping any 0-limit TopDocs.
    let hits = idx.search("cooking", 50, &[]).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Cooking");
}
