-- Every mail the importer has seen, redacted, and the record of each parse attempt over it.
-- The unredacted original stays in the Takeout export and in Gmail; this table holds the
-- redacted HTML so a better parser can be run over old mail without touching the mailbox.
create table raw_messages (
  id                 bigint generated always as identity primary key,
  platform           text not null default 'freshdirect',
  message_key        text not null unique,                 -- Message-ID header, else sha256 hex
  key_kind           text not null,
  received_at        timestamptz not null,                 -- the Date header
  subject            text not null,
  kind               text not null,
  dkim_pass          boolean not null,                     -- parsers refuse a message without it
  order_number       text,                                 -- filled when a parser finds one
  body_html          text,                                 -- redacted
  body_text          text,                                 -- redacted, nullable
  source_sha256      bytea not null unique,                -- hash of the unredacted bytes
  redaction_version  int  not null,
  parse_state        text not null default 'fetched',
  version            int  not null default 0,              -- bumped by every state transition
  imported_at        timestamptz not null default now(),
  constraint raw_messages_platform_check    check (platform in ('freshdirect')),
  constraint raw_messages_key_kind_check    check (key_kind in ('message_id', 'sha256')),
  constraint raw_messages_kind_check        check (kind in ('placed', 'updated', 'receipt', 'unknown')),
  constraint raw_messages_parse_state_check check (parse_state in ('fetched', 'parsed', 'partial', 'failed', 'unsupported'))
);

-- Serves: ParsePending (parse_state = 'fetched' in received order) and Review by state and date.
create index raw_messages_state_received_idx on raw_messages (platform, parse_state, received_at);

-- Serves: duplicate receipts for one order and placed mail with no receipt, both by order number.
create index raw_messages_order_number_idx on raw_messages (order_number);

create table parse_attempts (
  id              bigint generated always as identity primary key,
  raw_message_id  bigint not null references raw_messages (id),
  parser_name     text not null,
  parser_version  int  not null,
  outcome         text not null,
  error           text,
  anomaly_count   int  not null default 0,
  attempted_at    timestamptz not null default now(),
  constraint parse_attempts_outcome_check check (outcome in ('ok', 'partial', 'failed', 'unsupported'))
);

-- Serves: Review (the latest attempt per message) and ReparseStale (attempts by parser version).
create index parse_attempts_message_idx on parse_attempts (raw_message_id, attempted_at desc);
