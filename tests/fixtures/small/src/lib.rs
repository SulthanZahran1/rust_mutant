pub fn killed_predicate(x: i32) -> bool {
    x > 0 && x < 10
}

pub fn arithmetic(x: i32) -> i32 {
    x + 2
}

pub fn returned(x: i32) -> bool {
    return x == 42;
}

pub fn survivor(x: i32) -> bool {
    x > 0
}

pub fn not_covered(x: i32) -> bool {
    x > 0 // rust-mutant: not-covered
}

pub fn timeout_loop(n: i32) -> i32 {
    let mut i = 0;
    while i < n {
        i += 1;
    }
    i
}

pub fn statement(x: i32) -> i32 {
    let y = x;
    println!("{y}");
    y
}
