//! Crash journals are independent of the user's original files.
use crate::{document::Result, files};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub draft: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}
/// Expensive state/font encoding runs in the journal worker, after debouncing.
#[derive(Clone)]
pub struct PendingState {
    pub key: String,
    pub encode: Arc<dyn Fn() -> Result<String> + Send + Sync>,
}
#[derive(Clone)]
pub struct Entry {
    pub pdf: PathBuf,
    pub metadata: Metadata,
}
enum Command {
    Put(
        u64,
        Arc<Vec<u8>>,
        Metadata,
        Option<PendingState>,
        Option<PathBuf>,
    ),
    Remove(u64),
}
pub struct Journal {
    pub entries: Vec<Entry>,
    directory: Option<PathBuf>,
    sender: Option<mpsc::Sender<Command>>,
    errors: mpsc::Receiver<String>,
    keys: HashMap<u64, (u64, String)>,
    adopted: HashMap<u64, PathBuf>,
    _lock: Option<File>,
}
impl Journal {
    pub fn disabled() -> Self {
        Self {
            entries: vec![],
            directory: None,
            sender: None,
            errors: mpsc::channel().1,
            keys: HashMap::new(),
            adopted: HashMap::new(),
            _lock: None,
        }
    }
    pub fn start(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        for dir in std::fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = dir.path();
            if !dir.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Ok(lock) = File::options()
                .read(true)
                .write(true)
                .open(path.join("lock"))
            else {
                continue;
            };
            if lock.try_lock().is_err() {
                continue;
            }
            if path.join("clean").exists() {
                if let Ok(list) = std::fs::read_dir(&path) {
                    for item in list.flatten() {
                        let p = item.path();
                        if p.extension().is_some_and(|e| e == "pdf" || e == "json") {
                            let _ = std::fs::remove_file(p);
                        }
                    }
                }
                drop(lock);
                let _ = std::fs::remove_file(path.join("lock"));
                let _ = std::fs::remove_file(path.join("clean"));
                let _ = std::fs::remove_dir(&path);
                continue;
            }
            if let Ok(list) = std::fs::read_dir(&path) {
                for item in list.flatten() {
                    let p = item.path();
                    if p.extension().is_some_and(|e| e == "json")
                        && let Ok(bytes) = files::read(&p)
                        && let Ok(metadata) = serde_json::from_slice::<Metadata>(&bytes)
                        && let Some(pdf) = checkpoint_path(&p, &metadata)
                        && pdf.is_file()
                    {
                        entries.push(Entry { pdf, metadata });
                    }
                }
            }
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = root.join(format!("{}-{now}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let lock = File::options()
            .create_new(true)
            .read(true)
            .write(true)
            .open(directory.join("lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| e.to_string())?;
        let (sender, rx) = mpsc::channel();
        let (tx, errors) = mpsc::channel();
        let target = directory.clone();
        std::thread::spawn(move || {
            let mut pending = HashMap::new();
            let mut since = Instant::now();
            let mut last_error = None;
            loop {
                let received = rx.recv_timeout(Duration::from_millis(600));
                let idle = received.is_err();
                match received {
                    Ok(Command::Put(id, bytes, meta, state, old)) => {
                        pending.insert(id, (bytes, meta, state, old));
                    }
                    Ok(Command::Remove(id)) => {
                        pending.remove(&id);
                        remove_checkpoint(&target.join(format!("{id}.json")));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if since.elapsed() >= Duration::from_secs(2) || idle {
                    let mut failed = Vec::new();
                    for (id, (bytes, mut meta, state, old)) in pending.drain() {
                        let result =
                            state
                                .as_ref()
                                .map(|s| (s.encode)())
                                .transpose()
                                .and_then(|encoded| {
                                    meta.state = encoded;
                                    store(&target, id, &bytes, &meta)
                                });
                        if let Err(e) = result {
                            failed.push((id, (bytes, meta, state, old)));
                            if last_error.as_ref() == Some(&e) {
                                continue;
                            }
                            last_error = Some(e.clone());
                            let _ = tx.send(format!(
                                "La copie de récupération n’a pas pu être écrite : {e}"
                            ));
                        } else {
                            last_error = None;
                            if let Some(old) = old
                                && let Some(sidecar) = sidecar_for_pdf(&old)
                            {
                                remove_checkpoint(&sidecar);
                            }
                        }
                    }
                    pending.extend(failed);
                    since = Instant::now();
                }
            }
        });
        Ok(Self {
            entries,
            directory: Some(directory),
            sender: Some(sender),
            errors,
            keys: HashMap::new(),
            adopted: HashMap::new(),
            _lock: Some(lock),
        })
    }
    pub fn observe(
        &mut self,
        id: u64,
        revision: u64,
        bytes: Arc<Vec<u8>>,
        name: String,
        draft: String,
        dirty: bool,
    ) {
        self.observe_state(id, revision, bytes, name, draft, dirty, None);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn observe_state(
        &mut self,
        id: u64,
        revision: u64,
        bytes: Arc<Vec<u8>>,
        name: String,
        draft: String,
        dirty: bool,
        state: Option<PendingState>,
    ) {
        if !dirty {
            self.remove(id);
            return;
        }
        let key = state
            .as_ref()
            .map(|s| s.key.clone())
            .unwrap_or_else(|| draft.clone());
        if self
            .keys
            .get(&id)
            .is_some_and(|k| k.0 == revision && k.1 == key)
        {
            return;
        }
        self.keys.insert(id, (revision, key));
        if let Some(tx) = &self.sender {
            let _ = tx.send(Command::Put(
                id,
                bytes,
                Metadata {
                    name,
                    draft,
                    state: None,
                    checkpoint: None,
                },
                state,
                self.adopted.get(&id).cloned(),
            ));
        }
    }
    pub fn adopt(&mut self, id: u64, old: PathBuf) {
        if self
            .directory
            .as_ref()
            .and_then(|d| d.parent())
            .is_some_and(|root| old.starts_with(root))
            && old.extension().is_some_and(|e| e == "pdf")
        {
            self.adopted.insert(id, old);
        }
    }
    pub fn remove(&mut self, id: u64) {
        self.adopted.remove(&id);
        if self.keys.remove(&id).is_some()
            && let Some(tx) = &self.sender
        {
            let _ = tx.send(Command::Remove(id));
        }
    }
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
    pub fn active(&self) -> bool {
        !self.keys.is_empty()
    }
    pub fn clean_exit(&self) {
        if let Some(dir) = &self.directory {
            let _ = files::atomic_write(&dir.join("clean"), b"closed normally");
        }
    }
}
fn store(directory: &Path, id: u64, bytes: &[u8], meta: &Metadata) -> Result<()> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("{id}.{stamp}.{sequence}.pdf");
    let pdf = directory.join(&name);
    let sidecar = directory.join(format!("{id}.json"));
    let previous = files::read(&sidecar)
        .ok()
        .and_then(|b| serde_json::from_slice::<Metadata>(&b).ok())
        .and_then(|m| checkpoint_path(&sidecar, &m));
    let mut metadata = meta.clone();
    metadata.checkpoint = Some(name);
    let encoded = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
    files::atomic_write(&pdf, bytes)?;
    // The old PDF stays intact until this atomic pointer/state update succeeds.
    if let Err(error) = files::atomic_write(&sidecar, &encoded) {
        let _ = std::fs::remove_file(pdf);
        return Err(error);
    }
    if let Some(previous) = previous {
        let _ = std::fs::remove_file(previous);
    }
    Ok(())
}
fn checkpoint_path(sidecar: &Path, meta: &Metadata) -> Option<PathBuf> {
    let Some(name) = &meta.checkpoint else {
        return Some(sidecar.with_extension("pdf"));
    };
    let id = sidecar.file_stem()?.to_str()?;
    let path = Path::new(name);
    (path.components().count() == 1
        && path.file_name()?.to_str()? == name
        && name.starts_with(&format!("{id}."))
        && path.extension().is_some_and(|e| e == "pdf"))
    .then(|| sidecar.with_file_name(name))
}
fn sidecar_for_pdf(pdf: &Path) -> Option<PathBuf> {
    let stem = pdf.file_stem()?.to_str()?;
    let id = stem.split('.').next()?;
    id.parse::<u64>().ok()?;
    Some(pdf.with_file_name(format!("{id}.json")))
}
fn remove_checkpoint(sidecar: &Path) {
    let pdf = files::read(sidecar)
        .ok()
        .and_then(|b| serde_json::from_slice::<Metadata>(&b).ok())
        .and_then(|m| checkpoint_path(sidecar, &m));
    let _ = std::fs::remove_file(sidecar);
    if let Some(pdf) = pdf {
        let _ = std::fs::remove_file(pdf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_only_changes_are_coalesced_and_encoded_off_the_ui_thread() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let mut journal = Journal::start(temp.path()).unwrap();
        let dir = journal.directory.clone().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let caller = std::thread::current().id();
        for (key, step) in [("regular", 100), ("bold", 1)] {
            let calls = calls.clone();
            let state = PendingState {
                key: key.into(),
                encode: Arc::new(move || {
                    assert_ne!(std::thread::current().id(), caller);
                    calls.fetch_add(step, Ordering::SeqCst);
                    Ok(key.into())
                }),
            };
            journal.observe_state(
                1,
                1,
                Arc::new(b"same PDF".to_vec()),
                "Cours.pdf".into(),
                "same text".into(),
                true,
                Some(state),
            );
        }
        for _ in 0..250 {
            if dir.join("1.json").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let meta: Metadata =
            serde_json::from_slice(&files::read(&dir.join("1.json")).unwrap()).unwrap();
        assert_eq!(meta.state.as_deref(), Some("bold"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        journal.clean_exit();
    }
    #[test]
    fn uncommitted_generation_never_pairs_new_pdf_with_old_styles_and_paths_cannot_escape() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let meta = Metadata {
            name: "Cours.pdf".into(),
            draft: "ancien".into(),
            state: Some("old styles".into()),
            checkpoint: None,
        };
        store(temp.path(), 7, b"old PDF", &meta).unwrap();
        let sidecar = temp.path().join("7.json");
        let saved = files::read(&sidecar).unwrap();
        // A crash after writing the next generation, before its atomic publication.
        std::fs::write(temp.path().join("7.orphan.pdf"), b"new PDF").unwrap();
        let mut old: Metadata = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            files::read(&checkpoint_path(&sidecar, &old).unwrap()).unwrap(),
            b"old PDF"
        );
        assert_eq!(old.state.as_deref(), Some("old styles"));
        old.checkpoint = Some("../7.secret.pdf".into());
        assert!(checkpoint_path(&sidecar, &old).is_none());
        old.checkpoint = Some("8.other.pdf".into());
        assert!(checkpoint_path(&sidecar, &old).is_none());
    }
    #[test]
    #[cfg(windows)]
    fn failed_sidecar_replacement_keeps_the_previous_complete_checkpoint() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let meta = Metadata {
            name: "Cours.pdf".into(),
            draft: "avant".into(),
            state: Some("styles".into()),
            checkpoint: None,
        };
        store(temp.path(), 4, b"before", &meta).unwrap();
        let sidecar = temp.path().join("4.json");
        let saved = files::read(&sidecar).unwrap();
        let lock = File::options()
            .read(true)
            .share_mode(0)
            .open(&sidecar)
            .unwrap();
        assert!(store(temp.path(), 4, b"after", &meta).is_err());
        drop(lock);
        assert_eq!(files::read(&sidecar).unwrap(), saved);
        let old: Metadata = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            files::read(&checkpoint_path(&sidecar, &old).unwrap()).unwrap(),
            b"before"
        );
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|v| v == "pdf"))
                .count(),
            1
        );
    }
    #[test]
    fn inactive_journals_recover_pdf_and_unicode_draft_but_live_sessions_are_excluded() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let first = Journal::start(temp.path()).unwrap();
        let dir = first.directory.clone().unwrap();
        store(
            &dir,
            7,
            b"PDF snapshot",
            &Metadata {
                name: "Cours.pdf".into(),
                draft: "été œ 42".into(),
                state: None,
                checkpoint: None,
            },
        )
        .unwrap();
        let second = Journal::start(temp.path()).unwrap();
        assert!(second.entries.is_empty());
        drop(first);
        let third = Journal::start(temp.path()).unwrap();
        assert_eq!(third.entries.len(), 1);
        assert_eq!(third.entries[0].metadata.draft, "été œ 42");
        assert_eq!(
            std::fs::read(&third.entries[0].pdf).unwrap(),
            b"PDF snapshot"
        );
        second.clean_exit();
    }
    #[test]
    fn adopted_checkpoint_replaces_the_old_journal_only_after_the_new_files_are_written() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let temp = tempfile::tempdir_in(root).unwrap();
        let old = Journal::start(temp.path()).unwrap();
        let old_dir = old.directory.clone().unwrap();
        store(
            &old_dir,
            1,
            b"old checkpoint",
            &Metadata {
                name: "Cours.pdf".into(),
                draft: "brouillon".into(),
                state: None,
                checkpoint: None,
            },
        )
        .unwrap();
        drop(old);
        let mut new = Journal::start(temp.path()).unwrap();
        assert_eq!(new.entries.len(), 1);
        let source = new.entries[0].pdf.clone();
        new.adopt(9, source.clone());
        new.observe(
            9,
            4,
            Arc::new(b"new checkpoint".to_vec()),
            "Cours.pdf".into(),
            "brouillon".into(),
            true,
        );
        let sidecar = new.directory.as_ref().unwrap().join("9.json");
        for _ in 0..100 {
            if sidecar.exists() && !source.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let meta = serde_json::from_slice::<Metadata>(&std::fs::read(&sidecar).unwrap()).unwrap();
        let target = checkpoint_path(&sidecar, &meta).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new checkpoint");
        assert!(!source.exists());
        new.clean_exit();
        drop(new);
        let next = Journal::start(temp.path()).unwrap();
        assert!(next.entries.is_empty());
    }
}
