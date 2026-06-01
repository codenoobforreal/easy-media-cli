use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub struct IdGenerator {
    next_id: NonZeroUsize,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self {
            next_id: NonZeroUsize::new(1).unwrap(),
        }
    }

    pub fn next(&mut self) -> usize {
        let id = self.next_id.get();
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("ID generator overflowed");
        id
    }

    pub fn reset(&mut self) {
        self.next_id = NonZeroUsize::new(1).unwrap();
    }
}
