-- Grocery orders as the receipt states them, their lines, and the product identity the lines
-- resolve to. A line stores the alias it matched, not the product, so merging two products is
-- one reversible update on product_aliases and never touches order data.
create table grocery_orders (
  order_number            text primary key,               -- 11 or 13 digits; text, never numeric
  delivery_date           date not null,
  delivery_window         text,
  placed_at               timestamptz,                    -- from the placed mail when present
  receipt_raw_message_id  bigint not null references raw_messages (id),
  parser_name             text not null,
  parser_version          int  not null,
  subtotal        numeric(10,2) not null,
  tax             numeric(10,2),
  bottle_deposit  numeric(10,2),
  delivery_fee    numeric(10,2),
  service_fee     numeric(10,2),
  tip             numeric(10,2),
  total           numeric(10,2) not null,
  reconciled      boolean not null,                       -- the line sum matched the subtotal
  parsed_at       timestamptz not null default now()
);

-- Serves: order cadence and every purchase-history query ordered by date.
create index grocery_orders_delivery_date_idx on grocery_orders (delivery_date);

create table products (
  id            bigint generated always as identity primary key,
  display_name  text not null,
  size_text     text,
  sold_by       text not null,
  department    text,
  created_at    timestamptz not null default now(),
  constraint products_sold_by_check check (sold_by in ('each', 'weight'))
);

create table product_aliases (
  id                   bigint generated always as identity primary key,
  name_norm            text not null,
  size_norm            text not null default '',
  product_url_path     text,                              -- if the receipt links the product
  product_id           bigint not null references products (id),
  source               text not null,
  first_seen_raw_name  text not null,
  unique (name_norm, size_norm),
  constraint product_aliases_source_check check (source in ('auto', 'manual'))
);

-- Serves: the purchases view join, merge and split.
create index product_aliases_product_idx on product_aliases (product_id);

-- pg_trgm is a trusted extension: the database owner can create it without superuser rights.
create extension if not exists pg_trgm;

-- Serves: qm alias suggest, strict_word_similarity over normalized names.
create index product_aliases_name_trgm_idx on product_aliases using gist (name_norm gist_trgm_ops);

create table grocery_order_lines (
  id              bigint generated always as identity primary key,
  order_number    text not null references grocery_orders (order_number) on delete cascade,
  line_no         int  not null,
  department      text,
  raw_name        text not null,
  alias_id        bigint not null references product_aliases (id),
  qty_first       numeric(10,3),                          -- left of "2/1.56", verbatim
  qty_second      numeric(10,3),                          -- right of "2/1.56", verbatim
  weight          numeric(10,3),
  weight_unit     text,
  unit_price      numeric(10,2),
  unit_price_per  text,                                   -- ea | lb, as printed
  options_price   numeric(10,2),
  final_price     numeric(10,2) not null,
  flags           text[] not null default '{}',           -- T, S, D as printed
  billed_qty      numeric(10,3) not null,                 -- weight when weighed, else qty_second
  unique (order_number, line_no)
);

-- Serves: the purchases view (lines by alias) and merge impact counts.
create index grocery_order_lines_alias_idx on grocery_order_lines (alias_id);

-- Rows the parser could not classify. The order is still stored; nothing is dropped.
create table order_line_anomalies (
  id              bigint generated always as identity primary key,
  raw_message_id  bigint not null references raw_messages (id),
  order_number    text,
  row_index       int  not null,
  raw_cells       jsonb not null,
  reason          text not null,
  resolved        boolean not null default false
);

create table product_preferences (
  product_id  bigint primary key references products (id),
  muted       boolean not null default false,
  note        text,
  updated_at  timestamptz not null default now()
);

-- What the recommendation reads. Product identity is resolved through the alias at query time.
create view purchases as
select p.id as product_id,
       o.order_number,
       o.delivery_date,
       l.billed_qty,
       l.final_price,
       l.flags,
       l.raw_name
from grocery_order_lines l
join product_aliases a on a.id = l.alias_id
join products p on p.id = a.product_id
join grocery_orders o on o.order_number = l.order_number;
