use crate::SymbolId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SymbolIdCapacityError {
    pub(crate) index: usize,
}

pub(crate) struct SymbolIdAllocator {
    next_index: usize,
}

impl SymbolIdAllocator {
    pub(crate) const fn new() -> Self {
        Self { next_index: 0 }
    }

    pub(crate) const fn starting_at(next_index: usize) -> Self {
        Self { next_index }
    }

    pub(crate) fn next(&mut self) -> Result<SymbolId, SymbolIdCapacityError> {
        let index = self.next_index;
        
        let Some(id) = SymbolId::try_from_index(index) else {
            return Err(SymbolIdCapacityError { index });
        };

        let Some(next_index) = index.checked_add(1) else {
            return Err(SymbolIdCapacityError { index });
        };

        self.next_index = next_index;

        Ok(id)
    }

    pub(crate) const fn next_index(&self) -> usize {
        self.next_index
    }
}
