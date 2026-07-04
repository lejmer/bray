use bray_source::TextSize;

pub(super) fn text_size_to_usize(size: TextSize) -> usize {
    match usize::try_from(size.bytes()) {
        Ok(size) => size,
        Err(_) => panic!("TextSize did not fit in usize on this target"),
    }
}

pub(super) fn text_size_from_usize(size: usize) -> TextSize {
    match TextSize::try_from(size) {
        Ok(size) => size,
        Err(error) => panic!("source offset should fit in TextSize: {error:?}"),
    }
}
