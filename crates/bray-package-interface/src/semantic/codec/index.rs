use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::decode::DecodeBudget;
use crate::{InterfaceSectionTag, InterfaceSemanticRecord, InterfaceValidationError};

#[derive(Debug, Default)]
pub(crate) struct SemanticRecordIndex {
    pub(super) directory: OnceLock<Result<SemanticDirectory, InterfaceValidationError>>,
    pub(super) declaration_templates: OnceLock<Result<(), InterfaceValidationError>>,
    tables: Mutex<
        BTreeMap<
            (InterfaceSectionTag, usize),
            Arc<OnceLock<Result<RecordTableFrame, InterfaceValidationError>>>,
        >,
    >,
}

#[derive(Debug)]
pub(super) struct SemanticDirectory {
    pub(super) records: Arc<[InterfaceSemanticRecord]>,
    pub(super) budget: DecodeBudget,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RecordTableFrame {
    pub(super) payload_length: usize,
}

impl SemanticRecordIndex {
    pub(super) fn table(
        &self,
        section: InterfaceSectionTag,
        offset: usize,
        validate: impl FnOnce() -> Result<RecordTableFrame, InterfaceValidationError>,
    ) -> Result<RecordTableFrame, InterfaceValidationError> {
        let table = {
            let mut tables = self
                .tables
                .lock()
                .expect("immutable semantic record index lock must remain available");

            Arc::clone(tables.entry((section, offset)).or_default())
        };

        table.get_or_init(validate).clone()
    }
}
