-- Whether the receipt an order was read from had a DKIM pass that Gmail recorded for the
-- sender's domain. A receipt without one is parsed all the same and its order is tagged
-- 'non-verified' (ADR 0008). The values match `SenderCheck` in src/domain/states.rs.
alter table grocery_orders add column sender_check text not null default 'verified';
alter table grocery_orders add constraint grocery_orders_sender_check_check
  check (sender_check in ('verified', 'non-verified'));
