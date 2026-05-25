#![feature(try_trait_v2)]
#![feature(try_trait_v2_residual)]

pub mod old_new;
mod result;
mod text;

pub use old_new::{Changes, OldNew};
pub use result::DiffResult;
pub use text::{diff_text, diff_text_context};
