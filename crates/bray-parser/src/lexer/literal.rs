mod numeric;
mod quoted;

pub(super) use numeric::{scan_numeric_literal, scan_tuple_element_index_token};
pub(super) use quoted::{scan_character_literal, scan_string_literal};
