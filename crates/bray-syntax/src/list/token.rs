macro_rules! define_token_list_syntax {
    (
        list {
            $(#[$list_meta:meta])*
            $list_visibility:vis struct $list_syntax:ident {
                builder: $list_builder_syntax:ident,
                kind: $list_kind:path,
                items: $items_method:ident,
                source_slot: $list_source_slot:literal,
                range_description: $list_range_description:literal,
                debug_name: $list_debug_name:literal,
                builder_debug_name: $list_builder_debug_name:literal $(,)?
            }
        }
        item {
            $(#[$item_meta:meta])*
            $item_visibility:vis struct $item_syntax:ident {
                builder: $item_builder_syntax:ident,
                kind: $item_kind:path,
                token_kind: $token_kind:path,
                token: $token_method:ident,
                push_token: $push_token_method:ident,
                source_slot: $item_source_slot:literal,
                token_slot: $token_slot:literal,
                range_description: $item_range_description:literal,
                debug_name: $item_debug_name:literal,
                builder_debug_name: $item_builder_debug_name:literal,
                missing_token_panic: $missing_token_panic:literal $(,)?
            }
        }
        separator: {
            kind: $separator_kind:path $(,)?
        } $(,)?
    ) => {
        $crate::list::define_token_item_syntax! {
            $(#[$item_meta])*
            $item_visibility struct $item_syntax {
                builder: $item_builder_syntax,
                kind: $item_kind,
                token_kind: $token_kind,
                token: $token_method,
                push_token: $push_token_method,
                source_slot: $item_source_slot,
                token_slot: $token_slot,
                range_description: $item_range_description,
                debug_name: $item_debug_name,
                builder_debug_name: $item_builder_debug_name,
                missing_token_panic: $missing_token_panic,
            }
        }

        $crate::list::define_list_syntax! {
            $(#[$list_meta])*
            $list_visibility struct $list_syntax {
                builder: $list_builder_syntax,
                item: $item_syntax,
                kind: $list_kind,
                item_kind: $item_kind,
                items: $items_method,
                source_slot: $list_source_slot,
                range_description: $list_range_description,
                debug_name: $list_debug_name,
                builder_debug_name: $list_builder_debug_name,
            }
            separator: {
                kind: $separator_kind,
            }
        }
    };

    (
        list {
            $(#[$list_meta:meta])*
            $list_visibility:vis struct $list_syntax:ident {
                builder: $list_builder_syntax:ident,
                kind: $list_kind:path,
                items: $items_method:ident,
                source_slot: $list_source_slot:literal,
                range_description: $list_range_description:literal,
                debug_name: $list_debug_name:literal,
                builder_debug_name: $list_builder_debug_name:literal $(,)?
            }
        }
        item {
            $(#[$item_meta:meta])*
            $item_visibility:vis struct $item_syntax:ident {
                builder: $item_builder_syntax:ident,
                kind: $item_kind:path,
                token_kind: $token_kind:path,
                token: $token_method:ident,
                push_token: $push_token_method:ident,
                source_slot: $item_source_slot:literal,
                token_slot: $token_slot:literal,
                range_description: $item_range_description:literal,
                debug_name: $item_debug_name:literal,
                builder_debug_name: $item_builder_debug_name:literal,
                missing_token_panic: $missing_token_panic:literal $(,)?
            }
        }
        separator: none $(,)?
    ) => {
        $crate::list::define_token_item_syntax! {
            $(#[$item_meta])*
            $item_visibility struct $item_syntax {
                builder: $item_builder_syntax,
                kind: $item_kind,
                token_kind: $token_kind,
                token: $token_method,
                push_token: $push_token_method,
                source_slot: $item_source_slot,
                token_slot: $token_slot,
                range_description: $item_range_description,
                debug_name: $item_debug_name,
                builder_debug_name: $item_builder_debug_name,
                missing_token_panic: $missing_token_panic,
            }
        }

        $crate::list::define_list_syntax! {
            $(#[$list_meta])*
            $list_visibility struct $list_syntax {
                builder: $list_builder_syntax,
                item: $item_syntax,
                kind: $list_kind,
                item_kind: $item_kind,
                items: $items_method,
                source_slot: $list_source_slot,
                range_description: $list_range_description,
                debug_name: $list_debug_name,
                builder_debug_name: $list_builder_debug_name,
            }
            separator: none
        }
    };
}

pub(crate) use define_token_list_syntax;
