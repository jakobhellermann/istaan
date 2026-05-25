use diffy::{DiffOptions, PatchFormatter};

use crate::old_new::OldNew;

pub fn diff_text(data: OldNew<&str>, context_len: usize) -> String {
    diff_text_context(data, context_len)
}

pub fn diff_text_context(data: OldNew<&str>, context_len: usize) -> String {
    let len = data.map(str::len).max();
    let threshold = 1024 * 1024;
    let diff = len < threshold;

    if diff {
        // let context_len = usize::MAX;

        let patch = DiffOptions::new()
            .set_context_len(context_len)
            .create_patch(data.old, data.new);
        let text = PatchFormatter::new()
            .missing_newline_message(false)
            .fmt_patch(&patch)
            .to_string();
        text.lines().skip(2).collect::<Vec<_>>().join("\n")
    } else {
        format!("old: {}\nnew: {}", data.old, data.new)
    }
}
