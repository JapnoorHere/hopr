//! Command and connection history (§9.19). Commands come from shell integration (exact)
//! or from typed input (approximate); connections from session state changes.

use hopr_core::model::{CommandRecord, ConnectionRecord};
use sqlx::FromRow;

use crate::{Result, Store, new_id, now};

/// Commands kept; older ones are pruned at startup.
pub const HISTORY_KEEP: i64 = 50_000;

/// A command to remember.
pub struct NewCommand<'a> {
    /// Saved host, if any.
    pub host_id: Option<&'a str>,
    /// Host label.
    pub host_label: &'a str,
    /// Session it ran in.
    pub session_id: &'a str,
    /// Command line.
    pub command: &'a str,
    /// Working directory.
    pub cwd: Option<&'a str>,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// From shell integration.
    pub exact: bool,
    /// RFC 3339; `None` means now.
    pub started_at: Option<&'a str>,
    /// Run time.
    pub duration_ms: Option<u32>,
}

/// Whether a command must not be saved: blank, typed with a leading space (like bash's
/// `ignorespace`) when `ignore_space` is on, or matching an exclude pattern. Patterns are
/// case-insensitive globs over the whole command where `*` matches anything.
pub fn is_excluded(command: &str, ignore_space: bool, patterns: &[String]) -> bool {
    if command.trim().is_empty() || (ignore_space && command.starts_with(' ')) {
        return true;
    }
    let cmd = command.trim().to_lowercase();
    patterns
        .iter()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .any(|p| glob(&p, &cmd))
}

fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let Some((first, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    let Some(mut tail) = text.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return tail.is_empty(); // no `*`: exact match
    };
    for part in middle {
        match tail.find(part) {
            Some(i) => tail = &tail[i + part.len()..],
            None => return false,
        }
    }
    tail.ends_with(last)
}

#[derive(FromRow)]
struct CommandRow {
    id: String,
    host_id: Option<String>,
    host_label: String,
    command: String,
    cwd: Option<String>,
    exit_code: Option<i64>,
    exact: bool,
    started_at: String,
    duration_ms: Option<i64>,
}

impl From<CommandRow> for CommandRecord {
    fn from(r: CommandRow) -> Self {
        CommandRecord {
            id: r.id,
            host_id: r.host_id,
            host_label: r.host_label,
            command: r.command,
            cwd: r.cwd,
            exit_code: r.exit_code.and_then(|c| i32::try_from(c).ok()),
            exact: r.exact,
            started_at: r.started_at,
            duration_ms: r.duration_ms.and_then(|d| u32::try_from(d).ok()),
        }
    }
}

#[derive(FromRow)]
struct ConnectionRow {
    id: String,
    host_id: Option<String>,
    host_label: String,
    kind: String,
    started_at: String,
    ended_at: Option<String>,
    end_reason: Option<String>,
    bytes_in: i64,
    bytes_out: i64,
}

impl From<ConnectionRow> for ConnectionRecord {
    #[allow(clippy::cast_precision_loss)] // byte counts stay far below 2^53
    fn from(r: ConnectionRow) -> Self {
        ConnectionRecord {
            id: r.id,
            host_id: r.host_id,
            host_label: r.host_label,
            kind: r.kind,
            started_at: r.started_at,
            ended_at: r.ended_at,
            end_reason: r.end_reason,
            bytes_in: r.bytes_in as f64,
            bytes_out: r.bytes_out as f64,
        }
    }
}

const SELECT_COMMANDS: &str = "SELECT id, host_id, host_label, command, cwd, exit_code, exact, started_at, duration_ms FROM command_history";
const SELECT_CONNECTIONS: &str = "SELECT id, host_id, host_label, kind, started_at, ended_at, end_reason, bytes_in, bytes_out FROM connection_history";

/// `%`, `_` and `\` taken literally inside a LIKE pattern (with `ESCAPE '\'`).
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('%');
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

impl Store {
    /// Remember a command.
    pub async fn add_command(&self, c: NewCommand<'_>) -> Result<CommandRecord> {
        let id = new_id();
        let started = c.started_at.map_or_else(now, str::to_owned);
        sqlx::query(
            "INSERT INTO command_history (id, host_id, host_label, session_id, command, cwd, exit_code, exact, started_at, duration_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(c.host_id)
        .bind(c.host_label)
        .bind(c.session_id)
        .bind(c.command)
        .bind(c.cwd)
        .bind(c.exit_code)
        .bind(c.exact)
        .bind(&started)
        .bind(c.duration_ms)
        .execute(&self.pool)
        .await?;
        let row: CommandRow = sqlx::query_as(&format!("{SELECT_COMMANDS} WHERE id = ?"))
            .bind(&id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.into())
    }

    /// Commands, newest first, optionally containing `query` and from one host
    /// (`host_id` "" means quick-connect and local sessions).
    pub async fn list_commands(
        &self,
        query: &str,
        host_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CommandRecord>> {
        let mut sql = format!("{SELECT_COMMANDS} WHERE command LIKE ? ESCAPE '\\'");
        match host_id {
            Some("") => sql.push_str(" AND host_id IS NULL"),
            Some(_) => sql.push_str(" AND host_id = ?"),
            None => {}
        }
        sql.push_str(" ORDER BY started_at DESC, id DESC LIMIT ?");
        let mut q = sqlx::query_as::<_, CommandRow>(&sql).bind(like_escape(query.trim()));
        if let Some(h) = host_id.filter(|h| !h.is_empty()) {
            q = q.bind(h);
        }
        let rows = q.bind(limit.clamp(1, 5000)).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Forget one command.
    pub async fn delete_command(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM command_history WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Forget all commands and connections. Returns how many commands were removed.
    pub async fn clear_history(&self) -> Result<u64> {
        let n = sqlx::query("DELETE FROM command_history")
            .execute(&self.pool)
            .await?
            .rows_affected();
        sqlx::query("DELETE FROM connection_history")
            .execute(&self.pool)
            .await?;
        Ok(n)
    }

    /// Keep the newest [`HISTORY_KEEP`] commands; close connections left open by a crash.
    pub async fn prune_history(&self) -> Result<()> {
        sqlx::query(
            "DELETE FROM command_history WHERE id IN
             (SELECT id FROM command_history ORDER BY started_at DESC LIMIT -1 OFFSET ?)",
        )
        .bind(HISTORY_KEEP)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "UPDATE connection_history SET ended_at = started_at, end_reason = 'unknown' WHERE ended_at IS NULL",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A connection became ready. Returns its history id.
    pub async fn connection_started(
        &self,
        host_id: Option<&str>,
        host_label: &str,
        kind: &str,
    ) -> Result<String> {
        let id = new_id();
        sqlx::query(
            "INSERT INTO connection_history (id, host_id, host_label, kind, started_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(host_id)
        .bind(host_label)
        .bind(kind)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// A connection ended.
    pub async fn connection_ended(
        &self,
        id: &str,
        reason: &str,
        bytes_in: u64,
        bytes_out: u64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE connection_history SET ended_at = ?, end_reason = ?, bytes_in = ?, bytes_out = ? WHERE id = ?",
        )
        .bind(now())
        .bind(reason)
        .bind(i64::try_from(bytes_in).unwrap_or(i64::MAX))
        .bind(i64::try_from(bytes_out).unwrap_or(i64::MAX))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Connections, newest first, optionally for one host.
    pub async fn list_connections(
        &self,
        host_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ConnectionRecord>> {
        let limit = limit.clamp(1, 5000);
        let rows: Vec<ConnectionRow> = match host_id {
            Some(h) => sqlx::query_as(&format!(
                "{SELECT_CONNECTIONS} WHERE host_id = ? ORDER BY started_at DESC, id DESC LIMIT ?"
            ))
            .bind(h)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?,
            None => {
                sqlx::query_as(&format!(
                    "{SELECT_CONNECTIONS} ORDER BY started_at DESC, id DESC LIMIT ?"
                ))
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd<'a>(host: Option<&'a str>, command: &'a str) -> NewCommand<'a> {
        NewCommand {
            host_id: host,
            host_label: host.unwrap_or("local"),
            session_id: "s1",
            command,
            cwd: Some("/srv"),
            exit_code: Some(0),
            exact: true,
            started_at: None,
            duration_ms: Some(12),
        }
    }

    #[test]
    fn exclusion_rules() {
        let pats = vec![
            "*password*".to_owned(),
            "export *=*".to_owned(),
            " ".to_owned(),
        ];
        assert!(is_excluded("   ", false, &[]));
        assert!(is_excluded(" ls", true, &[]));
        assert!(!is_excluded(" ls", false, &[]));
        assert!(is_excluded("mysql --PASSWORD=x", false, &pats));
        assert!(is_excluded("export TOKEN=abc", false, &pats));
        assert!(!is_excluded("export", false, &pats));
        assert!(!is_excluded("ls -la", false, &pats));
        assert!(is_excluded("ls", false, &["ls".to_owned()]));
        assert!(!is_excluded("lsblk", false, &["ls".to_owned()]));
        assert!(is_excluded("lsblk", false, &["ls*".to_owned()]));
        assert!(!is_excluded("ab", false, &["a*b*b".to_owned()]));
    }

    #[tokio::test]
    async fn commands_roundtrip_search_and_filter() {
        let store = Store::open_memory().await.unwrap();
        store.add_command(cmd(Some("h1"), "ls -la")).await.unwrap();
        store
            .add_command(cmd(Some("h2"), "grep 100% log"))
            .await
            .unwrap();
        let local = store.add_command(cmd(None, "git status")).await.unwrap();
        assert_eq!(local.cwd.as_deref(), Some("/srv"));
        assert_eq!(local.exit_code, Some(0));

        let all = store.list_commands("", None, 100).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].command, "git status", "newest first");
        // LIKE wildcards are literal.
        assert_eq!(
            store.list_commands("100%", None, 100).await.unwrap().len(),
            1
        );
        assert!(
            store
                .list_commands("_", None, 100)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .list_commands("", Some("h1"), 100)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store.list_commands("", Some(""), 100).await.unwrap()[0].command,
            "git status"
        );

        store.delete_command(&local.id).await.unwrap();
        assert_eq!(store.list_commands("", None, 100).await.unwrap().len(), 2);
        assert_eq!(store.clear_history().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn connections_start_end_and_crash_recovery() {
        let store = Store::open_memory().await.unwrap();
        let id = store
            .connection_started(Some("h1"), "prod", "ssh")
            .await
            .unwrap();
        let open = store
            .connection_started(None, "pwsh", "local")
            .await
            .unwrap();
        store
            .connection_ended(&id, "user_closed", 10, 2)
            .await
            .unwrap();
        let list = store.list_connections(Some("h1"), 10).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].end_reason.as_deref(), Some("user_closed"));
        assert!((list[0].bytes_in - 10.0).abs() < f64::EPSILON);

        store.prune_history().await.unwrap();
        let all = store.list_connections(None, 10).await.unwrap();
        let crashed = all.iter().find(|c| c.id == open).unwrap();
        assert_eq!(crashed.end_reason.as_deref(), Some("unknown"));
    }
}
