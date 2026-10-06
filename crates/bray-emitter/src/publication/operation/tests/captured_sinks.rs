use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use crate::{
    ArtifactKind, IndirectOutputSink, OutputSinkId, OutputSinkResolver, OutputSinkTransaction,
    ReplacementPolicy,
};

pub(super) struct CapturingResolver {
    sinks: BTreeMap<String, Arc<Mutex<Vec<u8>>>>,
    fail_open: Option<ArtifactKind>,
}

impl CapturingResolver {
    pub(super) fn new(sinks: impl IntoIterator<Item = OutputSinkId>) -> Self {
        Self::with_failure(sinks, None)
    }

    pub(super) fn failing_open(
        sinks: impl IntoIterator<Item = OutputSinkId>,
        kind: ArtifactKind,
    ) -> Self {
        Self::with_failure(sinks, Some(kind))
    }

    fn with_failure(
        sinks: impl IntoIterator<Item = OutputSinkId>,
        fail_open: Option<ArtifactKind>,
    ) -> Self {
        let sinks = sinks
            .into_iter()
            .map(|sink| (sink.as_str().to_owned(), Arc::new(Mutex::new(Vec::new()))))
            .collect();

        Self { sinks, fail_open }
    }

    pub(super) fn bytes(&self, sink: &str) -> Vec<u8> {
        let Some(bytes) = self.sinks.get(sink) else {
            panic!("test sink must exist");
        };

        let Ok(bytes) = bytes.lock() else {
            panic!("test sink lock must be available");
        };

        bytes.clone()
    }
}

impl OutputSinkResolver for CapturingResolver {
    fn open(
        &self,
        sink: IndirectOutputSink<'_>,
        _replacement: ReplacementPolicy,
    ) -> io::Result<Box<dyn OutputSinkTransaction>> {
        let (identity, artifact_kind) = match sink {
            IndirectOutputSink::Memory {
                collector,
                artifact,
            } => (collector, Some(artifact.kind())),
            IndirectOutputSink::Stream(stream) => (stream, None),
        };

        if artifact_kind.is_some_and(|kind| self.fail_open == Some(kind)) {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }

        let Some(bytes) = self.sinks.get(identity.as_str()) else {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        };

        Ok(Box::new(CapturingWriter {
            destination: Arc::clone(bytes),
            buffer: Vec::new(),
        }))
    }
}

struct CapturingWriter {
    destination: Arc<Mutex<Vec<u8>>>,
    buffer: Vec<u8>,
}

impl Write for CapturingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buffer);

        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl OutputSinkTransaction for CapturingWriter {
    fn commit(self: Box<Self>) -> io::Result<()> {
        let Self {
            destination,
            buffer,
        } = *self;

        commit_buffer(destination, buffer)
    }
}

pub(super) fn commit_buffer(destination: Arc<Mutex<Vec<u8>>>, buffer: Vec<u8>) -> io::Result<()> {
    let mut destination = destination
        .lock()
        .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

    *destination = buffer;

    Ok(())
}
