create table users (
  user_id integer primary key autoincrement,
  username varchar not null,
  realname varchar,
  email varchar,
  phone varchar,
  password varchar not null,
  UNIQUE(username)
);

create table sessions (
  session_id integer primary key autoincrement,
  expires datetime not null,
  user_id integer not null
);
