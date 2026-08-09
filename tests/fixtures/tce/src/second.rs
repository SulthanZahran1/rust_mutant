// The second source file proves that TCE compares a multi-file Cargo crate,
// not only the file that received the mutation.
pub fn equivalent_4(x: i32) -> i32 {
    x + 0
}
pub fn equivalent_5(x: i32) -> i32 {
    x + 0
}
pub fn equivalent_6(x: i32) -> i32 {
    x + 0
}
pub fn different_4(x: i32) -> i32 {
    x + 1
}
pub fn different_5(x: i32) -> i32 {
    x + 1
}
pub fn different_6(x: i32) -> i32 {
    x + 1
}
