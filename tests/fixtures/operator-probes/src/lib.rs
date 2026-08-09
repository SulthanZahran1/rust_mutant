pub fn probe(a: i32, b: i32, p: bool, q: bool) -> bool {
    let mut total = a + b;
    total += 1;
    let compare = a < b;
    let logic = p && q;
    if logic {
        println!("{total}");
    }
    return compare && logic;
}
