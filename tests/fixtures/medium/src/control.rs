pub fn not_covered_a(x: i32) -> bool {
    x > 0 && x != 10 // rust-mutant: not-covered
}

pub fn not_covered_b(x: i32) -> i32 {
    x + 9 // rust-mutant: not-covered
}

pub fn timeout_loop(n: i32) -> i32 {
    let mut i = 0;
    while i < n {
        i += 1;
    }
    i
}

pub fn statement_a(x: i32) -> i32 {
    let y = x + 1;
    println!("{y}");
    y
}

pub fn statement_b(x: i32) -> i32 {
    let y = x - 1;
    println!("{y}");
    y
}

pub fn returned_a(x: i32) -> bool {
    return x == 42;
}

pub fn returned_b(x: i32) -> bool {
    return x > 2 && x < 8;
}
