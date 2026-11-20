-- What the user adds to the next order by hand, and why (ADR 0011). The reasons match
-- `AddReason` in src/domain/states.rs. `product_id` is set when the text named exactly one
-- product of the catalog; a new food has none. An addition is open until an order is
-- delivered after the day it was added; nothing here records that, it is read from
-- grocery_orders.
create table list_additions (
  id          bigint generated always as identity primary key,
  item_text   text not null,
  product_id  bigint references products (id),
  reason      text not null,
  added_on    date not null,
  created_at  timestamptz not null default now(),
  constraint list_additions_item_text_check check (char_length(item_text) between 1 and 120),
  constraint list_additions_reason_check
    check (reason in ('out-of-stock', 'recipe', 'new-food', 'repeat-favorite'))
);
