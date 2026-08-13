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

    pub(crate) fn contains_recovery_instruction(self) -> bool {
        self.parts.iter().any(|part| match part {
            MessageTemplatePart::Text(text) => text_contains_recovery_instruction(text),
            MessageTemplatePart::Arg(_) => false,
        })
    }
}

fn text_contains_recovery_instruction(text: &str) -> bool {
    let text = text.to_ascii_lowercase();

    const IMPERATIVE_PREFIXES: &[&str] = &[
        "add ",
        "change ",
        "check ",
        "choose ",
        "consider ",
        "declare ",
        "disable ",
        "enable ",
        "ensure ",
        "install ",
        "make sure ",
        "please ",
        "provide ",
        "read ",
        "rebuild ",
        "remove ",
        "replace ",
        "report ",
        "retry ",
        "run ",
        "see ",
        "select ",
        "set ",
        "specify ",
        "try ",
        "update ",
        "use ",
    ];

    let clause_starts_with_instruction = text.split(['.', ';']).any(|clause| {
        let clause = clause.trim_start();

        IMPERATIVE_PREFIXES
            .iter()
            .any(|prefix| clause.starts_with(prefix))
    });

    clause_starts_with_instruction
        || text.contains("to fix this")
        || text.contains("you can ")
        || text.contains("you should ")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MessageTemplatePart {
    Text(&'static str),
    Arg(DiagnosticArgName),
}

#[cfg(test)]
mod tests {
    use super::text_contains_recovery_instruction;

    #[test]
    fn primary_message_policy_rejects_recovery_instructions() {
        for imperative in [
            "add",
            "change",
            "check",
            "choose",
            "consider",
            "declare",
            "disable",
            "enable",
            "ensure",
            "install",
            "make sure",
            "please",
            "provide",
            "read",
            "rebuild",
            "remove",
            "replace",
            "report",
            "retry",
            "run",
            "see",
            "select",
            "set",
            "specify",
            "try",
            "update",
            "use",
        ] {
            let direct = format!("{imperative} the selected option");
            let appended = format!("linking failed. {imperative} the selected option");

            assert!(text_contains_recovery_instruction(&direct), "{direct}");
            assert!(text_contains_recovery_instruction(&appended), "{appended}");
        }

        for message in [
            "linking failed. To fix this, install LLVM",
            "linking failed. You should install LLVM",
            "linking failed. You can select another target",
        ] {
            assert!(text_contains_recovery_instruction(message), "{message}");
        }

        assert!(!text_contains_recovery_instruction(
            "selected target does not provide the required capability"
        ));
    }
}
