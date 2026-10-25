//! Identifier counters for one document build.
//!
//! The upstream writer numbered relationships, footnotes, pictures and revision marks from
//! process-wide counters. That made output differ from run to run, and let one conversion
//! observe how much work other conversions in the same process had done. The counters now
//! live per thread and an [`IdScope`] resets them for the duration of one conversion (a
//! conversion runs on one thread), so identical input produces identical identifiers.

use std::cell::Cell;

#[derive(Clone, Copy)]
struct Counters {
    pic: usize,
    docpr: usize,
    hyperlink: usize,
    footnote: usize,
    bookmark: usize,
    header: usize,
    history: usize,
    toc: usize,
    para: usize,
}

const FRESH: Counters = Counters {
    pic: 1,
    docpr: 1,
    hyperlink: 1,
    footnote: 1,
    bookmark: 1,
    header: 1,
    history: 0,
    toc: 0,
    para: 1,
};

thread_local! {
    static COUNTERS: Cell<Counters> = const { Cell::new(FRESH) };
}

/// Restarts every counter at its initial value; the previous values come back on drop, so
/// nested or re-entrant conversions on one thread do not disturb each other.
pub(crate) struct IdScope(Counters);

impl IdScope {
    pub(crate) fn new() -> Self {
        IdScope(COUNTERS.replace(FRESH))
    }
}

impl Drop for IdScope {
    fn drop(&mut self) {
        COUNTERS.set(self.0);
    }
}

fn next(pick: impl FnOnce(&mut Counters) -> &mut usize) -> usize {
    let mut c = COUNTERS.get();
    let slot = pick(&mut c);
    let id = *slot;
    *slot = id.wrapping_add(1);
    COUNTERS.set(c);
    id
}

pub(crate) fn docpr() -> usize {
    next(|c| &mut c.docpr)
}
pub(crate) fn pic() -> usize {
    next(|c| &mut c.pic)
}
pub(crate) fn hyperlink() -> usize {
    next(|c| &mut c.hyperlink)
}
pub(crate) fn footnote() -> usize {
    next(|c| &mut c.footnote)
}
pub(crate) fn bookmark() -> usize {
    next(|c| &mut c.bookmark)
}
pub(crate) fn header() -> usize {
    next(|c| &mut c.header)
}
pub(crate) fn history() -> usize {
    next(|c| &mut c.history)
}
pub(crate) fn toc() -> usize {
    next(|c| &mut c.toc)
}
pub(crate) fn para() -> usize {
    next(|c| &mut c.para)
}
