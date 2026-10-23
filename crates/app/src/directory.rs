use crate::{Directory, DirectorySpecial};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Builder {
    special: DirectorySpecial,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            special: DirectorySpecial::No,
        }
    }

    pub fn special(&mut self, special: DirectorySpecial) -> &mut Self {
        self.special = special;
        self
    }

    pub fn build(&self) -> Directory {
        Directory {
            special: self.special,
        }
    }
}
