use super::*;

#[test]
fn every_case_is_deterministic_and_declared_categories_are_covered() {
    let first = run_all();
    let second = run_all();
    assert_eq!(
        first.to_report_lines(),
        second.to_report_lines(),
        "running the eval set twice must produce identical results"
    );

    let failing: Vec<_> = first
        .to_report_lines()
        .into_iter()
        .zip(first.results.iter())
        .filter(|(_, (_, _, ok, _))| !ok)
        .map(|(line, _)| line)
        .collect();
    assert!(failing.is_empty(), "eval failures: {failing:#?}");

    let covered = first.categories_covered();
    for category in EvalCategory::ALL {
        assert!(
            covered.contains(&category),
            "missing eval coverage for category {:?}",
            category
        );
    }
    assert!(first.total() >= EvalCategory::ALL.len() * 2);
}

#[test]
fn summary_counts_are_consistent() {
    let summary = run_all();
    assert_eq!(summary.total(), summary.results.len());
    assert_eq!(summary.passed(), summary.total());
    assert!(summary.all_passed());
}
