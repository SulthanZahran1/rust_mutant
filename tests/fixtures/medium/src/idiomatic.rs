pub fn try_probe(value: i32) -> Result<i32, &'static str> {
    let result = if value >= 0 { Ok(value) } else { Err("negative") };
    let unwrapped = result?;
    Ok(unwrapped)
}

pub fn optional_probe() -> Option<()> {
    Some(())?;
    Some(())
}

pub fn unwrap_probe() {
    Some(3_i32).unwrap();
    Some(4_i32).expect("present");
}

pub async fn await_probe() {
    async { 7_i32 }.await;
}

pub fn move_probe() -> i32 {
    let closure = move || 9_i32;
    closure()
}

pub fn mut_probe() -> i32 {
    let mut value = 5_i32;
    let _reference = &mut value;
    value
}

pub fn clone_probe() {
    let _copy = 11_i32.clone();
}

pub fn arc_rc_probe() {
    let _arc = std::sync::Arc::new(1_i32);
    let _rc = std::rc::Rc::new(2_i32);
}

pub fn iterator_probe() -> usize {
    let mapped = vec![1_i32, 2, 3].into_iter().map(|_| true).collect::<Vec<_>>();
    let filtered = vec![1_i32, 2, 3]
        .into_iter()
        .filter(|_| true)
        .collect::<Vec<_>>();
    mapped.len() + filtered.len()
}
