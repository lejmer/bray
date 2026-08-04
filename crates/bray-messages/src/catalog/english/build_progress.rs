use crate::BuildProgressMessage;

pub(crate) const fn message(message: BuildProgressMessage) -> &'static str {
    match message {
        BuildProgressMessage::Building => "Building",
        BuildProgressMessage::Compiling => "Compiling",
        BuildProgressMessage::Compiled => "Compiled",
        BuildProgressMessage::Finished => "Finished",
        BuildProgressMessage::Failed => "Failed",
        BuildProgressMessage::Debug => "debug",
        BuildProgressMessage::Release => "release",
        BuildProgressMessage::Units => "units",
        BuildProgressMessage::CheckingInterface => "Checking dependency interface",
        BuildProgressMessage::ProducingArtifacts => "Producing product artifacts",
    }
}
