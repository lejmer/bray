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

    for key in application
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        let source = application
            .source(key.source().syntax().source_id())
            .expect("fixture `source` call must succeed");

        if !source.text().starts_with("trusted module std.channel") {
            continue;
        }

        let checked = application
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        if checked.diagnostics().has_errors() {
            failures.push(format!("{key:?}: {:?}", checked.diagnostics()));
        }
    }

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
        func genuine() -> Result<unit, std.channel.ChannelError> {
            let (sender, receiver) = try std.channel.bounded<u32>(capacity = 2);
            let duplicate = sender.duplicate();
            _ = duplicate.try_send(7);
            _ = receiver.try_receive();
            duplicate.close();
            receiver.close();
            sender.close();
            return Ok(unit);
        }
        func fabricated(pos pointer: DataPointer<std.channel.ChannelCore<u32>>) {
            let sender: std.channel.Sender<u32> = {
                owner = { instance = std.channel.owner.Marker {}, core = pointer },
            };
            _ = sender.try_send(1);
        }
        func expose_protocol(pos sender: &std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance)) {
            let shared = internal std.channel.channel_core<u32>(&sender.owner);
        }
        func change_kind(pos sender: std.channel.Sender<u32>)
            requires(trusted std.channel.owner.live(&sender.owner.instance), trusted std.channel.owner.sender(&sender.owner.instance)) {
            let receiver: std.channel.Receiver<u32> = { owner = sender.owner };
            _ = receiver.try_receive();
        }
        "#,
    );

    for (name, valid) in [
        ("genuine", true),
        ("fabricated", false),
        ("expose_protocol", false),
        ("change_kind", false),
    ] {
        let checked = application
            .lowered_unit(source_function_body_key(&application, name))
            .expect("fixture callable must lower successfully");

        if valid {
            assert!(
                !checked.diagnostics().has_errors(),
                "{name}: {:?}",
                checked.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
