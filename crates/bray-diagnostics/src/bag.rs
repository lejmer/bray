use crate::diagnostic::Diagnostic;
use crate::kind::DiagnosticKind;
use crate::severity::SeverityKind;

/// Ordered collection of diagnostics produced by one compiler operation.
///
/// A bag preserves insertion order. Callers that need global deterministic
/// ordering should add diagnostics in deterministic phase order or sort before
/// publication at the owning boundary.
///
/// `DiagnosticBag` has no interior mutability. Shared access is thread-safe:
/// immutable bags can be read concurrently by multiple workers, while mutation
/// requires exclusive `&mut self` access or caller-owned synchronization.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    /// Creates an empty diagnostic bag.
    pub const fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Creates an empty diagnostic bag with space for at least `capacity` items.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            diagnostics: Vec::with_capacity(capacity),
        }
    }

    /// Creates a diagnostic bag containing one diagnostic.
    pub fn single(diagnostic: Diagnostic) -> Self {
        let mut bag = Self::with_capacity(1);
        bag.add(diagnostic);

        bag
    }

    /// Adds one diagnostic to the end of the bag.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Adds diagnostics to the end of the bag, preserving iterator order.
    pub fn add_range(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    /// Moves all diagnostics from `other` into this bag.
    pub fn append(&mut self, other: &mut Self) {
        self.diagnostics.append(&mut other.diagnostics);
    }

    /// Returns all diagnostics in insertion order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns an iterator over diagnostics in insertion order.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics.iter()
    }

    /// Returns diagnostics with the requested stable category.
    pub fn by_kind(&self, kind: DiagnosticKind) -> impl Iterator<Item = &Diagnostic> {
        self.iter()
            .filter(move |diagnostic| diagnostic.kind() == kind)
    }

    /// Returns diagnostics with the requested severity.
    pub fn by_severity(&self, severity: SeverityKind) -> impl Iterator<Item = &Diagnostic> {
        self.iter()
            .filter(move |diagnostic| diagnostic.severity() == severity)
    }

    /// Returns diagnostics with error severity.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.by_severity(SeverityKind::Error)
    }

    /// Returns diagnostics with warning severity.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.by_severity(SeverityKind::Warning)
    }

    /// Returns whether the bag contains at least one error.
    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    /// Returns the number of diagnostics in the bag.
    pub const fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns whether the bag contains no diagnostics.
    pub const fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Converts the bag into its underlying ordered diagnostics.
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

impl From<Diagnostic> for DiagnosticBag {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::single(diagnostic)
    }
}

impl From<Vec<Diagnostic>> for DiagnosticBag {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }
}

impl From<DiagnosticBag> for Vec<Diagnostic> {
    fn from(bag: DiagnosticBag) -> Self {
        bag.into_vec()
    }
}

impl Extend<Diagnostic> for DiagnosticBag {
    fn extend<T: IntoIterator<Item = Diagnostic>>(&mut self, iter: T) {
        self.add_range(iter);
    }
}

impl FromIterator<Diagnostic> for DiagnosticBag {
    fn from_iter<T: IntoIterator<Item = Diagnostic>>(iter: T) -> Self {
        let mut bag = Self::new();
        bag.extend(iter);

        bag
    }
}

impl IntoIterator for DiagnosticBag {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticBag {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticBag;
    use crate::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};

    #[test]
    fn bags_collect_diagnostics_in_insertion_order() {
        let first = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let mut bag = DiagnosticBag::new();

        bag.add(first.clone());
        bag.add(second.clone());

        assert_eq!(bag.diagnostics(), &[first, second]);
        assert_eq!(bag.len(), 2);
        assert!(!bag.is_empty());
    }

    #[test]
    fn bags_append_and_drain_other_bags() {
        let first = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        );

        let mut left = DiagnosticBag::single(first.clone());
        let mut right = DiagnosticBag::single(second.clone());

        left.append(&mut right);

        assert_eq!(left.diagnostics(), &[first, second]);
        assert!(right.is_empty());
    }

    #[test]
    fn bags_filter_by_kind_and_severity() {
        let error = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let warning = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let mut bag = DiagnosticBag::new();
        bag.add_range([error.clone(), warning.clone()]);

        assert_eq!(
            bag.by_kind(DiagnosticKind::LexicalInvalidUtf8)
                .collect::<Vec<_>>(),
            vec![&error]
        );
        assert_eq!(bag.errors().collect::<Vec<_>>(), vec![&error]);
        assert_eq!(bag.warnings().collect::<Vec<_>>(), vec![&warning]);
        assert!(bag.has_errors());
    }

    #[test]
    fn bags_convert_to_and_from_owned_diagnostics() {
        let first = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let bag = DiagnosticBag::from(vec![first.clone(), second.clone()]);

        assert_eq!(bag.clone().into_vec(), vec![first.clone(), second.clone()]);
        assert_eq!(bag.into_iter().collect::<Vec<_>>(), vec![first, second]);
    }

    #[test]
    fn bags_work_with_standard_collection_traits() {
        let first = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let bag = [first.clone(), second.clone()]
            .into_iter()
            .collect::<DiagnosticBag>();

        assert_eq!(bag.iter().collect::<Vec<_>>(), vec![&first, &second]);
        assert_eq!(
            (&bag).into_iter().collect::<Vec<_>>(),
            vec![&first, &second]
        );
    }

    #[test]
    fn bags_are_send_and_sync() {
        assert_send_sync::<DiagnosticBag>();
    }

    #[test]
    fn bags_allow_concurrent_shared_reads() {
        let error = diagnostic(0, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error);
        let warning = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let bag = DiagnosticBag::from(vec![error, warning]);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert!(bag.has_errors());
            });
            scope.spawn(|| {
                assert_eq!(bag.warnings().count(), 1);
            });
            scope.spawn(|| {
                assert_eq!(bag.by_kind(DiagnosticKind::LexicalInvalidUtf8).count(), 1);
            });
        });
    }

    fn diagnostic(id: u32, kind: DiagnosticKind, severity: SeverityKind) -> Diagnostic {
        Diagnostic::new(DiagnosticId::new(id), kind, severity)
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
