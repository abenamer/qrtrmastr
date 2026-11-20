-- A fifth reason for an addition: `running-low`, the product is almost used up (ADR 0011).
-- `out-of-stock` means the user ran out at home. The check constraint is replaced by one that
-- admits one more value; every row it admitted before is still admitted. The list matches
-- `AddReason` in src/domain/states.rs.
alter table list_additions drop constraint list_additions_reason_check;
alter table list_additions add constraint list_additions_reason_check
  check (reason in ('out-of-stock', 'running-low', 'recipe', 'new-food', 'repeat-favorite'));
