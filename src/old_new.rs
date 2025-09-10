#![allow(dead_code)]
use std::ops::Deref;

#[derive(Clone, Copy)]
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
    pub fn consume<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
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
