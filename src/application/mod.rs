//! Use cases and the ports they need (REAS-ARCH-001).
//!
//! A use case takes its adapters as type parameters bound by a port trait (REAS-ARCH-006), so
//! the concrete adapter is visible where it is constructed, in `src/main.rs`.

pub mod import;
pub mod list;
pub mod migrate;
pub mod parse;
pub mod ports;
pub mod recommend;

/// Adds one to a counter of a run report.
///
/// A counter counts the messages of one run, so it stays far below the bound of its type. The
/// addition is still not left unchecked (REAS-SEC-003): at the bound the count stays there,
/// and it does not wrap to a small number or stop the run.
pub(crate) fn count(counter: &mut usize) {
    *counter = counter.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn a_counter_goes_up_by_one_and_stays_at_the_bound() {
        let mut counter = 0;
        count(&mut counter);
        count(&mut counter);
        assert_eq!(counter, 2);
        let mut full = usize::MAX;
        count(&mut full);
        assert_eq!(full, usize::MAX);
    }
}
