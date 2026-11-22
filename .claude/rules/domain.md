---
paths:
  - "src/domain/**"
---

# The domain layer

- Pure. No SQLx, file, network, Tokio or clock reads here: a function takes values and slices and returns values. The application layer loads the data and passes `as_of`; an adapter serializes the result. No serde here and none in `src/application`: no derive, no attribute, no `impl`. `scripts/check.sh layers` fails on one, and on any dependency that the script does not list for the layer; to let a layer use a new library, add it to the list in `scripts/check.sh` and in `scripts/check.test.sh`, and say why in the pull request.
- Identifiers are the newtypes in `domain::ids` (`OrderNumber`, `MessageKey`, `RawMessageId`, `ProductId`, `AliasId`), never a bare `i64` or `String`. Their fields are private; a value read back from the store or typed by the user goes through the checked constructor (`MessageKey::from_parts`, `ProductId::new`).
- Money and quantities are `domain::money::Money` and `Quantity` over `rust_decimal`; floating point never touches a price or a weight, and Clippy refuses float arithmetic in the whole package. A `Money` holds its currency and is at most 99,999,999.99 on either side of zero; a `Quantity` is below 10,000,000. These are the bounds of the columns, and inside them each sum and product is exact. Add, subtract and compare with `checked_add`, `checked_sub`, `distance` and `checked_cmp`, which give an error for two currencies and, except the last, for a result outside the range; do not add a method that saturates or that orders two amounts without the check. `Money::times` is the one place that rounds, half away from zero; a new rounding point names its mode.
- A lifecycle or a stored label is an enum in `domain::states` made with `text_enum!`, which gives it `as_str`, `FromStr`, `Display` and `ALL`; a new variant needs the check constraint that admits it (see the migrations rule). Transitions are functions that return a typed error when the state does not permit them; match every variant, no wildcard arm.
- No `unwrap` or `expect` outside tests, apart from an invariant the code itself guarantees, with the reason in the message.
- Doc comments say why: what the value or rule means and what it assumes, not a restatement of the code.
- Unit tests live in the module, named as sentences in snake_case (`a_price_times_a_weight_rounds_to_the_cent_like_the_receipt`), each checking a few related facts with hand-computed values. Run them with `cargo test --lib <module>`.
