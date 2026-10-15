#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RefinementSet {
    words: Box<[u64]>,
}

impl RefinementSet {
    pub(crate) fn empty(refinements: usize) -> Self {
        Self {
            words: vec![0; refinements.div_ceil(u64::BITS as usize)].into_boxed_slice(),
        }
    }

    #[cfg(test)]
    pub(crate) fn full(refinements: usize) -> Self {
        let mut set = Self {
            words: vec![u64::MAX; refinements.div_ceil(u64::BITS as usize)].into_boxed_slice(),
        };

        if let Some(last) = set.words.last_mut() {
            let trailing = refinements % u64::BITS as usize;

            if trailing != 0 {
                *last = (1_u64 << trailing) - 1;
            }
        }

        set
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    pub(crate) fn len(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    pub(crate) fn clear(&mut self) {
        self.words.fill(0);
    }

    pub(crate) fn insert(&mut self, index: usize) {
        let Some(word) = self.words.get_mut(index / u64::BITS as usize) else {
            return;
        };

        *word |= 1_u64 << (index % u64::BITS as usize);
    }

    pub(crate) fn remove(&mut self, index: usize) {
        let Some(word) = self.words.get_mut(index / u64::BITS as usize) else {
            return;
        };

        *word &= !(1_u64 << (index % u64::BITS as usize));
    }

    pub(crate) fn intersect(&mut self, other: &Self) -> bool {
        let mut changed = false;

        for (target, incoming) in self.words.iter_mut().zip(&other.words) {
            let merged = *target & incoming;

            changed |= merged != *target;
            *target = merged;
        }

        changed
    }

    pub(crate) fn retain(&mut self, mut predicate: impl FnMut(usize) -> bool) {
        for (word_index, word) in self.words.iter_mut().enumerate() {
            let mut remaining = *word;

            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let mask = 1_u64 << bit;
                let index = word_index * u64::BITS as usize + bit;

                if !predicate(index) {
                    *word &= !mask;
                }

                remaining &= !mask;
            }
        }
    }

    pub(crate) fn indexes(&self) -> impl Iterator<Item = usize> + Clone + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(|(word_index, word)| {
                let word = *word;

                (0..u64::BITS as usize).filter_map(move |bit| {
                    (word & (1_u64 << bit) != 0).then_some(word_index * u64::BITS as usize + bit)
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::RefinementSet;

    #[test]
    fn refinement_sets_intersect_in_place_and_keep_deterministic_indexes() {
        let mut target = RefinementSet::full(70);
        let mut incoming = RefinementSet::empty(70);

        incoming.insert(1);
        incoming.insert(65);

        assert!(target.intersect(&incoming));
        assert_eq!(target.indexes().collect::<Vec<_>>(), [1, 65]);
        assert!(!target.intersect(&incoming));
    }
}
