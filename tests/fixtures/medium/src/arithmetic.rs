pub fn predicate_a(a: i32, b: i32) -> bool {
    a > b && a < b + 10 || a == b
}

pub fn arithmetic_a(a: i32, b: i32) -> i32 {
    a + b * 2
}

pub fn arithmetic_b(a: i32, b: i32) -> i32 {
    a - b / 2
}
