use crate::StorageReportMessage;

pub(crate) const fn message(message: StorageReportMessage) -> &'static str {
    match message {
        StorageReportMessage::Heading => {
            "Category\tProduct\tTarget\tProfile\tToolchain\tBytes\tAlready counted\tAction\n"
        }
        StorageReportMessage::Empty => "No managed build state matches this selection.\n",
        StorageReportMessage::CurrentOutputs => "current outputs",
        StorageReportMessage::RetainedRerun => "retained rerun",
        StorageReportMessage::RetainedHistory => "retained history",
        StorageReportMessage::ActiveWork => "active work",
        StorageReportMessage::ReusableCache => "reusable cache",
        StorageReportMessage::Reclaimable => "reclaimable",
        StorageReportMessage::ChangingBytes => "changing",
        StorageReportMessage::Removed => "removed",
        StorageReportMessage::KeptActive => "kept active",
        StorageReportMessage::WouldRemove => "would remove",
        StorageReportMessage::Retained => "retained",
    }
}
