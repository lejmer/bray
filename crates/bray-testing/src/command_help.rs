use clap::Command;

/// Verifies that every visible command, argument, and enumerated value has useful help.
pub fn assert_complete_command_help(command: &mut Command) {
    command.build();
    assert_command_help(command);
}

fn assert_command_help(command: &Command) {
    assert!(
        command.get_about().is_some() || command.get_long_about().is_some(),
        "command `{}` must have help",
        command.get_name()
    );

    for argument in command
        .get_arguments()
        .filter(|argument| !argument.is_hide_set())
    {
        assert!(
            argument.get_help().is_some() || argument.get_long_help().is_some(),
            "argument `{}` on command `{}` must have help",
            argument.get_id(),
            command.get_name()
        );

        for value in argument.get_possible_values() {
            assert!(
                value.get_help().is_some(),
                "value `{}` for argument `{}` on command `{}` must have help",
                value.get_name(),
                argument.get_id(),
                command.get_name()
            );
        }
    }

    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        assert_command_help(subcommand);
    }
}
