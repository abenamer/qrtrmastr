//! Tests of the import use case with the real reader of a mail (REAS-TEST-004). The mail source
//! and the store are doubles, because control leaves the application there: a file or a
//! mailbox on one side, the database on the other. The classifier and the redactor are code of
//! this application and run for real. No database is needed.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::adapters::mail::redact::{RedactionTerms, Redactor};
use qrtrmastr::application::import::{ImportError, ImportMail};
use qrtrmastr::application::ports::{
    InsertOutcome, MailError, MailSource, MailStream, NewRawMessage, PendingMessage, RawMail,
    RawMessageStore, StoreError, StoredBody,
};
use qrtrmastr::domain::ids::{MessageKey, RawMessageId};
use qrtrmastr::domain::states::{KeyKind, MessageKind, Platform};

/// The reader of the program, with the made-up person of the synthetic receipts.
fn reader() -> PlatformMail {
    PlatformMail::new(
        vec![("service@freshdirect.com".to_owned(), Platform::Freshdirect)],
        Redactor::new(RedactionTerms {
            names: vec!["Pat Example".to_owned(), "Pat".to_owned()],
            emails: vec!["pat.example@example.com".to_owned()],
            phones: vec!["2125550199".to_owned()],
            address_lines: vec!["123 Imaginary Street Apt 4B".to_owned()],
            card_last4: vec!["4242".to_owned()],
        })
        .expect("terms"),
    )
}

/// One mail with an HTML body.
fn mail(from: &str, message_id: &str, body: &str) -> Result<RawMail, MailError> {
    Ok(RawMail {
        bytes: format!(
            "From: {from}\r\nSubject: Your FreshDirect order is in the works! Here's your receipt\r\n\
             Date: Tue, 3 Mar 2026 06:15:00 -0500\r\nMessage-ID: <{message_id}@mail.example>\r\n\
             Content-Type: text/html; charset=utf-8\r\n\r\n{body}"
        )
        .into_bytes(),
    })
}

/// A mail source that gives the items that it holds, in order.
struct Source(Vec<Result<RawMail, MailError>>);

impl MailSource for Source {
    fn messages(&self) -> MailStream {
        let (tx, rx) = tokio::sync::mpsc::channel(self.0.len().max(1));
        for item in &self.0 {
            let item = match item {
                Ok(mail) => Ok(mail.clone()),
                Err(MailError::Read(text)) => Err(MailError::Read(text.clone())),
                Err(other) => panic!("a source gives a read error only: {other}"),
            };
            tx.try_send(item).expect("the channel holds every item");
        }
        rx
    }
}

/// A store that keeps what is inserted, where the test can read it after the run.
#[derive(Default)]
struct MemoryStore {
    known: HashSet<MessageKey>,
    inserted: Rc<RefCell<Vec<NewRawMessage>>>,
}

impl RawMessageStore for MemoryStore {
    async fn known_keys(&self) -> Result<HashSet<MessageKey>, StoreError> {
        Ok(self.known.clone())
    }
    async fn newest_received_at(&self) -> Result<Option<jiff::Timestamp>, StoreError> {
        Ok(None)
    }
    async fn body_by_key(&self, _: &MessageKey) -> Result<Option<StoredBody>, StoreError> {
        Ok(None)
    }
    async fn insert(&self, message: &NewRawMessage) -> Result<InsertOutcome, StoreError> {
        self.inserted.borrow_mut().push(message.clone());
        Ok(InsertOutcome::Inserted(RawMessageId::new(1).expect("id")))
    }
    async fn pending(&self, _: Platform) -> Result<Vec<PendingMessage>, StoreError> {
        Ok(Vec::new())
    }
}

/// A store whose first call fails, as a database that is down.
struct BrokenStore;

impl RawMessageStore for BrokenStore {
    async fn known_keys(&self) -> Result<HashSet<MessageKey>, StoreError> {
        Err(StoreError::Migration("down".to_owned()))
    }
    async fn newest_received_at(&self) -> Result<Option<jiff::Timestamp>, StoreError> {
        unreachable!()
    }
    async fn body_by_key(&self, _: &MessageKey) -> Result<Option<StoredBody>, StoreError> {
        unreachable!()
    }
    async fn insert(&self, _: &NewRawMessage) -> Result<InsertOutcome, StoreError> {
        unreachable!("never reached after known_keys fails")
    }
    async fn pending(&self, _: Platform) -> Result<Vec<PendingMessage>, StoreError> {
        unreachable!()
    }
}

const GROCER: &str = "FreshDirect <service@freshdirect.com>";

#[tokio::test]
async fn known_messages_are_skipped_and_a_redaction_failure_is_reported_not_stored() {
    let inserted = Rc::new(RefCell::new(Vec::new()));
    let store = MemoryStore {
        known: HashSet::from([MessageKey::from_message_id("a@mail.example").expect("key")]),
        inserted: Rc::clone(&inserted),
    };
    let source = Source(vec![
        mail(GROCER, "a", "<p>Order 1</p>"),
        mail(
            GROCER,
            "b",
            "<p>Hi Pat Example, order 2 for 212-555-0199</p>",
        ),
        mail("Someone <someone@mail.example>", "c", "<p>hello</p>"),
        // The letters of the name with a tag between them: the passes of the redactor leave
        // this form, and its assertion finds it, so the message is refused.
        mail(GROCER, "leaky", "<p>P<b>a</b>t Example</p>"),
    ]);
    let report = ImportMail::new(source, reader(), store)
        .run()
        .await
        .expect("import");

    assert_eq!(report.read, 4);
    assert_eq!(report.inserted, 1);
    assert_eq!(
        report.inserted_by_platform,
        BTreeMap::from([(Platform::Freshdirect, 1)])
    );
    assert_eq!(report.already_known, 1);
    assert_eq!(report.other_sender, 1);
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);
    // The line of the refusal names the key and the kind of the term, and no text of the mail.
    assert_eq!(
        report.redaction_failures,
        vec!["message_id:leaky@mail.example: name".to_owned()]
    );
    assert_eq!(
        report.refused_by_cause,
        BTreeMap::from([(
            "freshdirect receipt, html body: name same letters and digits, other punctuation"
                .to_owned(),
            1
        )])
    );

    // One message reached the store: the new one, classified and redacted by the real reader.
    let inserted = inserted.borrow();
    assert_eq!(inserted.len(), 1);
    let stored = &inserted[0];
    assert_eq!(stored.key.kind(), KeyKind::MessageId);
    assert_eq!(stored.key.as_str(), "b@mail.example");
    assert_eq!(stored.platform, Platform::Freshdirect);
    assert_eq!(stored.kind, MessageKind::Receipt);
    let html = stored.body_html.as_deref().expect("an HTML body");
    assert!(html.contains("order 2"), "{html}");
    assert!(!html.contains("Pat") && !html.contains("0199"), "{html}");
}

#[tokio::test]
async fn a_store_failure_stops_the_run() {
    let source = Source(vec![
        mail(GROCER, "a", "<p>Order 1</p>"),
        mail(GROCER, "b", "<p>Order 2</p>"),
    ]);
    let result = ImportMail::new(source, reader(), BrokenStore).run().await;
    assert!(
        matches!(result, Err(ImportError::Store(StoreError::Migration(ref text))) if text == "down"),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_source_that_cannot_be_read_is_an_error_not_an_empty_report() {
    let inserted = Rc::new(RefCell::new(Vec::new()));
    let store = MemoryStore {
        known: HashSet::new(),
        inserted: Rc::clone(&inserted),
    };
    let source = Source(vec![
        mail(GROCER, "b", "<p>Order 2</p>"),
        Err(MailError::Read("/data/missing: no such file".to_owned())),
        mail(GROCER, "c", "<p>Order 3</p>"),
    ]);
    let result = ImportMail::new(source, reader(), store).run().await;
    assert!(
        matches!(result, Err(ImportError::Source(MailError::Read(ref text))) if text == "/data/missing: no such file"),
        "{result:?}"
    );
    // The message read before the failure is stored, and the one after it is not read.
    let keys: Vec<String> = inserted
        .borrow()
        .iter()
        .map(|message| message.key.as_str().to_owned())
        .collect();
    assert_eq!(keys, vec!["b@mail.example".to_owned()]);
}
