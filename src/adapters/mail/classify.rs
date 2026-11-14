//! Classification of a FreshDirect mail: which sender it is from, which of the three kinds it
//! is, and whether Gmail recorded a DKIM pass for it.

use std::sync::LazyLock;

use regex::Regex;

use crate::domain::states::MessageKind;

static RECEIPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)here'?s your receipt|your receipt").expect("regex"));
static UPDATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)order information has been updated|has been updated").expect("regex")
});
static PLACED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*your order for\b").expect("regex"));

/// The kind of a FreshDirect mail from its subject. Anything else is `Unknown`, which is stored.
pub fn kind_from_subject(subject: &str) -> MessageKind {
    if RECEIPT.is_match(subject) {
        MessageKind::Receipt
    } else if UPDATED.is_match(subject) {
        MessageKind::Updated
    } else if PLACED.is_match(subject) {
        MessageKind::Placed
    } else {
        MessageKind::Unknown
    }
}

/// Whether an `Authentication-Results` header that Gmail added (authserv-id `mx.google.com`)
/// says a DKIM signature verified for the sender's domain or one of its subdomains. A header
/// another server added, or a pass for an unrelated signing domain, does not count: a forged
/// sender with its own valid signature would otherwise pass.
pub fn dkim_passed(results: &[String], sender_domain: &str) -> bool {
    let domain = sender_domain.trim().to_ascii_lowercase();
    results.iter().any(|result| {
        let lower = result.to_ascii_lowercase();
        let authserv = lower.split(';').next().unwrap_or("").trim();
        if !authserv.starts_with("mx.google.com") {
            return false;
        }
        lower.split(';').any(|clause| {
            let clause = clause.trim();
            clause.starts_with("dkim=pass")
                && clause.split_whitespace().any(|token| {
                    let signed = token
                        .strip_prefix("header.i=")
                        .or_else(|| token.strip_prefix("header.d="))
                        .map(|v| v.trim_start_matches('@'));
                    signed.is_some_and(|d| d == domain || d.ends_with(&format!(".{domain}")))
                })
        })
    })
}

/// The domain part of a sender address, lowercase.
pub fn domain_of(address: &str) -> Option<String> {
    address
        .rsplit_once('@')
        .map(|(_, d)| d.trim().to_ascii_lowercase())
}

/// Whether the From address is one of the platform's senders. `allowed` is lowercase.
pub fn sender_matches(from: Option<&str>, allowed: &[String]) -> bool {
    from.map(|f| f.trim().to_ascii_lowercase())
        .is_some_and(|f| allowed.contains(&f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        "Your FreshDirect order is in the works! Here's your receipt",
        MessageKind::Receipt
    )]
    #[case("Your order information has been updated", MessageKind::Updated)]
    #[case("Your order for Tuesday, March 3", MessageKind::Placed)]
    #[case("20% off this week", MessageKind::Unknown)]
    fn subjects_classify(#[case] subject: &str, #[case] kind: MessageKind) {
        assert_eq!(kind_from_subject(subject), kind);
    }

    #[test]
    fn dkim_and_sender_rules() {
        let google_pass = "mx.google.com;\n       dkim=pass header.i=@freshdirect.com header.s=s1 header.b=abc;\n       spf=pass".to_owned();
        assert!(dkim_passed(
            std::slice::from_ref(&google_pass),
            "freshdirect.com"
        ));
        assert!(dkim_passed(
            &["mx.google.com; dkim=pass header.d=mail.freshdirect.com".to_owned()],
            "freshdirect.com"
        ));
        // Another signer, a forged header from another server, a fail, and no header at all.
        assert!(!dkim_passed(
            &["mx.google.com; dkim=pass header.i=@attacker.example".to_owned()],
            "freshdirect.com"
        ));
        assert!(!dkim_passed(
            &["evil.example; dkim=pass header.i=@freshdirect.com".to_owned()],
            "freshdirect.com"
        ));
        assert!(!dkim_passed(
            &["mx.google.com; dkim=fail header.i=@freshdirect.com".to_owned()],
            "freshdirect.com"
        ));
        assert!(!dkim_passed(&[], "freshdirect.com"));
        assert_eq!(
            domain_of("Service@FreshDirect.com"),
            Some("freshdirect.com".to_owned())
        );
        assert_eq!(domain_of("nobody"), None);
        let allowed = vec!["service@freshdirect.com".to_owned()];
        assert!(sender_matches(Some(" Service@FreshDirect.com "), &allowed));
        assert!(!sender_matches(Some("deals@a.freshdirect.com"), &allowed));
        assert!(!sender_matches(None, &allowed));
    }
}
