use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn borrowed_heap_fields_preserve_the_anchor_without_projecting_onto_it() {
    let mut failures = Vec::new();

    for (parameter, argument, valid) in [("&Owner", "owner", true), ("Owner", "&owner", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner {{ pointer: DataPointer<ForeignPayload>; }}
            struct Marker {{}}
            struct ForeignPayload {{ marker: Marker; }}
            trusted predicate retained(owner: &Owner);
            trusted func referent(pos owner: &Owner) -> &ForeignPayload
                requires(trusted retained(owner))
                executes(pure, total)
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos owner: &Owner)
                    requires(trusted retained(owner))
                    ensures(
                        trusted core.memory.valid_read<ForeignPayload>(pointer = owner.pointer, count = 1),
                        trusted core.memory.aligned_for<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.initialized_as<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.shared_alias_valid<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.epoch_current<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.synchronized_access<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.movement_stable<ForeignPayload>(pointer = owner.pointer),
                        trusted core.memory.finalization_pending<ForeignPayload>(pointer = owner.pointer),
                    )
                    executes(pure, total)
                {{}};

                trusted permit(owner);

                return trusted core.memory.borrow_from<ForeignPayload, Owner>(owner, owner.pointer);
            }}
            func selected_marker(pos owner: &Owner) -> &Marker
                requires(trusted retained(owner))
            {{
                let backing = referent(owner);
                return &backing.marker;
            }}
            func forwarded(pos owner: {parameter}) -> &Marker
                requires(trusted retained({argument}))
            {{ return selected_marker({argument}); }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let diagnostics = application.check_diagnostics();
        let expected = DiagnosticKind::CheckingEscapingStorageDependency;

        let matches_expected = if valid {
            !diagnostics.has_errors()
        } else {
            diagnostics.iter().any(|diagnostic| diagnostic.kind() == expected)
                && diagnostics.iter().all(|diagnostic| diagnostic.kind() == expected)
        };

        if !matches_expected {
            failures.push((valid, source, diagnostics.clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn nested_buffer_views_preserve_checked_storage_effects() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(),
        r#"
        trusted module app;

        using core.target;
        using core.memory;
        using std.memory;

        struct Row
        {
            first: usize;
            mut slots: std.memory.RawBuffer<u32>;
        }

        struct Directory
        {
            mut rows: std.memory.RawBuffer<Row>?;
            total: usize;
        }

        struct Pointer
        {
            pointer: DataPointer<u32>;
        }

        struct DirectoryLink
        {
            directory: &Directory;
        }

        struct LinkOwner
        {
            pointer: DataPointer<DirectoryLink>;
        }

        trusted predicate linked(owner: &LinkOwner);

        extern trusted func rewrite_link(pos owner: &mut LinkOwner)
            ensures(trusted linked(&owner));

        trusted func borrow_link(pos owner: &LinkOwner) -> &DirectoryLink
            requires(trusted linked(owner))
            executes(pure, total)
            uses(raw_memory, unchecked_alias)
        {
            let permit = trusted lambda(pos owner: &LinkOwner)
                requires(trusted linked(owner))
                ensures(
                    trusted core.memory.valid_read<DirectoryLink>(pointer = owner.pointer, count = 1),
                    trusted core.memory.aligned_for<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.initialized_as<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.shared_alias_valid<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.epoch_current<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.synchronized_access<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.movement_stable<DirectoryLink>(pointer = owner.pointer),
                    trusted core.memory.finalization_pending<DirectoryLink>(pointer = owner.pointer)
                )
                executes(pure, total)
            {
            };

            trusted permit(owner);
            return trusted core.memory.borrow_from<DirectoryLink, LinkOwner>(owner, owner.pointer);
        }

        trusted func inspect_shared(pos directory: &Directory, pos index: usize) -> &u32
            uses(raw_memory, intrinsic)
        {
            if let ?backing = directory.rows
            {
                let rows = trusted std.memory.initialized_slice<Row>(&backing);
                let slots = trusted std.memory.initialized_slice<u32>(&rows[index].slots);

                return &slots[0];
            }

            trusted core.target.abort();
        }

        trusted func inspect_indirect(pos owner: &LinkOwner, pos index: usize) -> &u32
            requires(trusted linked(owner))
        {
            let link = trusted borrow_link(owner);

            return trusted inspect_shared(link.directory, index);
        }

        trusted func inspect_replaced(pos owner: &mut LinkOwner, pos index: usize) -> &u32
            requires(trusted linked(&owner))
        {
            trusted rewrite_link(owner);

            let link = trusted borrow_link(&owner);

            return trusted inspect_shared(link.directory, index);
        }

        trusted func inspect_buffer(pos buffer: &mut std.memory.RawBuffer<Row>, pos index: usize) -> &mut u32
            uses(raw_memory)
        {
            let rows = trusted std.memory.initialized_slice_mut<Row>(buffer);
            let slots = trusted std.memory.initialized_slice_mut<u32>(&mut rows[index].slots);
            return &mut slots[0];
        }

        trusted func inspect_directory(pos directory: &mut Directory, pos index: usize) -> &mut u32
            requires(index > 0, index <= directory.total)
            uses(intrinsic, raw_memory)
        {
            if let ?backing = directory.rows
            {
                let rows = trusted std.memory.initialized_slice_mut<Row>(&mut backing);
                let mut low: usize = 0;
                let mut high = rows.length();

                while low < high
                {
                    let middle = low + (high - low) / 2;

                    if rows[middle].first <= index
                    {
                        low = middle + 1;
                    }
                    else
                    {
                        high = middle;
                    }
                }

                let first = rows[low - 1].first;
                let slots = trusted std.memory.initialized_slice_mut<u32>(&mut rows[low - 1].slots);

                return &mut slots[index - first];
            }

            trusted core.target.abort();
        }

        trusted func inspect_forwarded(pos buffer: &mut std.memory.RawBuffer<Row>, pos index: usize) -> &mut u32
        {
            return trusted inspect_buffer(buffer, index);
        }

        trusted func inspect_issued(pos buffer: &mut std.memory.RawBuffer<Row>, pos index: usize) -> Pointer
        {
            let slot = trusted inspect_buffer(buffer, index);
            let issue = trusted lambda(pos slot: &u32) -> Pointer
                uses(intrinsic)
            {
                return
                {
                    pointer = std.memory.address_of<u32>(slot)
                };
            };

            return trusted issue(&slot);
        }
        "#,
    );

    assert!(!application.syntax_tree_result().diagnostics().has_errors());

    let mut failures = Vec::new();

    for name in ["inspect_buffer", "inspect_directory", "inspect_forwarded", "inspect_issued", "inspect_shared", "inspect_indirect", "inspect_replaced"] {
        let key = source_function_body_key(&application, name);
        let checked = application.lowered_unit(key.clone()).unwrap();

        if checked.diagnostics().has_errors() {
            failures.push(format!("{name}: {checked:#?}"));
            continue;
        }

        let flow = application.storage_flow(key).unwrap();

        assert!(!flow.value().is_recovered());
        assert_eq!(flow.value().dependencies().preserves_storage_on_normal_return(), name != "inspect_replaced");
        assert!(flow.value().dependencies().normal_completion());

        let returned = flow.value().dependencies().borrows().iter().filter(|relation|
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result).collect::<Vec<_>>();

        assert!(!returned.is_empty(), "{name} publishes its actual result reference");

        let closed = returned.iter().all(|relation|
            relation.entry().is_some()
                && relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions
                && relation.target_projections().iter().filter(|projection| **projection == bray_symbols::DependencyProjection::OwnedTarget).count() == 2
                && relation.target_projections().contains(&bray_symbols::DependencyProjection::AnyElement));

        if closed != (name != "inspect_replaced") {
            failures.push(format!("{name}: closed entry backing expected={}, replacement cannot retain entry origins: {returned:#?}", name != "inspect_replaced"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn entry_slot_authority_preserves_its_pointer_without_granting_unowned_memory_access() {
    for (body, valid) in [
        ("opaque(); observe_slot(&pointer);", true),
        ("overwrite(other); observe_slot(&pointer);", true),
        (
            "_ = trusted publish(&pointer); opaque(); observe_slot(&pointer);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using core.target;
            trusted predicate slot(owner: &DataPointer<u32>);
            extern func opaque() executes(total);
            func observe_slot(pos owner: &DataPointer<u32>) requires(trusted slot(owner = owner)) executes(pure, total) {{}}
            trusted func publish(pos owner: &DataPointer<u32>) -> usize uses(intrinsic) {{
                return trusted core.target.expose_address<DataPointer<u32>>(core.memory.address_of<DataPointer<u32>>(owner));
            }}
            trusted func overwrite(pos pointer: DataPointer<u64>)
                requires(
                    trusted core.memory.valid_write<u64>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u64>(pointer = pointer)
                ) uses(raw_memory, unchecked_init) {{ trusted core.memory.write<u64>(pointer, 0); }}
            func caller(pos pointer: DataPointer<u32>, pos other: DataPointer<u64>)
                requires(
                    trusted slot(owner = &pointer),
                    trusted core.memory.valid_write<u64>(pointer = other, count = 1),
                    trusted core.memory.aligned_for<u64>(pointer = other)
                ) {{ {body} }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}"
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("entry slot authority retains its actual pointer owner");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }

    let source = r#"
        trusted module app;
        using core.memory;
        extern func opaque() executes(total);
        trusted predicate slot(owner: &DataPointer<u32>);
        trusted func caller(pos pointer: DataPointer<u32>) -> u32
            requires(
                trusted slot(owner = &pointer),
                trusted core.memory.valid_read<u32>(pointer = pointer, count = 1),
                trusted core.memory.aligned_for<u32>(pointer = pointer),
                trusted core.memory.initialized_as<u32>(pointer = pointer)
            )
            uses(raw_memory)
        {
            opaque();
            return core.memory.read<u32>(pointer);
        }
    "#;

    let application = compilation(source);

    assert!(
        !application.syntax_tree_result().diagnostics().has_errors(),
        "{source}"
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("unowned memory permissions retain their checked failure");

    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingTrustedObligationNotProven),
        "{checked:#?}"
    );
}
