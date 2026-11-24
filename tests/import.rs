//! Tests of the import use case with the real reader of a mail (REAS-TEST-004). The mail source
//! and the store are doubles, because control leaves the application there: a file or a
//! mailbox on one side, the database on the other. The classifier and the redactor are code of
//! this application and run for real. No database is needed.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use qrtrmastr::adapters::mail::PlatformMail;
use qrtrmastr::application::import::{ImportError, ImportMail};
use qrtrmastr::application::ports::{
    InsertOutcome, MailError, MailSource, MailStream, NewRawMessage, PendingMessage, RawMail,
    RawMessageStore, StoreError, StoredBody,
};
use qrtrmastr::domain::ids::{MessageKey, RawMessageId};
use qrtrmastr::domain::states::{KeyKind, MessageKind, Platform};

/// The reader of the program, with the made-up person of the synthetic receipts.
fn reader() -> PlatformMail {
    PlatformMail::new(vec![(
        "service@freshdirect.com".to_owned(),
        Platform::Freshdirect,
    )])
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
async fn a_known_message_is_skipped_and_the_page_of_each_receipt_stays_out_of_the_store() {
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
    ]);
    let report = ImportMail::new(source, reader(), store)
        .run()
        .await
        .expect("import");

    assert_eq!(report.read, 3);
    assert_eq!(report.inserted, 1);
    assert_eq!(
        report.inserted_by_platform,
        BTreeMap::from([(Platform::Freshdirect, 1)])
    );
    assert_eq!(report.already_known, 1);
    assert_eq!(report.other_sender, 1);
    assert!(report.malformed.is_empty(), "{:?}", report.malformed);

    // One record reached the store: the new one, classified by the real reader. A record
    // has no field for a subject or a body, so nothing that the mail says is in it.
    let inserted = inserted.borrow();
    assert_eq!(inserted.len(), 1);
    let stored = &inserted[0];
    assert_eq!(stored.key.kind(), KeyKind::MessageId);
    assert_eq!(stored.key.as_str(), "b@mail.example");
    assert_eq!(stored.platform, Platform::Freshdirect);
    assert_eq!(stored.kind, MessageKind::Receipt);

    // The page of each receipt of the run is in the report for the parse step, also the one
    // of the known message, and not the mail of the other sender.
    assert_eq!(report.pages.len(), 2);
    let known = MessageKey::from_message_id("a@mail.example").expect("key");
    assert_eq!(report.pages.get(&known), Some(Some("<p>Order 1</p>")));
    let page = report.pages.get(&stored.key).flatten().expect("the page");
    assert!(page.contains("order 2"), "{page}");
    // The debug form of the report, which a failed assertion prints, shows no page.
    let debug = format!("{report:?}");
    assert!(
        debug.contains("ReceivedPages(2 pages, not shown)"),
        "{debug}"
    );
    assert!(
        !debug.contains("Pat Example") && !debug.contains("0199"),
        "{debug}"
    );
}

#[tokio::test]
async fn the_page_of_a_receipt_is_kept_only_for_a_platform_that_the_caller_names() {
    let store = || MemoryStore {
        known: HashSet::new(),
        inserted: Rc::new(RefCell::new(Vec::new())),
    };
    let source = || Source(vec![mail(GROCER, "a", "<p>Order 1</p>")]);
    // The grocer is named: its receipt has a page for the parser.
    let named = ImportMail::new(source(), reader(), store())
        .keeping_pages_of(&[Platform::Freshdirect])
        .run()
        .await
        .expect("import");
    assert_eq!((named.inserted, named.pages.len()), (1, 1));
    // Another platform is named: the receipt of the grocer is stored as a record, and its
    // page is not kept.
    let other = ImportMail::new(source(), reader(), store())
        .keeping_pages_of(&[Platform::Seamless])
        .run()
        .await
        .expect("import");
    assert_eq!((other.inserted, other.pages.len()), (1, 0));
    assert!(other.pages.is_empty());
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
