use rust_mutant_small_fixture::{arithmetic, killed_predicate, returned, statement, survivor, timeout_loop};

#[test]
fn behaviorally_sensitive_cases_are_asserted() {
    assert!(!killed_predicate(0));
    assert!(killed_predicate(5));
    assert_eq!(arithmetic(2), 4);
    assert!(returned(42));
    assert_eq!(statement(3), 3);
}

#[test]
fn survivor_case_is_executed_without_a_behavioral_assertion() {
    let _ = survivor(0);
}

#[test]
fn timeout_case_is_finite_in_the_baseline() {
    assert_eq!(timeout_loop(2), 2);
}
