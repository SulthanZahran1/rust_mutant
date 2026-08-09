pub fn not_covered_all(x: i32) -> bool {
    x > 0 && x < 10 || x == 5 // rust-mutant: not-covered
}

pub fn not_covered_arithmetic(x: i32) -> i32 {
    x + 1 // rust-mutant: not-covered
}

pub fn not_covered_statement(x: i32) -> i32 {
    println!("{x}"); // rust-mutant: not-covered
    x
}

pub fn not_covered_return(x: i32) -> bool {
    return x > 0; // rust-mutant: not-covered
}

pub fn invalid_try(value: i32) -> Result<i32, &'static str> {
    let result = if value >= 0 { Ok(value) } else { Err("negative") };
    let unwrapped = result?;
    Ok(unwrapped)
}

pub fn timeout_loop(n: f64) -> f64 {
    let mut i = 0.0;
    while i < n {
        i += 1.0;
    }
    i
}
