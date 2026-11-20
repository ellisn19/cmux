//! DRILL, NOT FOR A MERGE. Step 2 of 2 of a rollback drill with real
//! binaries. The NEWER build is this commit's `cmux-tui`; the OLDER build is
//! the copy that step 1 left in `/tmp` (`rollback_drill_copy.rs` at the older
//! commit). Each part has a state folder of its own, a real daemon of the
//! newer build that leaves its store at schema number 16, and then the
//! older build on the same store.
//!
//! The parts:
//! (a) the check beside a live owner, a stop by request, the older build;
//! (b) a kill of the owner, the check, the older build;
//! (c) a kill of the owner, NO check, the older build's `server ensure`;
//! (d) then the newer build opens and stops, and the older build opens;
//! (e) SIGTERM to the owner, the older build;
//! (f) the check beside a live owner, a journal row after it, the older
//!     build's `server premigrate-registry` beside the owner, a stop by
//!     request, the older build;
//! (g) for 8, 80 and 800 sessions whose owner was killed (real stores, the
//!     product's sync mode): the time of the older build's check, and, on a
//!     new set, of the newer build's first check and of the check after it;
//!     a check of the newer build at or above 5 s fails the part;
//! (h) a check of 80 such sessions gets SIGKILL in its work; the check
//!     after it, and the older build on the real session's store;
//! (i) for 8, 80 and 800 OLD stores (the older build's daemon made the store
//!     and stopped by request) and one live session beside them: the time of
//!     the older build's first check and of its check again, and, on a new
//!     set, of the newer build's first check and of its check again; a
//!     check must leave the main file of each old store as it was;
//! (j) for 800 sessions whose owner was killed: the newer build's check with
//!     an end time of its own (`--within-ms 5000`), asked again until it
//!     gives a number; an ask at or above 6 s fails the part.
//!
//! Each line starts with `rollback drill:`. The test deletes the copy at its
//! end, in a pass and in a fail.

#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The older build's copy, and what step 1 printed for it.
const COPY: &str = "/tmp/daemon-git-cost-drill-6e6a2996-cmux-tui";
const COPY_BYTES: u64 = 0;
const COPY_SHA256: &str = "SET-FROM-STEP-1";

const REGISTRY: &str = "workspace-registry.sqlite3";
const CONVERSATIONS: &str = "conversations.sqlite3";
const LOCKS: &str = "session-locks";

fn say(text: &str) {
    println!("rollback drill: {text}");
}

fn newer() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_cmux-tui"))
}

fn older() -> &'static Path {
    Path::new(COPY)
}

/// Deletes the copy when the test ends, in a pass and in a fail.
struct CopyGoesAtTheEnd;

impl Drop for CopyGoesAtTheEnd {
    fn drop(&mut self) {
        say(&format!("the delete of the copy: {:?}", fs::remove_file(COPY)));
    }
}

/// One part's folder under `/tmp` (a socket path must be short).
struct Folder(PathBuf);

impl Folder {
    fn new(part: &str) -> Self {
        let dir = PathBuf::from("/tmp").join(format!("cmux-drill-{part}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("state").join("sessions")).unwrap();
        Self(dir)
    }

    fn root(&self) -> PathBuf {
        self.0.join("state").join("sessions")
    }

    fn socket(&self) -> PathBuf {
        self.0.join("mux.sock")
    }

    /// The one session's folder: the one that has a registry.
    fn session_folder(&self) -> PathBuf {
        fs::read_dir(self.root())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.join(REGISTRY).is_file())
            .expect("the newer build's start makes the registry")
    }

    fn registry(&self) -> PathBuf {
        self.session_folder().join(REGISTRY)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(binary: &Path, folder: &Folder) -> Command {
    let mut command = Command::new(binary);
    command
        .env("CMUX_TUI_STATE_DIR", folder.root())
        .env("CMUX_TUI_CONFIG", folder.0.join("config.json"))
        .env("CMUX_TUI_LOG_FILE", folder.0.join("client.log"))
        .env("LC_ALL", "C")
        .env_remove("CMUX_TUI_SOCKET")
        .env_remove("CMUX_TUI_TERMINAL_ID")
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_CLIENT")
        .env_remove("SSH_TTY")
        .stdin(Stdio::null());
    command
}

/// What one run of a command gave: its exit code and stdout, and the text
/// `exit code, stdout, stderr`, each exact. The 2 outputs go to files of
/// `folder`, not to pipes: a daemon that the command leaves keeps a pipe
/// open, and the read of the pipe would not end.
fn result_of(
    mut command: Command,
    folder: &Folder,
    limit: Duration,
) -> (Option<i32>, String, String) {
    let (out, err) = (folder.0.join("command-stdout.txt"), folder.0.join("command-stderr.txt"));
    let mut child = command
        .stdout(fs::File::create(&out).unwrap())
        .stderr(fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + limit;
    let mut in_time = true;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            in_time = false;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let code = child.wait().unwrap().code();
    let stdout = fs::read_to_string(&out).unwrap_or_default().trim().to_string();
    let stderr = fs::read_to_string(&err).unwrap_or_default().trim().to_string();
    let late = if in_time { "" } else { "KILLED AT ITS TIME LIMIT, " };
    (
        code,
        stdout.clone(),
        format!("{late}exit code {code:?}, stdout {stdout:?}, stderr {stderr:?}"),
    )
}

/// The schema number in the store's file, by a read-only connection.
fn number(registry: &Path) -> i64 {
    let db = Connection::open_with_flags(registry, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

/// `__store-schemas --stored` of `binary`: its exit code and text, the
/// workspace registry's number in its answer, and its time.
fn check_by(binary: &Path, folder: &Folder) -> (String, Option<i64>, Duration) {
    let mut command = command(binary, folder);
    command.args(["__store-schemas", "--stored"]);
    let began = Instant::now();
    let (_, stdout, text) = result_of(command, folder, Duration::from_secs(300));
    let time = began.elapsed();
    let answer = serde_json::from_str::<Value>(&stdout)
        .ok()
        .and_then(|value| value.get("workspace_registry").and_then(Value::as_i64));
    (text, answer, time)
}

/// The newer build's check: its text and its answer.
fn check(folder: &Folder) -> (String, Option<i64>) {
    let (text, answer, _) = check_by(newer(), folder);
    (text, answer)
}

/// One request on a new connection to the daemon's socket; its answer.
fn request(folder: &Folder, request: Value) -> Result<Value, String> {
    let stream = UnixStream::connect(folder.socket()).map_err(|error| error.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).map_err(|error| error.to_string())?;
    let mut writer = stream.try_clone().map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    writeln!(writer, "{request}").map_err(|error| error.to_string())?;
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|error| error.to_string())?;
    serde_json::from_str(&line).map_err(|error| format!("{error}: {line:?}"))
}

/// A daemon of one build that owns the part's session.
struct Owner<'a> {
    child: Child,
    binary: &'static Path,
    folder: &'a Folder,
    session: String,
}

impl<'a> Owner<'a> {
    /// Starts `binary` as the session's daemon. `Err` = the daemon ended
    /// before it answered: its exit code and its stderr.
    fn start(binary: &'static Path, folder: &'a Folder, session: &str) -> Result<Self, String> {
        let _ = fs::remove_file(folder.socket());
        let stderr = folder.0.join("daemon-stderr.txt");
        let child = command(binary, folder)
            .args(["--headless", "--session", session, "--socket"])
            .arg(folder.socket())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let mut owner = Self { child, binary, folder, session: session.to_string() };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if owner.answers() {
                return Ok(owner);
            }
            let ended = owner.child.try_wait().unwrap();
            if ended.is_some() || Instant::now() >= deadline {
                let _ = owner.child.kill();
                let status = owner.child.wait().unwrap();
                let text = fs::read_to_string(&stderr).unwrap_or_default();
                let log = fs::read_to_string(folder.0.join("client.log")).unwrap_or_default();
                let log_end = log.lines().rev().take(6).collect::<Vec<_>>();
                return Err(format!(
                    "no answer (ended by itself: {}), exit code {:?}, stderr {:?}, the last \
                     lines of its log file, newest first: {log_end:?}",
                    ended.is_some(),
                    status.code(),
                    text.trim()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn server(&self, verb: &str) -> Command {
        let mut command = command(self.binary, self.folder);
        command.args(["--json", "server", verb, "--session", &self.session, "--socket"]);
        command.arg(self.folder.socket());
        command
    }

    fn answers(&self) -> bool {
        self.server("status").output().is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("\"running\"")
        })
    }

    /// A workspace with no terminal: a journal row and no terminal host.
    fn make_workspace(&self, seed: u64) -> String {
        let name = format!("drill-ws-{seed}");
        let created = request(
            self.folder,
            json!({"id": 1, "cmd": "create-workspace",
                   "key": format!("00000000-0000-4000-8000-{seed:012x}"), "name": name,
                   "origin": "user",
                   "mutation_id": format!("00000000-0000-4000-8000-{:012x}", seed + 100)}),
        );
        assert!(
            created.as_ref().is_ok_and(|answer| answer["ok"] == true),
            "create-workspace {name}: {created:?}"
        );
        name
    }

    /// Makes workspaces until the store's file is at 16 (a journal row
    /// waits for its subject rows). The names of the workspaces.
    fn leave_the_store_at_16(&self, first_seed: u64) -> Vec<String> {
        let mut names = Vec::new();
        for seed in first_seed..first_seed + 5 {
            names.push(self.make_workspace(seed));
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if number(&self.folder.registry()) == 16 {
                    return names;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        panic!("the store is not at 16 after 5 workspaces: {}", number(&self.folder.registry()));
    }

    /// Which of `names` the daemon lists.
    fn lists(&self, names: &[String]) -> String {
        let listed = request(self.folder, json!({"id": 2, "cmd": "list-workspaces"}));
        let text = format!("{listed:?}");
        let found = names.iter().filter(|name| text.contains(name.as_str())).count();
        format!("lists {found} of the {} workspaces {names:?}", names.len())
    }

    /// A stop by request; the daemon's exit code.
    fn stop(mut self) -> String {
        let began = Instant::now();
        let deadline = began + Duration::from_secs(30);
        loop {
            let _ = self.server("stop").output();
            let retry = Instant::now() + Duration::from_secs(1);
            while Instant::now() < retry {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return format!(
                        "stopped by request in {} ms, exit code {:?}",
                        began.elapsed().as_millis(),
                        status.code()
                    );
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return "DID NOT STOP in 30 s; killed".to_string();
            }
        }
    }

    /// SIGKILL, as a crash.
    fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// SIGTERM, then the wait for the exit.
    fn terminate(mut self) -> String {
        let began = Instant::now();
        // This test's own child, which it has not waited for.
        let sent =
            Command::new("kill").args(["-TERM", &self.child.id().to_string()]).status().unwrap();
        assert!(sent.success(), "kill -TERM: {sent:?}");
        let deadline = began + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                let after = began.elapsed().as_millis();
                return format!(
                    "ended {after} ms after SIGTERM (SIGKILL comes 5000 ms after it; above 3000 \
                     ms is too near: {}), exit code {:?}, by a signal: {}",
                    after > 3000,
                    status.code(),
                    status.code().is_none()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        "DID NOT END 30 s after SIGTERM; killed".to_string()
    }
}

impl Drop for Owner<'_> {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The older build as the session's daemon on the part's store: what a
/// person gets, then its stop.
fn the_older_build_opens(part: &str, folder: &Folder, session: &str, names: &[String]) -> bool {
    let before = number(&folder.registry());
    match Owner::start(older(), folder, session) {
        Ok(owner) => {
            say(&format!(
                "({part}) THE OLDER BUILD OPENS the store (its number before: {before}): it \
                 runs and {}; then {}",
                owner.lists(names),
                owner.stop()
            ));
            true
        }
        Err(text) => {
            say(&format!(
                "({part}) THE OLDER BUILD DOES NOT OPEN the store (its number before: \
                 {before}): {text}"
            ));
            false
        }
    }
}

/// The newer build's owner with the store at 16: the part's start.
fn at_16<'a>(part: &str, folder: &'a Folder, session: &str) -> (Owner<'a>, Vec<String>) {
    let owner = Owner::start(newer(), folder, session).unwrap();
    let names = owner.leave_the_store_at_16(1);
    say(&format!(
        "({part}) the newer build runs, {} workspaces made, the store's number is {}",
        names.len(),
        number(&folder.registry())
    ));
    (owner, names)
}

/// The control: the copy runs as a daemon from `/tmp` on a store of its
/// own. A copy that does not is no result of the drill.
fn the_copy_runs_as_a_daemon() {
    let folder = Folder::new("control");
    let owner = Owner::start(older(), &folder, "drill-control").unwrap_or_else(|text| {
        panic!("THE COPY DOES NOT RUN AS A DAEMON, no drill result: {text}")
    });
    let names = vec![owner.make_workspace(1)];
    say(&format!(
        "the control: the older build runs as a daemon on a store of its own (its number: \
         {}), {}; {}",
        number(&folder.registry()),
        owner.lists(&names),
        owner.stop()
    ));
}

fn part_a() -> bool {
    let folder = Folder::new("a");
    let (owner, names) = at_16("a", &folder, "drill-a");
    let (text, answer) = check(&folder);
    say(&format!(
        "(a) the check beside the live owner: {text}; the store's number after it: {}",
        number(&folder.registry())
    ));
    say(&format!(
        "(a) the newer build: {}; the store's number: {}",
        owner.stop(),
        number(&folder.registry())
    ));
    let opens = the_older_build_opens("a", &folder, "drill-a", &names);
    answer == Some(15) && opens
}

fn part_b() -> bool {
    let folder = Folder::new("b");
    let (owner, names) = at_16("b", &folder, "drill-b");
    owner.kill();
    let before = number(&folder.registry());
    let (text, answer) = check(&folder);
    say(&format!(
        "(b) the owner was killed (the store's number: {before}); the check: {text}; the \
         store's number after it: {}",
        number(&folder.registry())
    ));
    let opens = the_older_build_opens("b", &folder, "drill-b", &names);
    answer == Some(15) && opens
}

/// Parts (c) and (d): the hand case. No check runs before the older build.
fn parts_c_and_d() -> bool {
    let folder = Folder::new("c");
    let (owner, names) = at_16("c", &folder, "drill-c");
    owner.kill();
    say(&format!("(c) the owner was killed; the store's number: {}", number(&folder.registry())));
    // An owner that this `server ensure` starts ends with this test's
    // process (`--exit-with-pid`), also when the stop below does not end it.
    let mut ensure = command(older(), &folder);
    ensure.args(["--json", "server", "ensure", "--session", "drill-c", "--exit-with-pid"]);
    ensure.arg(std::process::id().to_string()).arg("--socket").arg(folder.socket());
    let (_, _, text) = result_of(ensure, &folder, Duration::from_secs(90));
    say(&format!("(c) THE OLDER BUILD's `server ensure` with no check before it: {text}"));
    let mut stop = command(older(), &folder);
    stop.args(["--json", "server", "stop", "--session", "drill-c", "--socket"]);
    stop.arg(folder.socket());
    let (_, _, text) = result_of(stop, &folder, Duration::from_secs(60));
    say(&format!(
        "(c) the older build's `server stop` after it: {text}; the store's number: {}",
        number(&folder.registry())
    ));
    let owner = Owner::start(newer(), &folder, "drill-c").unwrap();
    say(&format!(
        "(d) THE NEWER BUILD OPENS the same store again: it runs and {}; {}; the store's \
         number: {}",
        owner.lists(&names),
        owner.stop(),
        number(&folder.registry())
    ));
    the_older_build_opens("d", &folder, "drill-c", &names)
}

fn part_e() -> bool {
    let folder = Folder::new("e");
    let (owner, names) = at_16("e", &folder, "drill-e");
    say(&format!(
        "(e) the newer build's daemon: {}; the store's number: {}",
        owner.terminate(),
        number(&folder.registry())
    ));
    the_older_build_opens("e", &folder, "drill-e", &names)
}

fn part_f() -> bool {
    let folder = Folder::new("f");
    let (owner, mut names) = at_16("f", &folder, "drill-f");
    let (text, answer) = check(&folder);
    say(&format!("(f) the check beside the live owner: {text}"));
    names.extend(owner.leave_the_store_at_16(50));
    say(&format!(
        "(f) the owner ran on and made a workspace more; the store's number: {}",
        number(&folder.registry())
    ));
    let mut premigrate = command(older(), &folder);
    premigrate.args(["--json", "server", "premigrate-registry", "--session", "drill-f"]);
    premigrate.args(["--max-seconds", "60"]);
    let (_, _, text) = result_of(premigrate, &folder, Duration::from_secs(90));
    say(&format!(
        "(f) THE OLDER BUILD's `server premigrate-registry` beside the live owner: {text}"
    ));
    say(&format!(
        "(f) the newer build: {}; the store's number: {}",
        owner.stop(),
        number(&folder.registry())
    ));
    let opens = the_older_build_opens("f", &folder, "drill-f", &names);
    answer == Some(15) && opens
}

/// Sessions whose owner was killed with the store at 16. The first session
/// is a real one; the others are copies of its folder and of its guard
/// file. No store is read after the kill: a check finds each store as the
/// kill left it (the copies are new files, so the machine's file cache has
/// them: this is not a read from a cold disk).
struct Killed {
    folder: Folder,
    names: Vec<String>,
    copies: Copies,
}

/// The one session of a folder and its copies: `sessions` session folders,
/// each with the files and the guard file of the first.
struct Copies {
    registries: Vec<PathBuf>,
    size: u64,
    folder_bytes: u64,
    /// The names of the files of one session's folder.
    files: Vec<String>,
    /// The first session had a guard file, and each copy has a copy of it.
    guarded: bool,
}

impl Copies {
    /// `None` = not made: the copies would take too much of the disk.
    fn new(part: &str, folder: &Folder, sessions: usize) -> Option<Self> {
        let first = folder.session_folder();
        let session = first.file_name().unwrap().to_str().unwrap().to_string();
        let locks = folder.root().join(LOCKS);
        let guard = locks.join(format!("{session}.lock"));
        let guarded = guard.is_file();
        let found = fs::read_dir(&first)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        let folder_bytes = found.iter().map(|path| fs::metadata(path).unwrap().len()).sum::<u64>();
        let mut files = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        files.sort();
        // No part fills the disk of the machine: 4 GB of copies is the limit.
        if folder_bytes * u64::try_from(sessions).unwrap() > 4_000_000_000 {
            say(&format!(
                "({part}) {sessions} sessions: NOT RUN: a session's folder has {folder_bytes} \
                 bytes in the files {files:?}, and the copies would take more than 4 GB"
            ));
            return None;
        }
        let mut registries = vec![first.join(REGISTRY)];
        for copy in 1..sessions {
            let name = format!("{session}-{copy:03}");
            let to = folder.root().join(&name);
            fs::create_dir(&to).unwrap();
            for path in &found {
                fs::copy(path, to.join(path.file_name().unwrap())).unwrap();
            }
            if guarded {
                fs::copy(&guard, locks.join(format!("{name}.lock"))).unwrap();
            }
            registries.push(to.join(REGISTRY));
        }
        let size = fs::metadata(&registries[0]).unwrap().len();
        Some(Self { registries, size, folder_bytes, files, guarded })
    }

    /// The names of the files that the first session's folder has now.
    fn files_now(&self) -> Vec<String> {
        let mut files = fs::read_dir(self.registries[0].parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    /// How many of the stores are at `wanted`.
    fn at(&self, wanted: i64) -> usize {
        self.registries.iter().filter(|path| number(path) == wanted).count()
    }

    /// The SHA-256 of each store's main file, by a read of the file.
    fn hashes(&self) -> Vec<String> {
        self.registries
            .iter()
            .map(|path| {
                Sha256::digest(fs::read(path).unwrap())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            })
            .collect()
    }

    fn stores(&self) -> String {
        format!(
            "a store's file has {} bytes; a session's folder has {} bytes in the files {:?}; \
             the conversation store's file is in each folder: {}; each session has a guard \
             file: {}",
            self.size,
            self.folder_bytes,
            self.files,
            self.files.iter().any(|file| file == CONVERSATIONS),
            self.guarded
        )
    }
}

impl Killed {
    /// `None` = not made: the copies would take too much of the disk.
    fn new(part: &str, sessions: usize) -> Option<Self> {
        let folder = Folder::new(&format!("{part}{sessions}"));
        let (owner, names) = at_16(part, &folder, &format!("drill-{part}"));
        owner.kill();
        let copies = Copies::new(part, &folder, sessions)?;
        assert!(copies.guarded, "a killed owner leaves its session's guard file");
        Some(Self { folder, names, copies })
    }

    fn at(&self, wanted: i64) -> usize {
        self.copies.at(wanted)
    }

    fn stores(&self) -> String {
        format!(
            "REAL stores of the newer build's daemon, the product's sync mode; {}",
            self.copies.stores()
        )
    }
}

/// Part (g): 3 times for `sessions` sessions whose owner was killed with the
/// store at 16: the older build's check (it reads and writes nothing), then,
/// on a new set of the same stores, the newer build's first check and the
/// check after it. Its result and the newer build's first time.
fn part_g(sessions: usize) -> (bool, Duration) {
    let each = |time: Duration| time.as_secs_f64() * 1000.0 / sessions as f64;
    // The app waits 10 s for the check. Half of that is the limit here: a
    // person's disk is not this machine's disk.
    let limit = Duration::from_secs(5);
    let Some(set) = Killed::new("g", sessions) else {
        return (false, Duration::ZERO);
    };
    let (older_text, older_answer, older_time) = check_by(older(), &set.folder);
    say(&format!(
        "(g) {sessions} sessions whose owner was killed with the store at 16 ({}): THE OLDER \
         BUILD's CHECK (the code before this change: it reads each store and writes none) took \
         {} ms ({:.2} ms a session): {older_text}; its answer is 16: {}; {} of {sessions} \
         stores are at 16 after it; AT OR ABOVE 5000 ms (half of the app's 10 s): {}",
        set.stores(),
        older_time.as_millis(),
        each(older_time),
        older_answer == Some(16),
        set.at(16),
        older_time >= limit
    ));
    drop(set);
    let Some(set) = Killed::new("g", sessions) else {
        return (false, Duration::ZERO);
    };
    let (first_text, first_answer, first_time) = check_by(newer(), &set.folder);
    let (second_text, second_answer, second_time) = check_by(newer(), &set.folder);
    let at_15 = set.at(15);
    let too_long = first_time.max(second_time) >= limit;
    say(&format!(
        "(g) {sessions} sessions whose owner was killed with the store at 16, a new set ({}): \
         THE NEWER BUILD's FIRST CHECK after the daemon's end took {} ms ({:.2} ms a session): \
         {first_text}; THE CHECK AFTER IT took {} ms ({:.2} ms a session): {second_text}; \
         {at_15} of {sessions} stores are at 15 after the 2 checks; A CHECK AT OR ABOVE 5000 ms \
         (half of the app's 10 s): {too_long}",
        set.stores(),
        first_time.as_millis(),
        each(first_time),
        second_time.as_millis(),
        each(second_time)
    ));
    let good =
        first_answer == Some(15) && second_answer == Some(15) && at_15 == sessions && !too_long;
    (good, first_time)
}

/// Part (h): a check that is ended in its work, as the app ends it at its
/// time limit. `whole` is the time of a whole first check of as many
/// sessions; the check gets half of it, then SIGKILL. The stores that it
/// wrote must stay at 15, the check after it must write the others and say
/// 15, and the older build must open the real session.
fn part_h(sessions: usize, whole: Duration) -> bool {
    let Some(set) = Killed::new("h", sessions) else {
        return false;
    };
    let after = (whole / 2).max(Duration::from_millis(50));
    let mut ended = command(newer(), &set.folder);
    ended.args(["__store-schemas", "--stored"]).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = ended.spawn().unwrap();
    std::thread::sleep(after);
    let before_the_kill = child.try_wait().unwrap().is_some();
    let _ = child.kill();
    let status = child.wait().unwrap();
    let written = set.at(15);
    let real_at = number(&set.copies.registries[0]);
    let (text, answer, time) = check_by(newer(), &set.folder);
    let at_15 = set.at(15);
    let in_its_work = !before_the_kill && written > 0 && written < sessions;
    say(&format!(
        "(h) {sessions} sessions whose owner was killed with the store at 16 ({}): THE CHECK GOT \
         SIGKILL {} ms after its start (half of the {} ms of a whole first check); it ended \
         before the kill: {before_the_kill}; its exit code: {:?}; {written} of {sessions} \
         stores are at 15 after the kill (ENDED IN ITS WORK: {in_its_work}); the real \
         session's store is at {real_at}; THE CHECK AFTER IT took {} ms: {text}; {at_15} of \
         {sessions} stores are at 15 after it",
        set.stores(),
        after.as_millis(),
        whole.as_millis(),
        status.code(),
        time.as_millis()
    ));
    let opens = match Owner::start(older(), &set.folder, "drill-h") {
        Ok(owner) => {
            say(&format!(
                "(h) THE OLDER BUILD OPENS the real session's store: it runs and {}; then {}",
                owner.lists(&set.names),
                owner.stop()
            ));
            true
        }
        Err(text) => {
            say(&format!("(h) THE OLDER BUILD DOES NOT OPEN the real session's store: {text}"));
            false
        }
    };
    in_its_work && answer == Some(15) && at_15 == sessions && opens
}

/// One half of part (i): `sessions` OLD stores (the older build's daemon made
/// the first and stopped by request; the others are copies of its folder)
/// and one live session of `binary` beside them; then 2 checks of `binary`.
/// No store of the set is opened between the stop and the first check: the
/// first check is the first open of each store. The copies are new files, so
/// the machine's file cache has them: this is not a read from a cold disk.
fn old_stores(build: &str, binary: &'static Path, sessions: usize) -> bool {
    let each = |time: Duration| time.as_secs_f64() * 1000.0 / sessions as f64;
    let folder = Folder::new(&format!("i{build}{sessions}"));
    let maker = Owner::start(older(), &folder, "drill-old").unwrap();
    maker.make_workspace(1);
    let stopped = maker.stop();
    let Some(old) = Copies::new("i", &folder, sessions) else {
        return false;
    };
    let before = old.hashes();
    let live = Owner::start(binary, &folder, "drill-live").unwrap();
    let live_registry = fs::read_dir(folder.root())
        .unwrap()
        .map(|entry| entry.unwrap().path().join(REGISTRY))
        .find(|path| path.is_file() && !old.registries.contains(path))
        .expect("the live session's registry");
    // The newer build's live session gets journal rows until its file is at
    // 16; the older build's gets one workspace and stays at its number.
    let most = if binary == newer() { 5 } else { 1 };
    for seed in 1..=most {
        live.make_workspace(seed);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && number(&live_registry) != 16 {
            std::thread::sleep(Duration::from_millis(50));
        }
        if number(&live_registry) == 16 {
            break;
        }
    }
    let live_at = number(&live_registry);
    let (first_text, first_answer, first_time) = check_by(binary, &folder);
    let (again_text, again_answer, again_time) = check_by(binary, &folder);
    let changed = old.hashes().iter().zip(&before).filter(|(now, was)| now != was).count();
    let files_after = old.files_now();
    let at_15 = old.at(15);
    say(&format!(
        "(i) {sessions} OLD stores (the older build's daemon made the first and {stopped}; the \
         others are copies of its folder; {}) + 1 live session of the {build} build (the number \
         in its store's file: {live_at}): THE {build} BUILD's FIRST CHECK took {} ms ({:.2} ms \
         a session): {first_text}; ITS CHECK AGAIN took {} ms ({:.2} ms a session): \
         {again_text}; A FIRST CHECK AT OR ABOVE 5000 ms (half of the app's 10 s): {}; the main \
         file of {changed} of {sessions} old stores changed in the 2 checks (SHA-256); \
         {at_15} of {sessions} old stores are at 15 after them; the files of an old session's \
         folder after them: {files_after:?}; the live session {}",
        old.stores(),
        first_time.as_millis(),
        each(first_time),
        again_time.as_millis(),
        each(again_time),
        first_time >= Duration::from_secs(5),
        live.stop()
    ));
    first_answer == Some(15) && again_answer == Some(15) && changed == 0 && at_15 == sessions
}

/// Part (i): the old stores with the older build's check (the code before
/// this change), then on a new set with the newer build's check. A time is
/// not in the result: the read of an old store is the older build's code.
fn part_i(sessions: usize) -> bool {
    let by_the_older = old_stores("older", older(), sessions);
    let by_the_newer = old_stores("newer", newer(), sessions);
    by_the_older && by_the_newer
}

/// Part (j): the newer build's check with an end time of its own, as the
/// app's half of the mend will ask it: `--stored --within-ms N` for
/// `sessions` sessions whose owner was killed with the store at 16, asked
/// again until it gives a number. Each ask: its time, its exit code and
/// answer, and how many stores are at 15 after it. The check reads its
/// clock only between 2 sessions, so an ask can end after its N ms by one
/// session's write; the part fails at an ask that took N + 1000 ms or more.
fn part_j(sessions: usize, within: Duration) -> bool {
    let Some(set) = Killed::new("j", sessions) else {
        return false;
    };
    let millis = within.as_millis().to_string();
    let limit = within + Duration::from_secs(1);
    let mut asks: Vec<String> = Vec::new();
    let mut slowest = Duration::ZERO;
    let mut answer: Option<i64> = None;
    // 40 asks at the most: a check that never gives a number ends the part.
    while answer.is_none() && asks.len() < 40 {
        let mut ask = command(newer(), &set.folder);
        ask.args(["__store-schemas", "--stored", "--within-ms", millis.as_str()]);
        let began = Instant::now();
        let (code, stdout, _) = result_of(ask, &set.folder, Duration::from_secs(300));
        let time = began.elapsed();
        slowest = slowest.max(time);
        answer = serde_json::from_str::<Value>(&stdout)
            .ok()
            .and_then(|value| value.get("workspace_registry").and_then(Value::as_i64));
        let ask_number = asks.len() + 1;
        let written = set.at(15);
        asks.push(format!(
            "ask {ask_number}: {} ms, exit code {code:?}, stdout {stdout:?}, {written} of \
             {sessions} stores at 15 after it",
            time.as_millis()
        ));
    }
    let at_15 = set.at(15);
    let too_long = slowest >= limit;
    say(&format!(
        "(j) {sessions} sessions whose owner was killed with the store at 16 ({}): THE NEWER \
         BUILD's CHECK WITH `--within-ms {millis}`, asked again until it gives a number: {} \
         asks; {}; the slowest ask took {} ms; AN ASK AT OR ABOVE {} ms (its end time and 1000 \
         ms): {too_long}; the last answer is 15: {}; {at_15} of {sessions} stores are at 15 \
         after the last ask",
        set.stores(),
        asks.len(),
        asks.join("; "),
        slowest.as_millis(),
        limit.as_millis(),
        answer == Some(15)
    ));
    answer == Some(15) && at_15 == sessions && !too_long
}

#[test]
fn rollback_drill_with_the_older_builds_copy() {
    let _copy = CopyGoesAtTheEnd;
    // The copy first: a file that is not step 1's is not run.
    let bytes = fs::read(COPY).expect("step 1 left the older build's copy");
    let hash: String = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    let length = u64::try_from(bytes.len()).unwrap();
    drop(bytes);
    assert_eq!(
        (length, hash.as_str()),
        (COPY_BYTES, COPY_SHA256),
        "THE COPY IS NOT STEP 1's FILE (bytes, SHA-256): this is no result of the drill"
    );
    say(&format!("the copy has step 1's {length} bytes and SHA-256 {hash}"));
    let folder = Folder::new("reads");
    for (build, binary) in [("older", older()), ("newer", newer())] {
        let mut readable = command(binary, &folder);
        readable.arg("__store-schemas");
        let (_, _, text) = result_of(readable, &folder, Duration::from_secs(30));
        say(&format!("the {build} build reads: {text}"));
    }
    drop(folder);
    the_copy_runs_as_a_daemon();
    let mut results = vec![
        ("a", part_a()),
        ("b", part_b()),
        ("c and d", parts_c_and_d()),
        ("e", part_e()),
        ("f", part_f()),
    ];
    let mut whole_at_80 = Duration::ZERO;
    for (name, sessions) in [("g 8", 8), ("g 80", 80), ("g 800", 800)] {
        let (good, first_time) = part_g(sessions);
        if sessions == 80 {
            whole_at_80 = first_time;
        }
        results.push((name, good));
    }
    results.push(("h", part_h(80, whole_at_80)));
    for (name, sessions) in [("i 8", 8), ("i 80", 80), ("i 800", 800)] {
        results.push((name, part_i(sessions)));
    }
    results.push(("j 800", part_j(800, Duration::from_secs(5))));
    say(&format!(
        "EACH PART (a to f: true = the older build opened the store at the part's end, and a \
         check beside a live owner said 15; g: true = both checks of the newer build said 15, \
         each store is at 15, and each of the 2 took less than 5000 ms; h: true = the check \
         was ended in its work, the check after it said 15, each store is at 15, and the older \
         build opened the real session's store; i: true = each of the 4 checks said 15, the \
         main file of no old store changed, and each old store is at 15; its times are not in \
         the result; j: true = the last ask with an end time of 5000 ms said 15, each store is \
         at 15, and each ask took less than 6000 ms): {results:?}"
    ));
    assert!(results.iter().all(|(_, good)| *good), "{results:?}");
}
