use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, DiagnosticDuplicateKey};
use crate::kind::DiagnosticKind;
use crate::severity::SeverityKind;

/// Persistent ordered collection of stage-local diagnostics.
///
/// A bag preserves insertion order. Merging retains immutable references to the
/// source collections, so dependent results do not copy diagnostic records.
/// Callers that need global deterministic ordering should merge bags in stable
/// source-and-stage order.
///
/// Merge deduplication compares structured content: severity, kind, primary span,
/// labels, notes, related locations, suggestions, and typed arguments. Localized
/// rendered text and diagnostic IDs are excluded from the key.
///
/// Shared access is thread-safe. Mutation requires exclusive `&mut self` access
/// or caller-owned synchronization.
#[derive(Clone)]
pub struct DiagnosticBag {
    root: Arc<DiagnosticCollection>,
}

#[derive(Debug)]
struct DiagnosticCollection {
    sources: Box<[Arc<Self>]>,
    local: Vec<Diagnostic>,
    has_diagnostics: bool,
}

impl DiagnosticBag {
    /// Creates an empty diagnostic bag.
    pub fn new() -> Self {
        Self {
            root: Arc::new(DiagnosticCollection {
                sources: Box::new([]),
                local: Vec::new(),
                has_diagnostics: false,
            }),
        }
    }

    /// Creates an empty diagnostic bag with space for at least `capacity` items.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            root: Arc::new(DiagnosticCollection {
                sources: Box::new([]),
                local: Vec::with_capacity(capacity),
                has_diagnostics: false,
            }),
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
        if let Some(root) = Arc::get_mut(&mut self.root) {
            root.local.push(diagnostic);
            root.has_diagnostics = true;

            return;
        }

        self.root = Arc::new(DiagnosticCollection {
            sources: Box::new([Arc::clone(&self.root)]),
            local: vec![diagnostic],
            has_diagnostics: true,
        });
    }

    /// Adds diagnostics to the end of the bag, preserving iterator order.
    pub fn add_range(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in diagnostics {
            self.add(diagnostic);
        }
    }

    /// Returns a new bag containing diagnostics from both bags without duplicates.
    ///
    /// The merged bag preserves the first occurrence order from `self`, then
    /// appends diagnostics from `other` that were not already present.
    pub fn merged(&self, other: &Self) -> Self {
        Self::merged_all([self, other])
    }

    /// Returns a new bag containing diagnostics from all bags without duplicates.
    ///
    /// The merged bag preserves the first occurrence order of the input bags
    /// and of diagnostics within each bag.
    pub fn merged_all<'diagnostic>(bags: impl IntoIterator<Item = &'diagnostic Self>) -> Self {
        let mut seen = HashSet::new();

        let sources = bags
            .into_iter()
            .filter(|bag| !bag.is_empty())
            .filter_map(|bag| {
                let identity = Arc::as_ptr(&bag.root);

                seen.insert(identity).then(|| Arc::clone(&bag.root))
            })
            .collect::<Vec<_>>();

        match sources.as_slice() {
            [] => Self::new(),
            [source] => Self {
                root: Arc::clone(source),
            },
            [_, _, ..] => Self {
                root: Arc::new(DiagnosticCollection {
                    sources: sources.into_boxed_slice(),
                    local: Vec::new(),
                    has_diagnostics: true,
                }),
            },
        }
    }

    /// Returns an iterator over diagnostics in insertion order.
    pub fn iter(&self) -> DiagnosticIter<'_> {
        DiagnosticIter::new(self.root.as_ref())
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
    pub fn len(&self) -> usize {
        if self.root.sources.is_empty() {
            return self.root.local.len();
        }

        self.iter().count()
    }

    /// Returns whether the bag contains no diagnostics.
    pub fn is_empty(&self) -> bool {
        !self.root.has_diagnostics
    }

    /// Converts the bag into its underlying ordered diagnostics.
    pub fn into_vec(self) -> Vec<Diagnostic> {
        match Arc::try_unwrap(self.root) {
            Ok(root) if root.sources.is_empty() => root.local,
            Ok(root) => Self {
                root: Arc::new(root),
            }
            .iter()
            .cloned()
            .collect(),
            Err(root) => Self { root }.iter().cloned().collect(),
        }
    }
}

/// Iterator over the unique diagnostics referenced by a bag.
pub struct DiagnosticIter<'diagnostic> {
    leaf: Option<std::slice::Iter<'diagnostic, Diagnostic>>,
    frames: Vec<DiagnosticFrame<'diagnostic>>,
    visited: HashSet<*const DiagnosticCollection>,
    yielded: HashSet<DiagnosticDuplicateKey<'diagnostic>>,
}

struct DiagnosticFrame<'diagnostic> {
    collection: &'diagnostic DiagnosticCollection,
    source: usize,
    local: usize,
}

impl<'diagnostic> DiagnosticIter<'diagnostic> {
    fn new(root: &'diagnostic DiagnosticCollection) -> Self {
        let identity = std::ptr::from_ref(root);

        if root.sources.is_empty() {
            return Self {
                leaf: Some(root.local.iter()),
                frames: Vec::new(),
                visited: HashSet::new(),
                yielded: HashSet::new(),
            };
        }

        Self {
            leaf: None,
            frames: vec![DiagnosticFrame {
                collection: root,
                source: 0,
                local: 0,
            }],
            visited: HashSet::from([identity]),
            yielded: HashSet::new(),
        }
    }
}

impl<'diagnostic> Iterator for DiagnosticIter<'diagnostic> {
    type Item = &'diagnostic Diagnostic;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(leaf) = &mut self.leaf {
            return leaf.next();
        }

        loop {
            let frame = self.frames.last_mut()?;

            if let Some(source) = frame.collection.sources.get(frame.source) {
                frame.source += 1;

                let source = source.as_ref();

                let identity = std::ptr::from_ref(source);

                if self.visited.insert(identity) {
                    self.frames.push(DiagnosticFrame {
                        collection: source,
                        source: 0,
                        local: 0,
                    });
                }

                continue;
            }

            if let Some(diagnostic) = frame.collection.local.get(frame.local) {
                frame.local += 1;

                if self.yielded.insert(diagnostic.duplicate_key()) {
                    return Some(diagnostic);
                }

                continue;
            }

            self.frames.pop();
        }
    }
}

impl Default for DiagnosticBag {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DiagnosticBag {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for DiagnosticBag {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for DiagnosticBag {}

impl Hash for DiagnosticBag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for diagnostic in self {
            diagnostic.hash(state);
        }
    }
}

impl From<Diagnostic> for DiagnosticBag {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::single(diagnostic)
    }
}

impl From<Vec<Diagnostic>> for DiagnosticBag {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        let has_diagnostics = !diagnostics.is_empty();

        Self {
            root: Arc::new(DiagnosticCollection {
                sources: Box::new([]),
                local: diagnostics,
                has_diagnostics,
            }),
        }
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
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticBag {
    type Item = &'a Diagnostic;
    type IntoIter = DiagnosticIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use super::DiagnosticBag;
    use crate::{Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, SeverityKind};

    #[test]
    fn bags_collect_diagnostics_in_insertion_order() {
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let mut bag = DiagnosticBag::new();

        bag.add(first.clone());
        bag.add(second.clone());

        assert_eq!(bag.iter().cloned().collect::<Vec<_>>(), [first, second]);
        assert_eq!(bag.len(), 2);
        assert!(!bag.is_empty());
    }

    #[test]
    fn bags_merge_without_mutating_inputs_and_remove_duplicates() {
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        );

        let left = DiagnosticBag::from(vec![first.clone(), second.clone()]);
        let right = DiagnosticBag::from(vec![second.clone(), first.clone()]);

        let merged = left.merged(&right);

        assert_eq!(
            merged.iter().cloned().collect::<Vec<_>>(),
            [first.clone(), second.clone()]
        );

        assert_eq!(
            left.iter().cloned().collect::<Vec<_>>(),
            [first.clone(), second.clone()]
        );

        assert_eq!(right.iter().cloned().collect::<Vec<_>>(), [second, first]);
    }

    #[test]
    fn cloned_and_merged_bags_reference_their_owning_collections() {
        let first = DiagnosticBag::single(diagnostic(
            0,
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        ));

        let second = DiagnosticBag::single(diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        ));

        let cloned = first.clone();
        let merged = first.merged(&second);

        assert!(Arc::ptr_eq(&first.root, &cloned.root));
        assert!(Arc::ptr_eq(&first.root, &merged.root.sources[0]));
        assert!(Arc::ptr_eq(&second.root, &merged.root.sources[1]));
    }

    #[test]
    fn adding_to_a_shared_bag_retains_the_original_collection() {
        let original = DiagnosticBag::single(diagnostic(
            0,
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        ));

        let mut extended = original.clone();

        extended.add(diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        ));

        assert!(Arc::ptr_eq(&original.root, &extended.root.sources[0]));
        assert_eq!(original.len(), 1);
        assert_eq!(extended.len(), 2);
    }

    #[test]
    fn bags_merge_many_without_duplicates() {
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let third = diagnostic(
            2,
            DiagnosticKind::LexicalUnterminatedBlockComment,
            SeverityKind::Error,
        );

        let first_bag = DiagnosticBag::from(vec![first.clone(), second.clone()]);
        let second_bag = DiagnosticBag::from(vec![second, third.clone()]);
        let third_bag = DiagnosticBag::single(first.clone());

        let merged = DiagnosticBag::merged_all([&first_bag, &second_bag, &third_bag]);

        assert_eq!(
            merged.iter().cloned().collect::<Vec<_>>(),
            [
                first,
                diagnostic(
                    1,
                    DiagnosticKind::LexicalInvalidCharacter,
                    SeverityKind::Warning
                ),
                third
            ]
        );
    }

    #[test]
    fn bags_deduplicate_structural_duplicates_with_different_ids() {
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error)
            .with_arg(DiagnosticArg::text_offset(TextSize::new(4)));

        let duplicate = diagnostic(99, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error)
            .with_arg(DiagnosticArg::text_offset(TextSize::new(4)));

        let left = DiagnosticBag::single(first.clone());
        let right = DiagnosticBag::single(duplicate);

        let merged = left.merged(&right);

        assert_eq!(merged.iter().cloned().collect::<Vec<_>>(), [first]);
    }

    #[test]
    fn bags_preserve_similarly_rendered_distinct_source_diagnostics() {
        let first_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(1), TextSize::new(2)),
        );

        let second_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let first = diagnostic(
            0,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(first_span);

        let second = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_primary_span(second_span);

        let left = DiagnosticBag::single(first.clone());
        let right = DiagnosticBag::single(second.clone());

        let merged = left.merged(&right);

        assert_eq!(merged.iter().cloned().collect::<Vec<_>>(), [first, second]);
    }

    #[test]
    fn bags_filter_by_kind_and_severity() {
        let error = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

        let warning = diagnostic(
            1,
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let mut bag = DiagnosticBag::new();

        bag.add_range([error.clone(), warning.clone()]);

        assert_eq!(
            bag.by_kind(DiagnosticKind::SourceInvalidUtf8)
                .collect::<Vec<_>>(),
            vec![&error]
        );

        assert_eq!(bag.errors().collect::<Vec<_>>(), vec![&error]);
        assert_eq!(bag.warnings().collect::<Vec<_>>(), vec![&warning]);
        assert!(bag.has_errors());
    }

    #[test]
    fn bags_convert_to_and_from_owned_diagnostics() {
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

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
        let first = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

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
        let error = diagnostic(0, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error);

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
                assert_eq!(bag.by_kind(DiagnosticKind::SourceInvalidUtf8).count(), 1);
            });
        });
    }

    fn diagnostic(id: u32, kind: DiagnosticKind, severity: SeverityKind) -> Diagnostic {
        Diagnostic::new(DiagnosticId::new(id), kind, severity)
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
