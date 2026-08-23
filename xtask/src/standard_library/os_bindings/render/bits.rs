pub(super) const fn bit_mask(width: u8) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::bit_mask;

    #[test]
    fn bit_masks_cover_the_selected_width() {
        assert_eq!(bit_mask(1), 1);
        assert_eq!(bit_mask(5), 31);
        assert_eq!(bit_mask(64), u64::MAX);
    }
}
