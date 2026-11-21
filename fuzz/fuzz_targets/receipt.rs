//! Fuzz target for the receipt parser (REAS-TEST-003): `Registry::parse`, which reads the
//! stored HTML body of a receipt, and the checks of the domain that read its result.

#![no_main]

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::receipts::registry::Registry;
use qrtrmastr::application::ports::{ParseAttempt, ReceiptParsing};
use qrtrmastr::domain::receipt::reconcile;

thread_local! {
    // The registry holds its parsers behind a pointer that is not shared between threads.
    static REGISTRY: Registry = Registry::default();
}

fuzz_target!(|data: &[u8]| {
    let html = String::from_utf8_lossy(data);
    let attempt = REGISTRY.with(|registry| registry.parse(&html));
    // Property: one text gives one result.
    assert_eq!(attempt, REGISTRY.with(|registry| registry.parse(&html)));
    match attempt {
        ParseAttempt::Unsupported => {}
        // Property: a failure says what failed.
        ParseAttempt::Failed { error, .. } => assert!(!error.is_empty()),
        ParseAttempt::Parsed { receipt, .. } => {
            // Property: a receipt has item lines, numbered from 1 without a gap.
            assert!(!receipt.lines.is_empty());
            for (expected, line) in (1..).zip(&receipt.lines) {
                assert_eq!(line.line_no, expected);
                // The quantity that is billed and the kind of sale are read without a stop.
                let _ = line.billed_qty();
                let _ = line.sold_by();
            }
            // Property: the checks of the domain give a result for any receipt that the
            // parser gives: no sum of it stops the program.
            let reconciliation = reconcile(&receipt);
            let _ = reconciliation.reconciled();
        }
    }
});
