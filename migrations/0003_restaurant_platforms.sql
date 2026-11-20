-- The restaurant platforms whose receipt mail the import stores (ADR 0007). The check
-- constraint is replaced by one that admits more values; every row it admitted before is
-- still admitted, so no data changes. The list matches `Platform` in src/domain/states.rs.
alter table raw_messages drop constraint raw_messages_platform_check;
alter table raw_messages add constraint raw_messages_platform_check
  check (platform in ('freshdirect', 'seamless', 'grubhub', 'toast', 'square', 'clover', 'doordash'));
