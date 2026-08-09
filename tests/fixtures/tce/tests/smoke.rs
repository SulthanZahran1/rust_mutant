#[test]
fn fixture_smoke() {
    assert_eq!(rust_mutant_tce_fixture::equivalent_1(4), 4);
    assert_eq!(rust_mutant_tce_fixture::equivalent_2(4), 4);
    assert_eq!(rust_mutant_tce_fixture::equivalent_commutative(2, 3), 5);
    assert_eq!(rust_mutant_tce_fixture::different_1(4), 5);
    assert_eq!(rust_mutant_tce_fixture::different_2(4), 5);
    assert_eq!(rust_mutant_tce_fixture::different_3(4), 5);
    assert_eq!(rust_mutant_tce_fixture::second::equivalent_4(4), 4);
    assert_eq!(rust_mutant_tce_fixture::second::equivalent_5(4), 4);
    assert_eq!(rust_mutant_tce_fixture::second::equivalent_6(4), 4);
    assert_eq!(rust_mutant_tce_fixture::second::different_4(4), 5);
    assert_eq!(rust_mutant_tce_fixture::second::different_5(4), 5);
    assert_eq!(rust_mutant_tce_fixture::second::different_6(4), 5);
}
