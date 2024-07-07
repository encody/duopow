create table if not exists users (id integer primary key, username text not null, signing_key_hex text not null, verified bool default false);

create unique index if not exists users_username_uidx on users (username);
