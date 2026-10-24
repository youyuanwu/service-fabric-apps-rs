use super::*;
use mssf_transport::{
    connection::{read_message, write_message},
    message::{ListenInstance, MessageId},
};
use tokio::net::TcpListener;

async fn session() -> (Arc<Session>, mpsc::Receiver<Cleanup>, tokio::net::TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        read_message(&mut peer).await.unwrap();
        write_message(
            &mut peer,
            &Message::listen_instance(
                MessageId {
                    guid: [1; 16],
                    index: 1,
                },
                &ListenInstance::default(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
        peer
    });
    let connection =
        Connection::connect_clear(address, deadline_after(Duration::from_secs(3)).unwrap())
            .await
            .unwrap();
    let (cleanup, cleanups) = mpsc::channel(MAX_CLEANUPS);
    (
        Arc::new(Session {
            connection,
            id: "session-test".into(),
            timeout: Duration::from_secs(3),
            state: Mutex::new(Registrations {
                next: 1,
                ..Default::default()
            }),
            cleanup,
            receiver: Arc::new(Receiver::default()),
            wake: Notify::new(),
        }),
        cleanups,
        task.await.unwrap(),
    )
}

#[tokio::test]
async fn full_cleanup_command_path_is_terminal_without_another_queue_write() {
    let (session, _cleanups, _peer) = session().await;
    for id in 0..MAX_CLEANUPS {
        session
            .cleanup
            .try_send(Cleanup {
                id: id as u64,
                deadline: deadline_after(Duration::from_secs(3)).unwrap(),
                complete: None,
            })
            .unwrap_or_else(|_| panic!("queue should fit"));
    }
    session
        .state
        .lock()
        .unwrap()
        .filters
        .insert(100, RegistrationState::Active);
    session.drop_registration(100);
    assert!(session.connection.is_closed());
    assert!(matches!(
        session.receiver.recv().await,
        Some(Err(Error::ResourceLimit(_)))
    ));
    assert!(session.receiver.recv().await.is_none());
    session.connection.close().await.unwrap();
}

#[tokio::test]
async fn guard_handoff_has_one_owner_and_retains_slot_until_confirmation() {
    let (session, mut cleanups, _peer) = session().await;
    session
        .state
        .lock()
        .unwrap()
        .filters
        .insert(1, RegistrationState::Active);
    let registration = ServiceNotificationRegistration {
        session: Arc::downgrade(&session),
        id: 1,
        armed: true,
    };
    drop(registration);
    let command = cleanups.recv().await.unwrap();
    assert_eq!(command.id, 1);
    assert_eq!(
        session.state.lock().unwrap().filters.get(&1),
        Some(&RegistrationState::Cleanup)
    );
    session.drop_registration(1);
    assert!(cleanups.try_recv().is_err());
    assert_eq!(session.state.lock().unwrap().filters.len(), 1);
    session.stop();
    session.connection.close().await.unwrap();
}

#[tokio::test]
async fn aborted_unpolled_worker_publishes_terminal_and_stale_guards_cannot_reopen() {
    let (session, _cleanups, _peer) = session().await;
    let exit = WorkerExit(session.clone());
    let task = tokio::spawn(async move {
        let _exit = exit;
        std::future::pending::<()>().await;
    });
    task.abort();
    let _ = task.await;
    assert!(matches!(
        session.receiver.recv().await,
        Some(Err(Error::Protocol(_)))
    ));
    assert!(session.receiver.recv().await.is_none());
    let old = ServiceNotificationRegistration {
        session: Arc::downgrade(&session),
        id: 1,
        armed: true,
    };
    assert!(matches!(old.unregister(None).await, Err(Error::Closed)));
    session.connection.close().await.unwrap();
}

#[tokio::test]
async fn cancelled_unsent_registration_releases_slot_without_retiring_session() {
    let (session, _cleanups, _peer) = session().await;
    session
        .state
        .lock()
        .unwrap()
        .filters
        .insert(1, RegistrationState::Registering);
    drop(Mutation {
        session: Arc::downgrade(&session),
        id: 1,
        sent: SendStatus::default(),
        armed: true,
    });
    assert!(session.state.lock().unwrap().filters.is_empty());
    assert!(!session.connection.is_closed());
    session.stop();
    session.connection.close().await.unwrap();
}
