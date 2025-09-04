mod systemd {
    use crate::{
        db::models::User,
        server::messages::*,
        testutil::{start_server, TestClient},
    };

    #[tokio::test]
    async fn list() {
        let mut client = TestClient::new(start_server(None).await.unwrap());
        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        assert!(client
            .post::<Option<String>, buckle::systemd::Unit>("/systemd/list", None)
            .await
            .is_err());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let list = client
            .post::<Option<String>, Vec<buckle::systemd::Unit>>("/systemd/list", None)
            .await
            .unwrap();

        assert!(!list.is_empty());
        assert_eq!(
            list.iter()
                .filter(|x| x.name == "network.target")
                .collect::<Vec<&buckle::systemd::Unit>>()
                .len(),
            1
        );

        let list = client
            .post::<Option<String>, Vec<buckle::systemd::Unit>>(
                "/systemd/list",
                Some("network.target".into()),
            )
            .await
            .unwrap();

        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn log() {
        let mut client = TestClient::new(start_server(None).await.unwrap());
        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        assert!(client
            .post::<Option<String>, buckle::systemd::Unit>("/systemd/log", None)
            .await
            .is_err());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let list = client
            .post::<Option<String>, Vec<buckle::systemd::Unit>>("/systemd/list", None)
            .await
            .unwrap();

        assert!(!list.is_empty());
        assert_eq!(
            list.iter()
                .filter(|x| x.name == "network.target")
                .collect::<Vec<&buckle::systemd::Unit>>()
                .len(),
            1
        );

        let list = client
            .post::<Option<String>, Vec<buckle::systemd::Unit>>(
                "/systemd/list",
                Some("network.target".into()),
            )
            .await
            .unwrap();

        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn set_unit() {
        // FIXME: not sure how to test this without potentially harmful side effects yet
    }
}

mod packages {
    use charon::{
        Input, InputType, PackageTitle, Prompt, PromptCollection, PromptResponse, PromptResponses,
    };

    use crate::{
        db::models::User,
        server::messages::*,
        testutil::{start_server, TestClient},
    };

    #[tokio::test]
    async fn get_prompts() {
        let mut client = TestClient::new(start_server(None).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        assert!(client
            .post::<PackageTitle, PromptCollection>(
                "/packages/prompts",
                PackageTitle {
                    name: "with-prompts".into(),
                    version: "0.0.1".into()
                }
            )
            .await
            .is_err());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let collection = client
            .post::<PackageTitle, PromptCollection>(
                "/packages/prompts",
                PackageTitle {
                    name: "with-prompts".into(),
                    version: "0.0.1".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            collection,
            PromptCollection(vec![
                Prompt {
                    template: "private_path".into(),
                    question: "Where do you want this mounted?".into(),
                    input_type: InputType::String,
                },
                Prompt {
                    template: "private_size".into(),
                    question: "How big should it be?".into(),
                    input_type: InputType::Integer,
                },
                Prompt {
                    template: "private_recreate".into(),
                    question: "Should we recreate this volume if it already exists?".into(),
                    input_type: InputType::Boolean,
                },
            ])
        )
    }

    #[tokio::test]
    async fn set_responses() {
        let responses = PromptResponses(vec![
            PromptResponse {
                input: Input::String("/tmp/volroot".into()),
                template: "private_path".into(),
            },
            PromptResponse {
                input: Input::Integer(8675309),
                template: "private_size".into(),
            },
            PromptResponse {
                input: Input::Boolean(false),
                template: "private_recreate".into(),
            },
        ]);

        let mut client = TestClient::new(start_server(None).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        assert!(client
            .post::<PromptResponsesWithName, ()>(
                "/packages/set_responses",
                PromptResponsesWithName {
                    name: "with-prompts".into(),
                    responses: responses.clone(),
                }
            )
            .await
            .is_err());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        assert!(client
            .post::<PromptResponsesWithName, ()>(
                "/packages/set_responses",
                PromptResponsesWithName {
                    name: "with-prompts".into(),
                    responses: responses.clone(),
                }
            )
            .await
            .is_ok());

        assert_eq!(
            responses,
            client
                .post::<charon::PackageTitle, charon::PromptResponses>(
                    "/packages/get_responses",
                    charon::PackageTitle {
                        name: "with-prompts".into(),
                        version: "".into(),
                    }
                )
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn install() {
        let (pool, file) = buckle::testutil::create_zpool("gild-install").unwrap();

        let mut client = TestClient::new(start_server(Some(pool)).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };

        client.put::<User, User>("/users", login).await.unwrap();

        assert!(client
            .post::<PackageTitle, ()>(
                "/packages/install",
                PackageTitle {
                    name: "podman-test".into(),
                    version: "0.0.1".into(),
                }
            )
            .await
            .is_err());

        assert!(client
            .post::<PackageTitle, ()>(
                "/packages/uninstall",
                PackageTitle {
                    name: "podman-test".into(),
                    version: "0.0.1".into(),
                }
            )
            .await
            .is_err());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        client
            .post::<PackageTitle, ()>(
                "/packages/install",
                PackageTitle {
                    name: "podman-test".into(),
                    version: "0.0.1".into(),
                },
            )
            .await
            .unwrap();

        client
            .post::<PackageTitle, bool>(
                "/packages/installed",
                PackageTitle {
                    name: "podman-test".into(),
                    version: "0.0.1".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            client
                .get::<Vec<PackageTitle>>("/packages/list_installed")
                .await
                .unwrap(),
            vec![PackageTitle {
                name: "podman-test".into(),
                version: "0.0.1".into()
            }]
        );

        client
            .post::<PackageTitle, ()>(
                "/packages/uninstall",
                PackageTitle {
                    name: "podman-test".into(),
                    version: "0.0.1".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            client
                .get::<Vec<PackageTitle>>("/packages/list_installed")
                .await
                .unwrap(),
            vec![]
        );

        let _ = buckle::testutil::destroy_zpool("gild-install", Some(&file));
    }
}

mod service {
    use buckle::client::Info;

    use crate::{
        db::models::User,
        server::messages::*,
        testutil::{start_server, TestClient},
    };

    #[tokio::test]
    async fn ping() {
        let mut client = TestClient::new(start_server(None).await.unwrap());
        let results = client.get::<PingResult>("/status/ping").await.unwrap();
        assert!(results.info.is_none());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let results = client.get::<PingResult>("/status/ping").await.unwrap();
        assert!(results.info.is_some());
        let info: Info = results.info.unwrap().into();
        assert_ne!(info.uptime, 0);
        assert_ne!(info.available_memory, 0);
        assert_ne!(info.total_memory, 0);
        assert_ne!(info.cpus, 0);
        assert_ne!(info.cpu_usage, 0.0);
        assert!(!info.host_name.is_empty());
        assert!(!info.kernel_version.is_empty());
        assert_ne!(info.load_average, [0.0, 0.0, 0.0]);
        assert_ne!(info.processes, 0);
    }
}

mod user {
    use crate::db::models::User;
    use crate::server::messages::Authentication;
    use crate::testutil::{start_server, TestClient};

    #[tokio::test]
    async fn login_logout() {
        let mut client = TestClient::new(start_server(None).await.unwrap());

        assert!(client.post::<(), Vec<User>>("/users", ()).await.is_err());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        assert!(client.post::<(), Vec<User>>("/users", ()).await.is_ok());
    }

    #[tokio::test]
    async fn first_time_setup() {
        let mut client = TestClient::new(start_server(None).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        let login = User {
            username: "test-login2".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_err());

        client
            .login(Authentication {
                username: "test-login2".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap_err();

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let login = User {
            username: "test-login2".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login2".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn users_validate() {
        let mut client = TestClient::new(start_server(None).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let list = client.post::<(), Vec<User>>("/users", ()).await.unwrap();
        assert_eq!(list.len(), 1);

        let table: &[User] = &[
            User {
                username: "".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("".into()),
                ..Default::default()
            },
            User {
                username: "erikhaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbeaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.meaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309aaaaaaaaaaaaaaaaaaaa".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclaxaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                ..Default::default()
            },
            User {
                username: "er".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Er".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("e@e".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlcla".into()),
                ..Default::default()
            },
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
        ];

        for (x, item) in table.iter().enumerate() {
            assert!(
                client
                    .put::<User, User>("/users", item.clone())
                    .await
                    .is_err(),
                "#{} succeeded",
                x
            )
        }
    }

    #[tokio::test]
    async fn users_crud() {
        let mut client = TestClient::new(start_server(None).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let list = client.post::<(), Vec<User>>("/users", ()).await.unwrap();
        assert_eq!(list.len(), 1);

        let table: &[User] = &[
            User {
                username: "erikh".into(),
                realname: Some("Erik Hollensbe".into()),
                email: Some("erikhollensbe@proton.me".into()),
                phone: Some("800-867-5309".into()),
                plaintext_password: Some("horlclax".into()),
                ..Default::default()
            },
            User {
                username: "scarlett".into(),
                realname: Some("Scarlett Hollensbe".into()),
                email: Some("scarlett@hollensbe.org".into()),
                phone: None,
                plaintext_password: Some("foobar123".into()),
                ..Default::default()
            },
            User {
                username: "cmaujean".into(),
                realname: Some("Christopher Maujean".into()),
                email: Some("christopher@maujean.org".into()),
                plaintext_password: Some("pooprocket".into()),
                ..Default::default()
            },
            User {
                username: "day".into(),
                realname: Some("Day Waterbury".into()),
                plaintext_password: Some("mmph1234".into()),
                ..Default::default()
            },
            User {
                username: "dpnvektor".into(),
                realname: Some("Julian Sutter".into()),
                plaintext_password: Some("meh12345".into()),
                ..Default::default()
            },
        ];

        let mut created = Vec::new();

        for item in table.into_iter() {
            let user = client
                .put::<User, User>("/users", item.clone())
                .await
                .unwrap();
            created.push(user);
        }

        for item in table.into_iter() {
            assert!(client
                .put::<User, User>("/users", item.clone())
                .await
                .is_err());
        }

        let list = client.post::<(), Vec<User>>("/users", ()).await.unwrap();
        assert_eq!(list.len(), table.len() + 1); // add the logged in user

        for item in created.iter() {
            assert_eq!(
                client
                    .get::<User>(&format!("/user/{}", item.id))
                    .await
                    .unwrap(),
                item.clone(),
            );
        }

        // update and fetch and compare
        for mut item in created.clone().into_iter() {
            item.realname = Some("new realname".into());
            client
                .post::<User, ()>(&format!("/user/{}", item.id), item.clone())
                .await
                .unwrap();
            assert_eq!(
                client
                    .get::<User>(&format!("/user/{}", item.id))
                    .await
                    .unwrap(),
                item.clone(),
            );
        }

        for item in created.into_iter() {
            client
                .delete::<()>(&format!("/user/{}", item.id))
                .await
                .unwrap();
        }

        let list = client.post::<(), Vec<User>>("/users", ()).await.unwrap();
        assert_eq!(list.len(), table.len() + 1);

        // check that our accounts actually got deleted
        let mut count = 0;
        for item in list {
            if item.deleted_at.is_none() {
                count += 1;
            }
        }

        assert_eq!(count, 1);

        // test deleted user unable to login
        assert!(client
            .login(Authentication {
                username: "erikh".into(),
                password: "horlclax".into(),
            })
            .await
            .is_err());
    }
}

mod zfs {
    use std::collections::HashMap;

    use crate::{
        db::models::User,
        server::messages::Authentication,
        testutil::{start_server, TestClient},
    };
    use buckle::client::ZFSStat;

    #[tokio::test]
    async fn zfs_errors() {
        let _ = buckle::testutil::destroy_zpool("errors", None);
        let (pool, file) = buckle::testutil::create_zpool("errors").unwrap();
        let mut client = TestClient::new(start_server(Some(pool.clone())).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let res = client
            .post::<_, ()>(
                "/zfs/modify_volume",
                buckle::client::ModifyVolume {
                    name: "volume".into(),
                    modifications: buckle::client::Volume {
                        name: "volume2".into(),
                        size: 100000 * 1024 * 1024,
                    },
                },
            )
            .await;

        let err = res.err().unwrap().to_string();

        let map: HashMap<String, String> = serde_json::from_str(&err).unwrap();

        assert_eq!(
            map.get("detail").unwrap().to_string(),
            "Error: cannot open 'buckle-test-errors/volume': dataset does not exist".to_string()
        );

        buckle::testutil::destroy_zpool("errors", Some(&file)).unwrap();
    }

    #[tokio::test]
    async fn zfs_basic() {
        let _ = buckle::testutil::destroy_zpool("gild-basic", None);
        let (pool, file) = buckle::testutil::create_zpool("gild-basic").unwrap();
        let mut client = TestClient::new(start_server(Some(pool.clone())).await.unwrap());

        let login = User {
            username: "test-login".into(),
            plaintext_password: Some("test-password".into()),
            ..Default::default()
        };
        assert!(client.put::<User, User>("/users", login).await.is_ok());

        client
            .login(Authentication {
                username: "test-login".into(),
                password: "test-password".into(),
            })
            .await
            .unwrap();

        let result: Vec<ZFSStat> = client.post("/zfs/list", "").await.unwrap();
        assert_eq!(result.len(), 0);
        client
            .post::<_, ()>(
                "/zfs/create_dataset",
                buckle::client::Dataset {
                    name: "dataset".into(),
                    quota: None,
                },
            )
            .await
            .unwrap();
        let result: Vec<ZFSStat> = client.post("/zfs/list", "").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "dataset");
        assert_eq!(result[0].full_name, "buckle-test-gild-basic/dataset");
        assert_ne!(result[0].size, 0);
        assert_ne!(result[0].avail, 0);
        assert_ne!(result[0].refer, 0);
        assert_ne!(result[0].used, 0);
        assert_eq!(
            result[0].mountpoint,
            Some("/buckle-test-gild-basic/dataset".into())
        );
        client
            .post::<_, ()>(
                "/zfs/create_volume",
                buckle::client::Volume {
                    name: "volume".into(),
                    size: 100 * 1024 * 1024,
                },
            )
            .await
            .unwrap();
        let result: Vec<ZFSStat> = client.post("/zfs/list", "").await.unwrap();
        assert_eq!(result.len(), 2);
        let result: Vec<ZFSStat> = client.post("/zfs/list", "volume").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "volume");
        assert_eq!(result[0].full_name, "buckle-test-gild-basic/volume");
        assert_ne!(result[0].size, 0);
        assert_ne!(result[0].avail, 0);
        assert_ne!(result[0].refer, 0);
        assert_ne!(result[0].used, 0);
        assert_eq!(result[0].mountpoint, None);

        client
            .post::<_, ()>(
                "/zfs/modify_volume",
                buckle::client::ModifyVolume {
                    name: "volume".into(),
                    modifications: buckle::client::Volume {
                        name: "volume2".into(),
                        size: 5 * 1024 * 1024,
                    },
                },
            )
            .await
            .unwrap();

        let result: Vec<ZFSStat> = client.post("/zfs/list", "volume2").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "volume2");
        assert_eq!(result[0].full_name, "buckle-test-gild-basic/volume2");
        assert_ne!(result[0].size, 0);
        assert!(result[0].size < 6 * 1024 * 1024 && result[0].size > 4 * 1024 * 1024);
        assert_ne!(result[0].avail, 0);
        assert_ne!(result[0].refer, 0);
        assert_ne!(result[0].used, 0);
        assert_eq!(result[0].mountpoint, None);

        let result: Vec<ZFSStat> = client.post("/zfs/list", "dataset").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "dataset");
        assert_eq!(result[0].full_name, "buckle-test-gild-basic/dataset");
        assert_ne!(result[0].size, 0);
        assert_ne!(result[0].avail, 0);
        assert_ne!(result[0].refer, 0);
        assert_ne!(result[0].used, 0);
        assert_eq!(
            result[0].mountpoint,
            Some("/buckle-test-gild-basic/dataset".into())
        );

        client
            .post::<_, ()>(
                "/zfs/modify_dataset",
                buckle::client::ModifyDataset {
                    name: "dataset".into(),
                    modifications: buckle::client::Dataset {
                        name: "dataset2".into(),
                        quota: Some(5 * 1024 * 1024),
                    },
                },
            )
            .await
            .unwrap();

        let result: Vec<ZFSStat> = client.post("/zfs/list", "dataset2").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "dataset2");
        assert_eq!(result[0].full_name, "buckle-test-gild-basic/dataset2");
        assert_ne!(result[0].size, 0);
        assert_ne!(result[0].avail, 0);
        assert_ne!(result[0].refer, 0);
        assert_ne!(result[0].used, 0);
        assert_eq!(
            result[0].mountpoint,
            Some("/buckle-test-gild-basic/dataset2".into())
        );

        client
            .post::<_, ()>("/zfs/destroy", "dataset2")
            .await
            .unwrap();
        let result: Vec<ZFSStat> = client.post("/zfs/list", "dataset2").await.unwrap();
        assert_eq!(result.len(), 0);
        let result: Vec<ZFSStat> = client.post("/zfs/list", "").await.unwrap();
        assert_eq!(result.len(), 1);
        client
            .post::<_, ()>("/zfs/destroy", "volume2")
            .await
            .unwrap();
        let result: Vec<ZFSStat> = client.post("/zfs/list", "volume2").await.unwrap();
        assert_eq!(result.len(), 0);

        buckle::testutil::destroy_zpool("gild-basic", Some(&file)).unwrap();
    }
}
