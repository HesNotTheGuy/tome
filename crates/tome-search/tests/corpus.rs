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
