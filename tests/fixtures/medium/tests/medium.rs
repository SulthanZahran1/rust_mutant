use rust_mutant_medium_fixture::{arithmetic, control, idiomatic};

#[test]
fn baseline_covers_medium_behaviors() {
    assert!(arithmetic::predicate_a(1, 0));
    assert_eq!(arithmetic::arithmetic_a(3, 4), 11);
    assert_eq!(arithmetic::arithmetic_b(8, 2), 7);

    assert_eq!(control::timeout_loop(3), 3);
    assert_eq!(control::statement_a(4), 5);
    assert_eq!(control::statement_b(4), 3);
    assert!(control::returned_a(42));
    assert!(control::returned_b(4));

    assert_eq!(idiomatic::try_probe(3).unwrap(), 3);
    assert!(idiomatic::optional_probe().is_some());
    idiomatic::unwrap_probe();
    assert_eq!(idiomatic::move_probe(), 9);
    assert_eq!(idiomatic::mut_probe(), 5);
    idiomatic::clone_probe();
    idiomatic::arc_rc_probe();
    assert_eq!(idiomatic::iterator_probe(), 6);
}
