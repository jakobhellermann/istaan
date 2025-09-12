#![allow(dead_code)]
use std::collections::BTreeSet;
use std::ops::Deref;

#[derive(Debug, Clone, Copy)]
pub struct OldNew<T> {
    pub old: T,
    pub new: T,
}

impl<T> OldNew<T> {
    pub fn new(old: T, new: T) -> Self {
        OldNew { old, new }
    }
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> OldNew<U> {
        OldNew {
            old: f(self.old),
            new: f(self.new),
        }
    }
    pub fn map_zip<U, O>(self, other: &OldNew<O>, mut f: impl FnMut(T, &O) -> U) -> OldNew<U> {
        OldNew {
            old: f(self.old, &other.old),
            new: f(self.new, &other.new),
        }
    }
    pub fn as_ref(&self) -> OldNew<&T> {
        OldNew {
            old: &self.old,
            new: &self.new,
        }
    }
    pub fn try_map<U, E>(self, mut f: impl FnMut(T) -> Result<U, E>) -> Result<OldNew<U>, E> {
        Ok(OldNew {
            old: f(self.old)?,
            new: f(self.new)?,
        })
    }
    pub fn try_map_zip<U, O, E>(
        self,
        other: &OldNew<O>,
        mut f: impl FnMut(T, &O) -> Result<U, E>,
    ) -> Result<OldNew<U>, E> {
        Ok(OldNew {
            old: f(self.old, &other.old)?,
            new: f(self.new, &other.new)?,
        })
    }
    pub fn consume<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }

    pub fn changes<Iter>(&self, mut f: impl FnMut(&T) -> Iter) -> Changes<Iter::Item>
    where
        Iter: Iterator,
        <Iter as Iterator>::Item: Ord + Copy,
    {
        let old_items: BTreeSet<_> = f(&self.old).collect();
        let new_items: BTreeSet<_> = f(&self.new).collect();
        let removed: BTreeSet<_> = old_items.difference(&new_items).copied().collect();
        let added: BTreeSet<_> = new_items.difference(&old_items).copied().collect();
        let same: BTreeSet<_> = old_items.intersection(&new_items).copied().collect();
        Changes {
            removed,
            added,
            same,
        }
    }

    pub fn max(self) -> T
    where
        T: Ord,
    {
        std::cmp::max(self.old, self.new)
    }
}

#[derive(Debug)]
pub struct Changes<T> {
    pub removed: BTreeSet<T>,
    pub added: BTreeSet<T>,
    pub same: BTreeSet<T>,
}

impl<T: Deref> OldNew<T> {
    pub fn as_deref(&self) -> OldNew<&T::Target> {
        OldNew {
            old: self.old.deref(),
            new: self.new.deref(),
        }
    }
}

impl<T: PartialEq> OldNew<T> {
    pub fn changed(&self) -> bool {
        self.old != self.new
    }
}
