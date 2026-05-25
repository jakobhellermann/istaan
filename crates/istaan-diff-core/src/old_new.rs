#![allow(dead_code)]
use std::collections::BTreeSet;
use std::ops::{ControlFlow, Deref, FromResidual, Residual, Try};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
    pub fn map_zip<'a, U, O>(
        self,
        other: &'a OldNew<O>,
        mut f: impl FnMut(T, &'a O) -> U,
    ) -> OldNew<U> {
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
    pub fn try_map_zip<'a, U, O, E>(
        self,
        other: &'a OldNew<O>,
        mut f: impl FnMut(T, &'a O) -> Result<U, E>,
    ) -> Result<OldNew<U>, E> {
        Ok(OldNew {
            old: f(self.old, &other.old)?,
            new: f(self.new, &other.new)?,
        })
    }

    #[cfg(feature = "rayon")]
    pub fn try_map_parallel<U: Send, E: Send>(
        self,
        f: impl Fn(T) -> Result<U, E> + Send + Sync,
    ) -> Result<OldNew<U>, E>
    where
        T: Send,
    {
        let res = rayon::join(|| f(self.old), || f(self.new));
        Ok(OldNew {
            old: res.0?,
            new: res.1?,
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

impl<T, E> OldNew<Result<T, E>> {
    pub fn ok(self) -> OldNew<Option<T>> {
        self.map(Result::ok)
    }
}

// Residual carries both halves: each side is either its successful output or
// its inner residual. That way `from_residual` can rebuild a full OldNew<T>
// even when only one half failed (the successful half is re-wrapped via T::from_output).
pub struct OldNewResidual<T: Try>(
    ControlFlow<T::Residual, T::Output>,
    ControlFlow<T::Residual, T::Output>,
);

impl<T: Try> Try for OldNew<T> {
    type Output = OldNew<T::Output>;
    type Residual = OldNewResidual<T>;

    fn from_output(output: Self::Output) -> Self {
        OldNew {
            old: T::from_output(output.old),
            new: T::from_output(output.new),
        }
    }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        let old = self.old.branch();
        let new = self.new.branch();
        match (old, new) {
            (ControlFlow::Continue(old), ControlFlow::Continue(new)) => {
                ControlFlow::Continue(OldNew { old, new })
            }
            (old, new) => ControlFlow::Break(OldNewResidual(old, new)),
        }
    }
}

impl<T: Try> FromResidual<OldNewResidual<T>> for OldNew<T> {
    fn from_residual(residual: OldNewResidual<T>) -> Self {
        let rebuild = |cf: ControlFlow<T::Residual, T::Output>| match cf {
            ControlFlow::Continue(v) => T::from_output(v),
            ControlFlow::Break(r) => T::from_residual(r),
        };
        OldNew {
            old: rebuild(residual.0),
            new: rebuild(residual.1),
        }
    }
}

impl<T: Try> Residual<OldNew<T::Output>> for OldNewResidual<T> {
    type TryType = OldNew<T>;
}

// Lets `?` on OldNew<Result<U, E>> flow into ANY Result<X, F> (with F: From<E>) —
// e.g. a fn returning Result<()> can `?` through a OldNew of Results.
impl<U, E, X, F> FromResidual<OldNewResidual<Result<U, E>>> for Result<X, F>
where
    F: From<E>,
{
    fn from_residual(residual: OldNewResidual<Result<U, E>>) -> Self {
        let extract = |cf: ControlFlow<Result<std::convert::Infallible, E>, U>| match cf {
            ControlFlow::Continue(_) => None,
            ControlFlow::Break(Err(e)) => Some(e),
            ControlFlow::Break(Ok(never)) => match never {},
        };
        let err = extract(residual.0).or_else(|| extract(residual.1));
        Err(F::from(
            err.expect("residual must carry at least one error"),
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn try_returns_oldnew() -> Result<OldNew<i32>, &'static str> {
        let pair: OldNew<Result<i32, &'static str>> = OldNew::new(Ok(1), Ok(2));
        let unwrapped: OldNew<i32> = pair?;
        Ok(unwrapped.map(|x| x + 10))
    }

    fn try_propagates_error() -> Result<OldNew<i32>, &'static str> {
        let pair: OldNew<Result<i32, &'static str>> = OldNew::new(Err("old failed"), Ok(2));
        let unwrapped: OldNew<i32> = pair?;
        Ok(unwrapped)
    }

    #[test]
    fn try_success() {
        let r = try_returns_oldnew().unwrap();
        assert_eq!(r.old, 11);
        assert_eq!(r.new, 12);
    }

    #[test]
    fn try_failure() {
        let r = try_propagates_error();
        assert_eq!(r.unwrap_err(), "old failed");
    }
}
