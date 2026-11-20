//! Classification of a platform's mail: which platform the sender belongs to, which kind the
//! message is, and whether Gmail recorded a DKIM pass for it.

use std::sync::LazyLock;

use regex::Regex;

use crate::domain::states::{MessageKind, Platform};

fn rule(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a subject rule is a valid regex")
}

static FD_RECEIPT: LazyLock<Regex> =
    LazyLock::new(|| rule(r"(?i)here'?s your receipt|your receipt"));
static FD_UPDATED: LazyLock<Regex> =
    LazyLock::new(|| rule(r"(?i)order information has been updated|has been updated"));
static FD_PLACED: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)^\s*your order for\b"));
/// Seamless and Grubhub confirm an order with "Thanks for your <Restaurant> order".
static THANKS_FOR_ORDER: LazyLock<Regex> =
    LazyLock::new(|| rule(r"(?i)^\s*thanks for your\b.*\border\b"));
/// Square: "Receipt from <Merchant> #xxxx".
static RECEIPT_FROM: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)^\s*receipt from\b"));
/// Toast and Clover: no subject is on record yet, so the word itself is the rule (ADR 0007).
static RECEIPT_WORD: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)\breceipt\b"));
static DD_PLACED: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)^\s*order confirmation for\b"));
static DD_RECEIPT: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)^\s*final receipt\b"));
static DD_UPDATED: LazyLock<Regex> = LazyLock::new(|| rule(r"(?i)^\s*adjustment\b"));

fn receipt_or_unknown(is_receipt: bool) -> MessageKind {
    if is_receipt {
        MessageKind::Receipt
    } else {
        MessageKind::Unknown
    }
}

/// The kind of a platform's mail from its subject. Anything else is `Unknown`, which is stored:
/// a sender that also sends promotions (DoorDash, Square) is sorted out by kind, not dropped.
pub fn kind_from_subject(platform: Platform, subject: &str) -> MessageKind {
    match platform {
        Platform::Freshdirect => {
            if FD_RECEIPT.is_match(subject) {
                MessageKind::Receipt
            } else if FD_UPDATED.is_match(subject) {
                MessageKind::Updated
            } else if FD_PLACED.is_match(subject) {
                MessageKind::Placed
            } else {
                MessageKind::Unknown
            }
        }
        Platform::Seamless | Platform::Grubhub => {
            receipt_or_unknown(THANKS_FOR_ORDER.is_match(subject))
        }
        Platform::Square => receipt_or_unknown(RECEIPT_FROM.is_match(subject)),
        Platform::Toast | Platform::Clover => receipt_or_unknown(RECEIPT_WORD.is_match(subject)),
        Platform::Doordash => {
            if DD_RECEIPT.is_match(subject) {
                MessageKind::Receipt
            } else if DD_UPDATED.is_match(subject) {
                MessageKind::Updated
            } else if DD_PLACED.is_match(subject) {
                MessageKind::Placed
            } else {
                MessageKind::Unknown
            }
        }
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

/// The platform whose sender the From address is, by exact address. `senders` is lowercase.
/// A marketing address of the same company is another address and matches nothing.
pub fn platform_of(from: Option<&str>, senders: &[(String, Platform)]) -> Option<Platform> {
    let from = from?.trim().to_ascii_lowercase();
    senders
        .iter()
        .find(|(address, _)| *address == from)
        .map(|(_, platform)| *platform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        Platform::Freshdirect,
        "Your FreshDirect order is in the works! Here's your receipt",
        MessageKind::Receipt
    )]
    #[case(
        Platform::Freshdirect,
        "Your order information has been updated",
        MessageKind::Updated
    )]
    #[case(
        Platform::Freshdirect,
        "Your order for Tuesday, March 3",
        MessageKind::Placed
    )]
    #[case(Platform::Freshdirect, "20% off this week", MessageKind::Unknown)]
    #[case(
        Platform::Seamless,
        "Thanks for your Example Noodle Bar order",
        MessageKind::Receipt
    )]
    #[case(
        Platform::Grubhub,
        "Thanks for your Example Diner order",
        MessageKind::Receipt
    )]
    #[case(Platform::Grubhub, "An update on your refund", MessageKind::Unknown)]
    #[case(
        Platform::Seamless,
        "Your order for Tuesday, March 3",
        MessageKind::Unknown
    )]
    #[case(
        Platform::Square,
        "Receipt from Example Coffee #1a2B",
        MessageKind::Receipt
    )]
    #[case(
        Platform::Square,
        "Example Coffee sent you a receipt offer",
        MessageKind::Unknown
    )]
    #[case(
        Platform::Toast,
        "Your receipt from Example Bistro",
        MessageKind::Receipt
    )]
    #[case(Platform::Clover, "Receipt for your purchase", MessageKind::Receipt)]
    #[case(Platform::Clover, "Rate your visit", MessageKind::Unknown)]
    #[case(
        Platform::Doordash,
        "Order Confirmation for Pat from Example Tacos",
        MessageKind::Placed
    )]
    #[case(
        Platform::Doordash,
        "Final receipt for Pat from Example Tacos",
        MessageKind::Receipt
    )]
    #[case(Platform::Doordash, "Adjustment to your order", MessageKind::Updated)]
    #[case(Platform::Doordash, "50% off your next order", MessageKind::Unknown)]
    fn subjects_classify(
        #[case] platform: Platform,
        #[case] subject: &str,
        #[case] kind: MessageKind,
    ) {
        assert_eq!(kind_from_subject(platform, subject), kind);
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
        let senders = vec![
            ("service@freshdirect.com".to_owned(), Platform::Freshdirect),
            ("orders@eat.seamless.com".to_owned(), Platform::Seamless),
        ];
        assert_eq!(
            platform_of(Some(" Service@FreshDirect.com "), &senders),
            Some(Platform::Freshdirect)
        );
        assert_eq!(
            platform_of(Some("orders@eat.seamless.com"), &senders),
            Some(Platform::Seamless)
        );
        // The marketing address of the same company is not the orders address.
        assert_eq!(platform_of(Some("email@a.seamless.com"), &senders), None);
        assert_eq!(platform_of(Some("deals@a.freshdirect.com"), &senders), None);
        assert_eq!(platform_of(None, &senders), None);
    }
}
