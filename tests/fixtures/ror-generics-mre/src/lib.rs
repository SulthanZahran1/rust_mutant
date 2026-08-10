#[derive(Debug, PartialEq, Eq)]
pub struct DriftEntry;

pub struct MigrationPlan {
    pub drift: Vec<DriftEntry>,
    pub optional: Option<DriftEntry>,
}

pub fn is_less(left: usize, right: usize) -> bool {
    left < right
}

#[cfg(test)]
mod tests {
    use super::{is_less, DriftEntry, MigrationPlan};

    #[test]
    fn baseline_exercises_generic_and_relational_syntax() {
        let plan = MigrationPlan {
            drift: vec![DriftEntry],
            optional: Some(DriftEntry),
        };
        assert_eq!(plan.drift.len(), 1);
        assert!(is_less(1, 2));
    }
}
