use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_syntax::SyntaxToken;

#[derive(Clone, Debug)]
pub(super) struct TokenScan {
    token: SyntaxToken,
    diagnostics: DiagnosticBag,
}

impl TokenScan {
    pub(super) fn clean(token: SyntaxToken) -> Self {
        Self {
            token,
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(super) fn with_diagnostic(token: SyntaxToken, diagnostic: Diagnostic) -> Self {
        Self {
            token,
            diagnostics: DiagnosticBag::single(diagnostic),
        }
    }

    pub(super) fn with_diagnostics(token: SyntaxToken, diagnostics: DiagnosticBag) -> Self {
        Self { token, diagnostics }
    }

    pub(super) fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(super) fn into_token(self) -> SyntaxToken {
        self.token
    }
}
