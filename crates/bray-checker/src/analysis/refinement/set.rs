#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FactSet {
    words: Box<[u64]>,
}

impl FactSet {
    pub(super) fn empty(facts: usize) -> Self {
        Self {
            words: vec![0; facts.div_ceil(u64::BITS as usize)].into_boxed_slice(),
        }
    }

    pub(super) fn full(facts: usize) -> Self {
        let mut set = Self {
            words: vec![u64::MAX; facts.div_ceil(u64::BITS as usize)].into_boxed_slice(),
        };

        if let Some(last) = set.words.last_mut() {
            let trailing = facts % u64::BITS as usize;

            if trailing != 0 {
                *last = (1_u64 << trailing) - 1;
            }
        }

        set
    }

    pub(super) fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    pub(super) fn clear(&mut self) {
        self.words.fill(0);
    }

    pub(super) fn insert(&mut self, index: usize) {
        let Some(word) = self.words.get_mut(index / u64::BITS as usize) else {
            return;
        };

        *word |= 1_u64 << (index % u64::BITS as usize);
    }

    pub(super) fn remove(&mut self, index: usize) {
        let Some(word) = self.words.get_mut(index / u64::BITS as usize) else {
            return;
        };

        *word &= !(1_u64 << (index % u64::BITS as usize));
    }

    pub(super) fn replace(&mut self, other: &Self) -> bool {
        if self == other {
            return false;
        }

        self.words.clone_from(&other.words);

        true
    }

    pub(super) fn intersect(&mut self, other: &Self) -> bool {
        let mut changed = false;

        for (target, incoming) in self.words.iter_mut().zip(&other.words) {
            let merged = *target & incoming;

            changed |= merged != *target;
            *target = merged;
        }

        changed
    }

    pub(super) fn retain(&mut self, mut predicate: impl FnMut(usize) -> bool) {
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

    pub(super) fn indexes(&self) -> impl Iterator<Item = usize> + '_ {
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
    use super::FactSet;

    #[test]
    fn fact_sets_intersect_in_place_and_keep_deterministic_indexes() {
        let mut target = FactSet::full(70);
        let mut incoming = FactSet::empty(70);

        incoming.insert(1);
        incoming.insert(65);

        assert!(target.intersect(&incoming));
        assert_eq!(target.indexes().collect::<Vec<_>>(), [1, 65]);
        assert!(!target.intersect(&incoming));
    }
}
