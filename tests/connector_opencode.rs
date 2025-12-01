use coding_agent_search::connectors::opencode::OpenCodeConnector;
use coding_agent_search::connectors::{Connector, ScanContext};
use rusqlite::Connection;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper to create a basic schema
fn init_db(path: &PathBuf) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE sessions (
            id INTEGER PRIMARY KEY,
            title TEXT,
            workspace TEXT,
            created_at INTEGER
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE messages (
            id INTEGER PRIMARY KEY,
            session_id INTEGER,
            role TEXT,
            content TEXT,
            created_at INTEGER
        )",
        [],
    )
    .unwrap();

    conn
}

#[test]
fn opencode_parses_sqlite_fixture() {
    let fixture_root = PathBuf::from("tests/fixtures/opencode");
    let conn = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: fixture_root.clone(),
        since_ts: None,
    };
    // This relies on the existing binary fixture
    let convs = conn.scan(&ctx).expect("scan");
    assert_eq!(convs.len(), 1);
    let c = &convs[0];
    assert_eq!(c.title.as_deref(), Some("OpenCode Session"));
    assert_eq!(c.messages.len(), 2);
}

#[test]
fn opencode_filters_messages_with_since_ts() {
    let fixture_root = PathBuf::from("tests/fixtures/opencode");
    let conn = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: fixture_root.clone(),
        since_ts: Some(1_700_000_000_000),
    };
    let convs = conn.scan(&ctx).expect("scan");
    assert_eq!(convs.len(), 1);
    let c = &convs[0];
    assert_eq!(c.messages.len(), 1);
    assert_eq!(c.messages[0].created_at, Some(1_700_000_005_000));
}

/// Test basic session parsing from a fresh DB
#[test]
fn opencode_parses_created_db() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO sessions (id, title, workspace, created_at) VALUES (1, 'My Session', '/tmp', 1000)",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (1, 'user', 'hello', 1000)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (1, 'assistant', 'hi', 2000)",
        [],
    ).unwrap();

    // Close connection to ensure flush
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.title, Some("My Session".to_string()));
    assert_eq!(c.workspace, Some(PathBuf::from("/tmp")));
    assert_eq!(c.messages.len(), 2);
    assert_eq!(c.messages[0].content, "hello");
    assert_eq!(c.messages[1].content, "hi");
}

/// Test handling of DB without sessions table (fallback mode)
#[test]
fn opencode_handles_missing_sessions_table() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nosessions.db");
    let conn = Connection::open(&db_path).unwrap();

    // Only messages table
    conn.execute(
        "CREATE TABLE messages (
            role TEXT,
            content TEXT,
            created_at INTEGER
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO messages (role, content, created_at) VALUES ('user', 'orphan', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    // Should be treated as a fallback conversation
    let c = &convs[0];
    assert!(c.messages[0].content.contains("orphan"));
    assert!(c.workspace.is_none());
}

/// Test column name fallbacks
#[test]
fn opencode_maps_alternate_columns() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("altcols.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE messages (
            sender TEXT,    -- instead of role
            text TEXT,      -- instead of content
            timestamp INTEGER -- instead of created_at
        )",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO messages (sender, text, timestamp) VALUES ('bot', 'alternate', 5000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let c = &convs[0];
    assert_eq!(c.messages[0].role, "bot");
    assert_eq!(c.messages[0].content, "alternate");
    assert_eq!(c.messages[0].created_at, Some(5000));
}

/// Test that CASS internal DBs are ignored
#[test]
fn opencode_ignores_internal_dbs() {
    let dir = TempDir::new().unwrap();

    // Create ignored DBs
    for name in ["agent_search.db", "conversations.db"] {
        let path = dir.path().join(name);
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE messages (content TEXT)", [])
            .unwrap();
    }

    // Create valid DB
    let valid_path = dir.path().join("valid.sqlite");
    let conn = Connection::open(&valid_path).unwrap();
    conn.execute("CREATE TABLE messages (content TEXT)", [])
        .unwrap();
    conn.execute("INSERT INTO messages (content) VALUES ('valid')", [])
        .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();

    // Should only find the valid one
    assert_eq!(convs.len(), 1);
    assert!(
        convs[0]
            .source_path
            .to_string_lossy()
            .contains("valid.sqlite")
    );
}

/// Test since_ts filtering logic
#[test]
fn opencode_since_ts_logic() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("since.db");
    let conn = init_db(&db_path);

    // Old message
    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (1, 'u', 'old', 1000)",
        [],
    )
    .unwrap();
    // New message
    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (1, 'u', 'new', 3000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: Some(2000),
    };
    let convs = connector.scan(&ctx).unwrap();

    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].messages.len(), 1);
    assert_eq!(convs[0].messages[0].content, "new");
}

/// Test orphaned messages (no matching session) are collected
#[test]
fn opencode_collects_orphaned_messages() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("orphans.db");
    let conn = init_db(&db_path);

    // Message with session_id that doesn't exist in sessions table
    // (Note: the code doesn't strictly require FK existence, it just groups by session_id)
    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (999, 'u', 'orphan_session', 1000)",
        [],
    ).unwrap();

    // Message with NULL session_id
    conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES (NULL, 'u', 'null_session', 2000)",
        [],
    ).unwrap();

    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();

    // We expect:
    // 1 conversation for session 999 (even if not in sessions table, it has an ID)
    // 1 conversation for fallback messages (NULL session_id)
    assert_eq!(convs.len(), 2);

    let has_999 = convs
        .iter()
        .any(|c| c.messages[0].content == "orphan_session");
    let has_null = convs
        .iter()
        .any(|c| c.messages[0].content == "null_session");

    assert!(has_999);
    assert!(has_null);
}

/// Test title extraction from sessions table
#[test]
fn opencode_title_extraction() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("titles.db");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO sessions (id, title, created_at) VALUES (1, 'Explicit Title', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'msg', 1000)",
        [],
    )
    .unwrap();

    // Session 2: No title, fallback to first message
    conn.execute("INSERT INTO sessions (id, created_at) VALUES (2, 2000)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (2, 'First Line', 2000)",
        [],
    )
    .unwrap();

    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();

    let t1 = convs
        .iter()
        .find(|c| c.external_id.as_ref().unwrap().contains("session-1"))
        .unwrap();
    assert_eq!(t1.title, Some("Explicit Title".to_string()));

    let t2 = convs
        .iter()
        .find(|c| c.external_id.as_ref().unwrap().contains("session-2"))
        .unwrap();
    assert_eq!(t2.title, Some("First Line".to_string()));
}

/// Test multiple databases in directory
#[test]
fn opencode_scans_multiple_dbs() {
    let dir = TempDir::new().unwrap();

    for i in 1..=3 {
        let path = dir.path().join(format!("db{i}.sqlite"));
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE messages (content TEXT)", [])
            .unwrap();
        conn.execute("INSERT INTO messages (content) VALUES ('msg')", [])
            .unwrap();
    }

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 3);
}

// ============================================================================
// Additional edge case tests
// ============================================================================

/// Test agent_slug is always "opencode"
#[test]
fn opencode_sets_correct_agent_slug() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("slug.db");
    let conn = init_db(&db_path);
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'test', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].agent_slug, "opencode");
}

/// Test source_path is set to DB path
#[test]
fn opencode_sets_source_path() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("sourcepath.db");
    let conn = init_db(&db_path);
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'test', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].source_path, db_path);
}

/// Test started_at and ended_at computation
#[test]
fn opencode_computes_started_ended_at() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("times.db");
    let conn = init_db(&db_path);

    // Session with started_at
    conn.execute("INSERT INTO sessions (id, created_at) VALUES (1, 500)", [])
        .unwrap();

    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'first', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'second', 2000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'third', 3000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    // started_at comes from session, ended_at from last message
    assert_eq!(convs[0].started_at, Some(500));
    assert_eq!(convs[0].ended_at, Some(3000));
}

/// Test sequential index assignment
#[test]
fn opencode_assigns_sequential_indices() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("indices.db");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'm0', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'm1', 2000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'm2', 3000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let msgs = &convs[0].messages;
    for (i, msg) in msgs.iter().enumerate() {
        assert_eq!(msg.idx, i as i64);
    }
}

/// Test workspace extraction from root_path column
#[test]
fn opencode_workspace_from_root_path() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("rootpath.db");
    let conn = Connection::open(&db_path).unwrap();

    // Schema with root_path instead of workspace
    conn.execute(
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, root_path TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();
    conn.execute(
        "CREATE TABLE messages (session_id INTEGER, content TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO sessions (id, root_path, created_at) VALUES (1, '/my/project', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'test', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].workspace, Some(PathBuf::from("/my/project")));
}

/// Test empty database handling
#[test]
fn opencode_handles_empty_db() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("empty.db");
    let conn = init_db(&db_path);
    // No messages inserted
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert!(convs.is_empty());
}

/// Test DB without messages table
#[test]
fn opencode_handles_db_without_messages() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nomsg.db");
    let conn = Connection::open(&db_path).unwrap();
    // Only sessions table, no messages
    conn.execute(
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, title TEXT)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO sessions (id, title) VALUES (1, 'Test')", [])
        .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert!(convs.is_empty());
}

/// Test task_id as alternative to session_id
#[test]
fn opencode_groups_by_task_id() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("taskid.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE messages (task_id INTEGER, content TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO messages (task_id, content, created_at) VALUES (100, 'task100', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (task_id, content, created_at) VALUES (200, 'task200', 2000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();

    // Should have 2 conversations (one per task_id)
    assert_eq!(convs.len(), 2);
}

/// Test author extraction from sender column
#[test]
fn opencode_extracts_author() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("author.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE messages (sender TEXT, content TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (sender, content, created_at) VALUES ('claude-3', 'hello', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    // sender column used for both role and author
    let msg = &convs[0].messages[0];
    assert_eq!(msg.role, "claude-3");
    assert_eq!(msg.author, Some("claude-3".to_string()));
}

/// Test "message" as alternate content column
#[test]
fn opencode_message_column_fallback() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("msgcol.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE messages (message TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (message, created_at) VALUES ('from message col', 1000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].messages[0].content, "from message col");
}

/// Test "name" as alternate title column in sessions
#[test]
fn opencode_name_column_for_title() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("namecol.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "CREATE TABLE messages (session_id INTEGER, content TEXT)",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO sessions (id, name) VALUES (1, 'Name As Title')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content) VALUES (1, 'msg')",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].title, Some("Name As Title".to_string()));
}

/// Test "ts" as alternate timestamp column
#[test]
fn opencode_ts_column_fallback() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("tscol.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute("CREATE TABLE messages (content TEXT, ts INTEGER)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO messages (content, ts) VALUES ('ts time', 5000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].messages[0].created_at, Some(5000));
}

/// Test external ID format includes session ID and DB hash
#[test]
fn opencode_external_id_format() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("extid.db");
    let conn = init_db(&db_path);

    conn.execute("INSERT INTO sessions (id) VALUES (42)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content) VALUES (42, 'msg')",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let ext_id = convs[0].external_id.as_ref().unwrap();
    assert!(ext_id.starts_with("session-42-"));
    // Should contain hex hash
    assert!(ext_id.chars().any(|c| c.is_ascii_hexdigit()));
}

/// Test message ordering by timestamp
#[test]
fn opencode_orders_messages_by_timestamp() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("order.db");
    let conn = init_db(&db_path);

    // Insert out of order
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'third', 3000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'first', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'second', 2000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let msgs = &convs[0].messages;
    assert_eq!(msgs[0].content, "first");
    assert_eq!(msgs[1].content, "second");
    assert_eq!(msgs[2].content, "third");
}

/// Test nested directory scanning
#[test]
fn opencode_scans_nested_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("nested").join("deep");
    std::fs::create_dir_all(&nested).unwrap();

    let db_path = nested.join("deep.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute("CREATE TABLE messages (content TEXT)", [])
        .unwrap();
    conn.execute("INSERT INTO messages (content) VALUES ('deep')", [])
        .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].messages[0].content, "deep");
}

/// Test metadata contains db_path
#[test]
fn opencode_metadata_contains_db_path() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("meta.db");
    let conn = init_db(&db_path);
    conn.execute(
        "INSERT INTO messages (session_id, content) VALUES (1, 'test')",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);

    let metadata = &convs[0].metadata;
    assert!(metadata.get("db_path").is_some());
    assert!(metadata.get("session_id").is_some());
}

/// Test empty directory handling
#[test]
fn opencode_handles_empty_directory() {
    let dir = TempDir::new().unwrap();
    // No DBs created

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert!(convs.is_empty());
}

/// Test messages with NULL content are preserved
#[test]
fn opencode_preserves_null_content() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("nullcontent.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "CREATE TABLE messages (content TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (content, created_at) VALUES (NULL, 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (content, created_at) VALUES ('valid', 2000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].messages.len(), 2);
    assert_eq!(convs[0].messages[0].content, ""); // NULL becomes empty string
    assert_eq!(convs[0].messages[1].content, "valid");
}

/// Test started_at fallback to first message timestamp when session has no timestamp
#[test]
fn opencode_started_at_fallback() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("fallback.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute(
        "CREATE TABLE sessions (id INTEGER PRIMARY KEY, title TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "CREATE TABLE messages (session_id INTEGER, content TEXT, created_at INTEGER)",
        [],
    )
    .unwrap();

    // Session without created_at
    conn.execute("INSERT INTO sessions (id, title) VALUES (1, 'Test')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 'msg', 5000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 1);
    // started_at falls back to first message timestamp
    assert_eq!(convs[0].started_at, Some(5000));
}

/// Test multiple sessions in same DB
#[test]
fn opencode_multiple_sessions_same_db() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("multi.db");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO sessions (id, title) VALUES (1, 'Session 1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title) VALUES (2, 'Session 2')",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 's1m1', 1000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (1, 's1m2', 2000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO messages (session_id, content, created_at) VALUES (2, 's2m1', 3000)",
        [],
    )
    .unwrap();
    drop(conn);

    let connector = OpenCodeConnector::new();
    let ctx = ScanContext {
        data_root: dir.path().to_path_buf(),
        since_ts: None,
    };
    let convs = connector.scan(&ctx).unwrap();
    assert_eq!(convs.len(), 2);

    let s1 = convs
        .iter()
        .find(|c| c.title == Some("Session 1".to_string()));
    let s2 = convs
        .iter()
        .find(|c| c.title == Some("Session 2".to_string()));
    assert!(s1.is_some());
    assert!(s2.is_some());
    assert_eq!(s1.unwrap().messages.len(), 2);
    assert_eq!(s2.unwrap().messages.len(), 1);
}
