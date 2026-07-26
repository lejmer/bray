pub(super) fn try_count_comparison(comparisons: &mut u64, maximum: u64) -> bool {
    if *comparisons >= maximum {
        return false;
    }

    *comparisons += 1;

    true
}

#[cfg(test)]
mod tests {
    use super::try_count_comparison;

    #[test]
    fn comparison_counting_accepts_only_the_configured_number() {
        let mut comparisons = 0;

        assert!(try_count_comparison(&mut comparisons, 2));
        assert!(try_count_comparison(&mut comparisons, 2));
        assert!(!try_count_comparison(&mut comparisons, 2));
        assert_eq!(comparisons, 2);
    }
}
