//! Fuzz target for the inbound adapter (REAS-TEST-003).
//!
//! Budgets live in `.github/workflows/docs.yml`: 30 seconds on each push,
//! from the seed corpus in `corpus/receive/`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use reas_trust_boundary::checksum;
use reas_trust_boundary::doubles::XorTagVerifier;
use reas_trust_boundary::frame::Frame;
use reas_trust_boundary::inbound::receive;

fuzz_target!(|data: &[u8]| {
    // Property 1: the adapter returns a value or a typed error. A panic
    // fails the target.
    let verifier = XorTagVerifier::new(0x5a);
    let _ = receive(data, &verifier);

    // Property 2: a parsed frame encodes back to the input bytes.
    if let Ok(frame) = Frame::parse(data) {
        assert_eq!(frame.to_bytes(), data);
    }

    // Property 3: the duplicate checksum equals the reference (REAS-SEC-005).
    assert_eq!(checksum::fast(data), checksum::reference(data));
});
