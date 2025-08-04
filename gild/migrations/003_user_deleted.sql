alter table users add column deleted_at timestamp;
create index users_deleted_at_idx on users (deleted_at);
