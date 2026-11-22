//! Fuzz target for the size-text reader (REAS-TEST-003): `domain::pack::pack_amount`, which
//! reads the size that a receipt prints after a product name.

#![no_main]

use libfuzzer_sys::fuzz_target;
use qrtrmastr::domain::pack::pack_amount;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let amount = pack_amount(&text);
    // Property: an amount is above zero.
    if let Some(pack) = amount {
        assert!(pack.amount.is_sign_positive() && !pack.amount.is_zero(), "{pack:?} from {text:?}");
    }
    // Property: the case of an ASCII letter and a space at an end change nothing. Other
    // letters are not tried: for some of them a change of case is a change of the letter.
    assert_eq!(amount, pack_amount(&text.to_ascii_uppercase()));
    assert_eq!(amount, pack_amount(&format!("  {text}  ")));
});
