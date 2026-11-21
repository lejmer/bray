use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};

#[test]
#[ignore = "tier:proof"]
fn production_channel_declarations_check_their_own_bodies() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        "module app;",
    );

    let mut failures = Vec::new();
    let mut selected = 0;

    for key in application
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        if !super::fixtures::body_belongs_to(&application, &key, &[&["std", "channel"], &["std", "channel", "owner"]], &[]) {
            continue;
        }

        selected += 1;

        let checked = application
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

    assert!(selected > 0, "production channel must supply callable bodies");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "tier:proof"]
fn production_channel_rejects_fabrication_and_protocol_access() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        r#"
        module app;

        using std.channel;
        func genuine() -> Result<unit, std.channel.ChannelError>
        {
            let (sender, receiver) = try std.channel.bounded<u32>(capacity = 2);
            let duplicate = sender.duplicate();
            _ = duplicate.try_send(7);
            _ = receiver.try_receive();
            duplicate.close();
            receiver.close();
            sender.close();
            return Ok(unit);
        }
        func fabricated(pos pointer: DataPointer<std.channel.ChannelCore<u32>>)
        {
            let sender: std.channel.Sender<u32> =
            {
                owner = { instance = std.channel.owner.Marker {}, core = pointer },
            };
            _ = sender.try_send(1);
        }
        func expose_protocol(pos sender: &std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance))
        {
            let shared = internal std.channel.channel_core<u32>(&sender.owner);
        }
        func change_kind(pos sender: std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance))
        {
            let receiver: std.channel.Receiver<u32> = { owner = sender.owner };
            _ = receiver.try_receive();
        }
        "#,
    );

    for (name, site) in [
        ("genuine", None),
        ("fabricated", Some("sender.try_send(1)")),
        ("expose_protocol", Some("std.channel.channel_core<u32>(&sender.owner)")),
        ("change_kind", Some("_ = receiver.try_receive()")),
    ] {
        let key = source_function_body_key(&application, name);
        let source_id = key.source().syntax().source_id();

        let checked = application
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        if let Some(site) = site {
            let source = application.source(source_id).expect("the fixture source is retained");

            assert!(checked.diagnostics().by_kind(DiagnosticKind::CheckingTrustedObligationNotProven).any(|diagnostic|
                crate::test_support::diagnostic_at_call(diagnostic, source_id, source.text(), site)
            ), "{name}: {:?}", checked.diagnostics());
        } else {
            assert!(
                !checked.diagnostics().has_errors(),
                "{name}: {:?}",
                checked.diagnostics()
            );
        }
    }
}
