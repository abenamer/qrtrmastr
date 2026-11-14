---
paths:
  - "src/domain/**"
---

# The domain layer

- Pure. No SQLx, file, network, Tokio or clock reads here: a function takes values and slices and returns values. The application layer loads the data and passes `as_of`; an adapter serializes the result.
- Identifiers are the newtypes in `domain::ids` (`OrderNumber`, `MessageKey`, `RawMessageId`, `ProductId`, `AliasId`), never a bare `i64` or `String`.
- Money and quantities are `domain::money::Money` and `Quantity` over `rust_decimal`; floating point never touches a price or a weight.
- A lifecycle or a stored label is an enum in `domain::states` made with `text_enum!`, which gives it `as_str`, `FromStr`, `Display` and `ALL`; a new variant needs the check constraint that admits it (see the migrations rule). Transitions are functions that return a typed error when the state does not permit them; match every variant, no wildcard arm.
- No `unwrap` or `expect` outside tests, apart from an invariant the code itself guarantees, with the reason in the message.
- Doc comments say why: what the value or rule means and what it assumes, not a restatement of the code.
- Unit tests live in the module, named as sentences in snake_case (`a_price_times_a_weight_rounds_to_the_cent_like_the_receipt`), each checking a few related facts with hand-computed values. Run them with `cargo test --lib <module>`.
