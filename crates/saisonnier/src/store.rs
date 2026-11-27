use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedTask {
    pub schema_version: u32,
    pub task_id: String,
    pub created_at: String,
    pub prompt: String,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    NoChanges,
    Dirty,
    Failed,
    Crashed,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::NoChanges => "no_changes",
            Self::Dirty => "dirty",
            Self::Failed => "failed",
            Self::Crashed => "crashed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Running => "🤖",
            Self::Completed => "✔",
            Self::NoChanges => "💬",
            Self::Dirty | Self::Crashed => "⚠️",
            Self::Failed | Self::Cancelled => "✖",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            Self::Running => "cyan",
            Self::Completed => "green",
            Self::NoChanges => "yellow",
            Self::Dirty | Self::Crashed => "orange",
            Self::Failed | Self::Cancelled => "red",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReceipt {
    pub schema_version: u32,
    pub run_id: String,
    pub task_id: String,
    pub cep: String,
    pub branch: String,
    pub base: Option<String>,
    pub start_sha: String,
    pub end_sha: Option<String>,
    pub profile: String,
    pub prompt_file: PathBuf,
    pub log_path: PathBuf,
    pub session_path: PathBuf,
    pub session_id: String,
    pub tmux_session: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub status: RunStatus,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VignobleStore {
    root: PathBuf,
}

impl VignobleStore {
    pub fn from_git_common_dir(git_common_dir: &Path) -> Self {
        Self {
            root: git_common_dir.join("vignoble"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn queue_dir(&self) -> PathBuf {
        self.root.join("queue")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    pub fn tick_lock_path(&self) -> PathBuf {
        self.root.join("tick.lock")
    }

    pub fn task_path(&self, task_id: &str) -> PathBuf {
        self.queue_dir().join(format!("{}.json", task_id))
    }

    pub fn receipt_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(format!("{}.json", run_id))
    }

    pub fn prompt_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(format!("{}.prompt.md", run_id))
    }

    pub fn log_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(format!("{}.log", run_id))
    }

    pub fn session_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(format!("{}.session.jsonl", run_id))
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.queue_dir())?;
        fs::create_dir_all(self.runs_dir())?;
        Ok(())
    }

    /// Acquire an exclusive advisory lock on `tick.lock`.
    /// The returned `File` holds the lock until dropped.
    pub fn acquire_tick_lock(&self) -> Result<File> {
        fs::create_dir_all(&self.root)?;
        let lock_path = self.tick_lock_path();
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file {}", lock_path.display()))?;
        file.lock()
            .with_context(|| format!("Failed to lock {}", lock_path.display()))?;
        Ok(file)
    }

    pub fn write_task(&self, task: &QueuedTask) -> Result<PathBuf> {
        self.ensure_dirs()?;
        let path = self.task_path(&task.task_id);
        write_json_atomic(&path, task)?;
        Ok(path)
    }

    pub fn remove_task(&self, task_id: &str) -> Result<bool> {
        let path = self.task_path(task_id);
        if path.is_file() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn list_tasks(&self) -> Result<Vec<QueuedTask>> {
        let dir = self.queue_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut tasks = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read task {}", path.display()))?;
            let task: QueuedTask = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse task {}", path.display()))?;
            tasks.push(task);
        }
        tasks.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.task_id.cmp(&b.task_id))
        });
        Ok(tasks)
    }

    pub fn write_prompt(&self, run_id: &str, prompt: &str) -> Result<PathBuf> {
        self.ensure_dirs()?;
        let path = self.prompt_path(run_id);
        write_bytes_atomic(&path, prompt.as_bytes())?;
        Ok(path)
    }

    pub fn write_receipt(&self, receipt: &RunReceipt) -> Result<PathBuf> {
        self.ensure_dirs()?;
        let path = self.receipt_path(&receipt.run_id);
        write_json_atomic(&path, receipt)?;
        Ok(path)
    }

    pub fn read_receipt(&self, run_id: &str) -> Result<RunReceipt> {
        let path = self.receipt_path(run_id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read run receipt {}", path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse run receipt {}", path.display()))
    }

    pub fn remove_receipt(&self, run_id: &str) -> Result<bool> {
        let receipt_path = self.receipt_path(run_id);
        let existed = receipt_path.is_file();
        if existed {
            fs::remove_file(&receipt_path)?;
        }
        for path in [
            self.prompt_path(run_id),
            self.log_path(run_id),
            self.session_path(run_id),
        ] {
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
        Ok(existed)
    }

    pub fn list_receipts(&self) -> Result<Vec<RunReceipt>> {
        let dir = self.runs_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut receipts = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read run receipt {}", path.display()))?;
            let receipt: RunReceipt = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse run receipt {}", path.display()))?;
            receipts.push(receipt);
        }
        receipts.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.run_id.cmp(&a.run_id))
        });
        Ok(receipts)
    }

    pub fn find_task(&self, id: &str) -> Result<Option<QueuedTask>> {
        let tasks = self.list_tasks()?;
        if let Some(exact) = tasks.iter().find(|t| t.task_id == id) {
            return Ok(Some(exact.clone()));
        }
        let matches: Vec<&QueuedTask> =
            tasks.iter().filter(|t| t.task_id.starts_with(id)).collect();
        match matches.as_slice() {
            [only] => Ok(Some((*only).clone())),
            [] => Ok(None),
            _ => bail!("Task ID prefix '{}' is ambiguous", id),
        }
    }

    pub fn find_receipt(&self, id: &str) -> Result<Option<RunReceipt>> {
        let receipts = self.list_receipts()?;
        if let Some(exact) = receipts.iter().find(|r| r.run_id == id || r.task_id == id) {
            return Ok(Some(exact.clone()));
        }
        let matches: Vec<&RunReceipt> = receipts
            .iter()
            .filter(|r| r.run_id.starts_with(id) || r.task_id.starts_with(id))
            .collect();
        match matches.as_slice() {
            [only] => Ok(Some((*only).clone())),
            [] => Ok(None),
            _ => bail!("Run ID prefix '{}' is ambiguous", id),
        }
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let serialized = serde_json::to_string_pretty(value)?;
    write_bytes_atomic(path, serialized.as_bytes())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let seq = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("tmp");
    let tmp_path = parent.join(format!(".{}.tmp.{}.{}", file_name, std::process::id(), seq));
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_iso8601_utc() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_unix_millis_iso8601_utc(dur.as_secs(), dur.subsec_millis())
}

pub fn format_unix_iso8601_utc(secs: u64) -> String {
    let (year, month, day, hour, minute, second) = unix_secs_to_ymdhms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

pub fn format_unix_millis_iso8601_utc(secs: u64, millis: u32) -> String {
    let (year, month, day, hour, minute, second) = unix_secs_to_ymdhms(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, millis
    )
}

fn unix_secs_to_ymdhms(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = (rem / 3_600) as u32;
    let minute = ((rem % 3_600) / 60) as u32;
    let second = (rem % 60) as u32;

    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32, hour, minute, second)
}

pub fn parse_iso8601_utc_secs(ts: &str) -> Option<u64> {
    let trimmed = ts.strip_suffix('Z')?;
    let base = match trimmed.split_once('.') {
        Some((prefix, frac)) => {
            if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            prefix
        }
        None => trimmed,
    };
    let bytes = base.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year: i64 = base[0..4].parse().ok()?;
    let month: u32 = base[5..7].parse().ok()?;
    let day: u32 = base[8..10].parse().ok()?;
    let hour: u64 = base[11..13].parse().ok()?;
    let minute: u64 = base[14..16].parse().ok()?;
    let second: u64 = base[17..19].parse().ok()?;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    // Howard Hinnant's days-from-civil algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    if days < 0 {
        return None;
    }
    Some(days as u64 * 86_400 + hour * 3_600 + minute * 60 + second)
}

pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3_600 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        let hours = seconds / 3_600;
        let mins = (seconds % 3_600) / 60;
        format!("{}h{:02}m", hours, mins)
    }
}

pub fn generate_sortable_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let (year, month, day, hour, minute, second) = unix_secs_to_ymdhms(now.as_secs());
    let millis = now.subsec_millis();
    let mut bytes = [0u8; 3];
    fill_random_bytes(&mut bytes);
    let hex_suffix = format!("{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2]);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}{:03}-{}",
        year, month, day, hour, minute, second, millis, hex_suffix
    )
}

pub fn generate_uuid_v4() -> String {
    let mut b = [0u8; 16];
    fill_random_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn fill_random_bytes(dest: &mut [u8]) {
    #[cfg(unix)]
    if let Ok(mut file) = File::open("/dev/urandom") {
        if file.read_exact(dest).is_ok() {
            return;
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = nanos
        ^ ((std::process::id() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        ^ (ID_COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0xbf58_476d_1ce4_e5b9));
    for chunk in dest.chunks_mut(8) {
        state = splitmix64(state);
        let bytes = state.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso8601_roundtrip_known_timestamps() {
        assert_eq!(format_unix_iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(parse_iso8601_utc_secs("1970-01-01T00:00:00Z"), Some(0));

        // 2024-02-29T12:34:56Z (leap day)
        let ts = "2024-02-29T12:34:56Z";
        let secs = parse_iso8601_utc_secs(ts).unwrap();
        assert_eq!(format_unix_iso8601_utc(secs), ts);

        // Fractional milliseconds parse cleanly.
        assert_eq!(
            parse_iso8601_utc_secs("2024-02-29T12:34:56.789Z"),
            Some(secs)
        );
    }

    #[test]
    fn test_generate_uuid_v4_format() {
        let u1 = generate_uuid_v4();
        let u2 = generate_uuid_v4();
        assert_eq!(u1.len(), 36);
        assert_eq!(&u1[14..15], "4");
        assert_ne!(u1, u2);
    }

    #[test]
    fn test_generate_sortable_id_uniqueness() {
        let a = generate_sortable_id();
        let b = generate_sortable_id();
        assert_ne!(a, b);
        assert!(a.len() >= 22);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(42), "42s");
        assert_eq!(format_duration(125), "2m05s");
        assert_eq!(format_duration(3_665), "1h01m");
    }
}
