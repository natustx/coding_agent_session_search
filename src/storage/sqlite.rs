//! SQLite backend: schema, pragmas, and migrations.

use crate::model::types::{Agent, AgentKind, Conversation, Message, MessageRole, Snippet};
use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 1;

const MIGRATION_V1: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version TEXT,
    kind TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    display_name TEXT
);

CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    agent_id INTEGER NOT NULL REFERENCES agents(id),
    workspace_id INTEGER REFERENCES workspaces(id),
    external_id TEXT,
    title TEXT,
    source_path TEXT NOT NULL,
    started_at INTEGER,
    ended_at INTEGER,
    approx_tokens INTEGER,
    metadata_json TEXT,
    UNIQUE(agent_id, external_id)
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    role TEXT NOT NULL,
    author TEXT,
    created_at INTEGER,
    content TEXT NOT NULL,
    extra_json TEXT,
    UNIQUE(conversation_id, idx)
);

CREATE TABLE IF NOT EXISTS snippets (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    file_path TEXT,
    start_line INTEGER,
    end_line INTEGER,
    language TEXT,
    snippet_text TEXT
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS conversation_tags (
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_conversations_agent_started
    ON conversations(agent_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_messages_conv_idx
    ON messages(conversation_id, idx);

CREATE INDEX IF NOT EXISTS idx_messages_created
    ON messages(created_at);
"#;

pub struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating db directory {}", parent.display()))?;
        }

        let mut conn = Connection::open(path)
            .with_context(|| format!("opening sqlite db at {}", path.display()))?;

        apply_pragmas(&mut conn)?;
        init_meta(&mut conn)?;
        migrate(&mut conn)?;

        Ok(Self { conn })
    }

    pub fn raw(&self) -> &Connection {
        &self.conn
    }

    pub fn ensure_agent(&self, agent: &Agent) -> Result<i64> {
        let now = now_millis();
        self.conn.execute(
            "INSERT INTO agents(slug, name, version, kind, created_at, updated_at) VALUES(?,?,?,?,?,?)
             ON CONFLICT(slug) DO UPDATE SET name=excluded.name, version=excluded.version, kind=excluded.kind, updated_at=excluded.updated_at",
            params![
                &agent.slug,
                &agent.name,
                &agent.version,
                agent_kind_str(agent.kind.clone()),
                now,
                now
            ],
        )?;

        self.conn
            .query_row(
                "SELECT id FROM agents WHERE slug = ?",
                params![&agent.slug],
                |row| row.get(0),
            )
            .with_context(|| format!("fetching agent id for {}", agent.slug))
    }

    pub fn ensure_workspace(&self, path: &Path, display_name: Option<&str>) -> Result<i64> {
        let path_str = path.to_string_lossy();
        self.conn.execute(
            "INSERT INTO workspaces(path, display_name) VALUES(?,?)
             ON CONFLICT(path) DO UPDATE SET display_name=COALESCE(excluded.display_name, workspaces.display_name)",
            params![path_str, display_name],
        )?;

        self.conn
            .query_row(
                "SELECT id FROM workspaces WHERE path = ?",
                params![path_str],
                |row| row.get(0),
            )
            .with_context(|| format!("fetching workspace id for {}", path_str))
    }

    pub fn insert_conversation_tree(
        &mut self,
        agent_id: i64,
        workspace_id: Option<i64>,
        conv: &Conversation,
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;

        if let Some(ext) = &conv.external_id
            && let Some(existing) = tx
                .query_row(
                    "SELECT id FROM conversations WHERE agent_id = ? AND external_id = ?",
                    params![agent_id, ext],
                    |row| row.get(0),
                )
                .optional()?
        {
            return Ok(existing);
        }

        let conv_id = insert_conversation(&tx, agent_id, workspace_id, conv)?;
        for msg in &conv.messages {
            let msg_id = insert_message(&tx, conv_id, msg)?;
            insert_snippets(&tx, msg_id, &msg.snippets)?;
        }
        tx.commit()?;
        Ok(conv_id)
    }
}

fn apply_pragmas(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -65536; -- 64MB
        PRAGMA mmap_size = 268435456; -- 256MB
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

fn init_meta(conn: &mut Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;

    let existing: Option<i64> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0).map(|s| s.parse().unwrap_or(0)),
        )
        .optional()?;

    if existing.is_none() {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?)",
            params![SCHEMA_VERSION.to_string()],
        )?;
    }

    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0).map(|s| s.parse().unwrap_or(0)),
        )
        .optional()?
        .unwrap_or(0);

    match current {
        0 => {
            conn.execute_batch(MIGRATION_V1)?;
            conn.execute(
                "UPDATE meta SET value = ? WHERE key = 'schema_version'",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }
        v if v == SCHEMA_VERSION => {}
        v => return Err(anyhow!("unsupported schema version {}", v)),
    }

    Ok(())
}

fn insert_conversation(
    tx: &Transaction<'_>,
    agent_id: i64,
    workspace_id: Option<i64>,
    conv: &Conversation,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO conversations(
            agent_id, workspace_id, external_id, title, source_path, started_at, ended_at, approx_tokens, metadata_json
        ) VALUES(?,?,?,?,?,?,?,?,?)",
        params![
            agent_id,
            workspace_id,
            conv.external_id,
            conv.title,
            path_to_string(&conv.source_path),
            conv.started_at,
            conv.ended_at,
            conv.approx_tokens,
            serde_json::to_string(&conv.metadata_json)?
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

fn insert_message(tx: &Transaction<'_>, conversation_id: i64, msg: &Message) -> Result<i64> {
    tx.execute(
        "INSERT INTO messages(conversation_id, idx, role, author, created_at, content, extra_json)
         VALUES(?,?,?,?,?,?,?)",
        params![
            conversation_id,
            msg.idx,
            role_str(&msg.role),
            msg.author,
            msg.created_at,
            msg.content,
            serde_json::to_string(&msg.extra_json)?
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

fn insert_snippets(tx: &Transaction<'_>, message_id: i64, snippets: &[Snippet]) -> Result<()> {
    for snip in snippets {
        tx.execute(
            "INSERT INTO snippets(message_id, file_path, start_line, end_line, language, snippet_text)
             VALUES(?,?,?,?,?,?)",
            params![
                message_id,
                snip.file_path.as_ref().map(path_to_string),
                snip.start_line,
                snip.end_line,
                snip.language,
                snip.snippet_text,
            ],
        )?;
    }
    Ok(())
}

fn path_to_string<P: AsRef<Path>>(p: P) -> String {
    p.as_ref().to_string_lossy().into_owned()
}

fn role_str(role: &MessageRole) -> String {
    match role {
        MessageRole::User => "user".to_owned(),
        MessageRole::Agent => "agent".to_owned(),
        MessageRole::Tool => "tool".to_owned(),
        MessageRole::System => "system".to_owned(),
        MessageRole::Other(v) => v.clone(),
    }
}

fn agent_kind_str(kind: AgentKind) -> String {
    match kind {
        AgentKind::Cli => "cli".into(),
        AgentKind::VsCode => "vscode".into(),
        AgentKind::Hybrid => "hybrid".into(),
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
