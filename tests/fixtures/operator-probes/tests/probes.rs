use rust_mutant_operator_probes::probe;

#[test]
fn probe_baseline_is_green() {
    assert!(probe(1, 2, true, true));
    assert!(!probe(2, 1, true, false));
}
