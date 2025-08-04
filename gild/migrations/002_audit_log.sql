create table audit_log (
  id integer primary key autoincrement,
  user_id integer,
  time timestamp not null,
  entry varchar not null,
  endpoint varchar not null,
  ip varchar not null,
  data varchar not null,
  error varchar
);

create index audit_log_time_idx on audit_log (time);
create index audit_log_entry_idx on audit_log (entry);
create index audit_log_endpoint_idx on audit_log (endpoint);
create index audit_log_ip_idx on audit_log (ip);
