use bray_diagnostics::DiagnosticArgName;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MessageTemplate {
    parts: &'static [MessageTemplatePart],
}

impl MessageTemplate {
    pub(crate) const fn new(parts: &'static [MessageTemplatePart]) -> Self {
        Self { parts }
    }

    pub(crate) const fn parts(self) -> &'static [MessageTemplatePart] {
        self.parts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MessageTemplatePart {
    Text(&'static str),
    Arg(DiagnosticArgName),
}
