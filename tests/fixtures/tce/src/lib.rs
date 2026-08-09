pub mod second;

// Five identity-fold equivalent cases: at opt-level=2, x + 0 and x - 0
// normalize to the same LLVM arithmetic.
pub fn equivalent_1(x: i32) -> i32 {
    x + 0
}
pub fn equivalent_2(x: i32) -> i32 {
    x + 0
}
// The marker makes AOR generate the sixth equivalent case as x + y -> y + x.
pub fn equivalent_commutative(x: i32, y: i32) -> i32 {
    x + y // rust-mutant:commutative
}

// Three must-differ cases in the first file.
pub fn different_1(x: i32) -> i32 {
    x + 1
}
pub fn different_2(x: i32) -> i32 {
    x + 1
}
pub fn different_3(x: i32) -> i32 {
    x + 1
}
