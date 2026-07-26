pub(crate) fn reject_trailing_argument(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), String> {
    match arguments.next() {
        Some(argument) => Err(format!("unexpected argument: {argument}")),
        None => Ok(()),
    }
}
