use std::panic::panic_any;

use bray_runtime_abi::CHARACTER_UNICODE_DATA_VERSION;

const _: () = assert!(
    char::UNICODE_VERSION.0 == CHARACTER_UNICODE_DATA_VERSION.0
        && char::UNICODE_VERSION.1 == CHARACTER_UNICODE_DATA_VERSION.1
        && char::UNICODE_VERSION.2 == CHARACTER_UNICODE_DATA_VERSION.2
);

#[derive(Debug)]
struct NativeCharacterInvariantFailure;

native_export! {
    pub extern "C" fn bray_runtime_character_scalar_value_v1(value: u32) -> u32 {
        value
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_from_scalar_value_v1(
        value: u32,
        scalar: *mut u32,
    ) -> u8 {
        let Some(character) = char::from_u32(value) else {
            return 0;
        };

        #[expect(
            unsafe_code,
            reason = "the private native ABI writes to a compiler-provided scalar result"
        )]
        unsafe {
            scalar.write(u32::from(character));
        }

        1
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_utf8_length_v1(value: u32) -> usize {
        native_character(value).len_utf8()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_utf8_byte_v1(value: u32, index: usize) -> u8 {
        let mut bytes = [0; 4];
        let encoded = native_character(value).encode_utf8(&mut bytes);

        encoded.as_bytes().get(index).copied().unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_alphabetic_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_alphabetic())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_numeric_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_numeric())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_character_is_whitespace_v1(value: u32) -> u8 {
        u8::from(native_character(value).is_whitespace())
    }
}

fn native_character(value: u32) -> char {
    char::from_u32(value).unwrap_or_else(|| panic_any(NativeCharacterInvariantFailure))
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::CHARACTER_UNICODE_DATA_VERSION;

    use super::{
        bray_runtime_character_from_scalar_value_v1, bray_runtime_character_is_alphabetic_v1,
        bray_runtime_character_is_numeric_v1, bray_runtime_character_is_whitespace_v1,
        bray_runtime_character_scalar_value_v1, bray_runtime_character_utf8_byte_v1,
        bray_runtime_character_utf8_length_v1,
    };

    #[test]
    fn operations_follow_unicode_scalar_semantics() {
        assert_eq!(char::UNICODE_VERSION, CHARACTER_UNICODE_DATA_VERSION);

        let character = u32::from('٣');

        assert_eq!(bray_runtime_character_scalar_value_v1(character), character);
        assert_eq!(bray_runtime_character_utf8_length_v1(character), 2);
        assert_eq!(bray_runtime_character_utf8_byte_v1(character, 0), 0xd9);
        assert_eq!(bray_runtime_character_utf8_byte_v1(character, 1), 0xa3);
        assert_eq!(bray_runtime_character_utf8_byte_v1(character, 2), 0);
        assert_eq!(bray_runtime_character_is_alphabetic_v1(character), 0);
        assert_eq!(bray_runtime_character_is_numeric_v1(character), 1);
        assert_eq!(bray_runtime_character_is_whitespace_v1(character), 0);

        assert_eq!(
            bray_runtime_character_is_whitespace_v1(u32::from('\u{2003}')),
            1
        );

        let mut scalar = 0;

        assert_eq!(
            bray_runtime_character_from_scalar_value_v1(character, &raw mut scalar),
            1
        );

        assert_eq!(scalar, character);

        assert_eq!(
            bray_runtime_character_from_scalar_value_v1(0x11_0000, &raw mut scalar),
            0
        );
    }
}
