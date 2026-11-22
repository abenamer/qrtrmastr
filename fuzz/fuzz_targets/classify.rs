//! Fuzz target for the classifier (REAS-TEST-003): the functions of `mail::classify`, which
//! read a subject, a sender address and the `Authentication-Results` headers of a mail.
//!
//! The input is text: its first line is the subject, its second the address, and each other
//! line one header of results.

#![no_main]

use libfuzzer_sys::fuzz_target;
use qrtrmastr::adapters::mail::classify::{dkim_passed, domain_of, kind_from_subject, platform_of};
use qrtrmastr::domain::states::Platform;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut lines = text.split('\n');
    let subject = lines.next().unwrap_or("");
    let address = lines.next().unwrap_or("");
    let results: Vec<String> = lines.map(str::to_owned).collect();

    // Each platform classifies any subject without a stop. No property is asserted here: a
    // kind is one of the variants of its type.
    for platform in Platform::ALL {
        let _ = kind_from_subject(*platform, subject);
    }

    // Property: a domain is in lower case, has no space at an end, and holds no `@`.
    let domain = domain_of(address);
    if let Some(domain) = &domain {
        assert_eq!(domain, &domain.trim().to_ascii_lowercase());
        assert!(!domain.contains('@'), "{domain:?}");
    } else {
        assert!(!address.contains('@'));
    }

    // Property: only a configured sender address gives a platform, and no address gives none.
    let senders = vec![
        ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
        (address.trim().to_ascii_lowercase(), Platform::Seamless),
    ];
    assert_eq!(platform_of(None, &senders), None);
    if let Some(platform) = platform_of(Some(address), &senders) {
        let wanted = address.trim().to_ascii_lowercase();
        assert!(
            senders
                .iter()
                .any(|(configured, of)| *configured == wanted && *of == platform)
        );
    }
    assert_eq!(platform_of(Some(address), &[]), None);

    // Property: a pass needs a header of the trusted server that says `dkim=pass`, and it is
    // a pass for a domain: no results, and no domain, give no pass.
    assert!(!dkim_passed(&[], domain.as_deref().unwrap_or("example.com")));
    assert!(
        !dkim_passed(&results, ""),
        "a pass for no domain: {results:?}"
    );
    assert!(!dkim_passed(&results, " "));
    if let Some(domain) = &domain {
        if dkim_passed(&results, domain) {
            // The header that gives the pass names the domain of the sender.
            assert!(results.iter().any(|result| {
                let lower = result.to_ascii_lowercase();
                lower.trim_start().starts_with("mx.google.com")
                    && lower.contains("dkim=pass")
                    && lower.contains(domain.as_str())
            }));
        }
    }
});
