use std::ops::Deref;

use welds::state::DbState;

use super::User;
use crate::{
	db::models::{AuditLog, JWT_EXPIRATION_TIME, JWT_SESSION_ID_KEY, Session},
	server::messages::Authentication,
	testutil::*,
};

#[tokio::test]
async fn audit_log() {
	let db = make_config(None, None)
		.await
		.unwrap()
		.get_db()
		.await
		.unwrap();

	let mut log = AuditLog {
		user_id: Some(1),
		endpoint: "http://localhost".into(),
		ip: "127.0.0.1".into(),
		..Default::default()
	};

	let log = log
		// just any struct that can serde should work here
		.with_data(Authentication {
			username: "erikh".into(),
			password: "testinglogs".into(),
		})
		.unwrap()
		.with_entry("this is a log message".into());

	for _ in 0..10 {
		log.complete(&db).await.unwrap();
		tokio::time::sleep(std::time::Duration::from_millis(100)).await;
	}

	let mut time = chrono::Local::now();

	for record in AuditLog::all().run(db.handle()).await.unwrap() {
		assert_ne!(record.time, time);
		time = record.time
	}
}

#[tokio::test]
async fn session_jwt() {
	let db = make_config(None, None)
		.await
		.unwrap()
		.get_db()
		.await
		.unwrap();

	let mut user = User::new();
	user.username = "erikh".into();
	assert!(user.set_password("horlclax".into()).is_ok());
	user.save(db.handle()).await.unwrap();
	let mut session = Session::new_assigned(user.deref());
	session.save(db.handle()).await.unwrap();
	let claims = session.to_jwt();
	assert_eq!(
		claims[JWT_SESSION_ID_KEY].parse::<u32>().unwrap(),
		session.id
	);
	assert_eq!(
		claims[JWT_EXPIRATION_TIME]
			.parse::<chrono::DateTime<chrono::Local>>()
			.unwrap(),
		session.expires,
	);

	let session2 = Session::from_jwt(&db, claims).await.unwrap();
	assert_eq!(session.into_inner(), session2.into_inner());
}

#[tokio::test]
async fn user_password() {
	let db = make_config(None, None)
		.await
		.unwrap()
		.get_db()
		.await
		.unwrap();

	let mut user = User::new();
	user.username = "erikh".into();
	assert!(user.set_password("horlclax".into()).is_ok());
	assert_ne!(user.password, "horlclax".to_string());
	assert!(user.save(&db.handle).await.is_ok());

	let user2 = User::find_by_id(&db.handle, user.id).await.unwrap();
	assert!(user2.is_some());
	let user2 = user2.unwrap();
	assert_eq!(user2.username, user.username);
	assert_eq!(user2.password, user.password);
	assert_eq!(user2.id, user.id);

	assert!(user.login("test".into()).is_err());
	assert!(user.login("horlclax".into()).is_ok());
}

#[tokio::test]
async fn user_basic() {
	let db = make_config(None, None)
		.await
		.unwrap()
		.get_db()
		.await
		.unwrap();

	let table: &mut [DbState<User>] = &mut [
		DbState::new_uncreated(User {
			id: 0,
			username: "erikh".into(),
			realname: Some("Erik Hollensbe".into()),
			email: Some("erikhollensbe@proton.me".into()),
			phone: Some("800-867-5309".into()),
			password: "".into(),
			plaintext_password: Some("horlclax".into()),
			deleted_at: None,
		}),
		DbState::new_uncreated(User {
			id: 0,
			username: "scarlett".into(),
			realname: Some("Scarlett Hollensbe".into()),
			email: Some("scarlett@hollensbe.org".into()),
			phone: None,
			password: "".into(),
			plaintext_password: Some("foobar".into()),
			deleted_at: None,
		}),
		DbState::new_uncreated(User {
			id: 0,
			username: "cmaujean".into(),
			realname: Some("Christopher Maujean".into()),
			email: Some("christopher@maujean.org".into()),
			phone: None,
			password: "".into(),
			plaintext_password: Some("pooprocket".into()),
			deleted_at: None,
		}),
		DbState::new_uncreated(User {
			id: 0,
			username: "day".into(),
			realname: Some("Day Waterbury".into()),
			email: None,
			phone: None,
			password: "".into(),
			plaintext_password: Some("mmph".into()),
			deleted_at: None,
		}),
		DbState::new_uncreated(User {
			id: 0,
			username: "dpnvektor".into(),
			realname: Some("Julian Sutter".into()),
			email: None,
			phone: None,
			password: "".into(),
			plaintext_password: Some("meh".into()),
			deleted_at: None,
		}),
	];

	for item in table.into_iter() {
		let pw = item.plaintext_password.clone().unwrap();
		item.set_password(pw).unwrap();
		assert_ne!(item.password.len(), 0);
		assert_ne!(item.password, item.plaintext_password.clone().unwrap(),);
		assert!(item.save(&db.handle).await.is_ok());
		assert_ne!(item.id, 0);
	}

	assert_eq!(User::all().count(&db.handle).await.unwrap(), 5);

	for item in table.into_iter() {
		assert_eq!(
			User::all()
				.where_col(|c| c.username.equal(&item.username))
				.run(&db.handle)
				.await
				.unwrap()
				.first()
				.unwrap()
				.id,
			item.id
		)
	}

	for item in table.into_iter() {
		assert!(item.delete(&db.handle).await.is_ok());
	}

	assert_eq!(User::all().count(&db.handle).await.unwrap(), 0);
}
