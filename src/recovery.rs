//! Crash journals are independent of the user's original files.
use crate::{document::Result, files};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub draft: String,
}
// New checkpoints include the PDF length so a torn PDF/JSON pair is ignored.
// Older journals without pdf_length remain recoverable.
#[derive(Serialize, Deserialize)]
struct StoredMetadata {
    #[serde(flatten)]
    metadata: Metadata,
    #[serde(default)]
    pdf_length: Option<u64>,
    #[serde(default)]
    pdf_checksum: Option<u64>,
}

#[derive(Clone)]
pub struct Entry {
    pub pdf: PathBuf,
    pub metadata: Metadata,
}
enum Command {
    Put(u64, Arc<Vec<u8>>, Metadata, Option<PathBuf>),
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
                        && p.with_extension("pdf").is_file()
                        && let Ok(bytes) = files::read(&p)
                        && let Ok(stored) = serde_json::from_slice::<StoredMetadata>(&bytes)
                        && stored.pdf_length.is_none_or(|expected| {
                            std::fs::metadata(p.with_extension("pdf"))
                                .is_ok_and(|actual| actual.len() == expected)
                        })
                        && stored.pdf_checksum.is_none_or(|expected| {
                            checksum_file(&p.with_extension("pdf"))
                                .is_ok_and(|actual| actual == expected)
                        })
                    {
                        entries.push(Entry {
                            pdf: p.with_extension("pdf"),
                            metadata: stored.metadata,
                        });
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
                let disconnected = matches!(&received, Err(mpsc::RecvTimeoutError::Disconnected));
                match received {
                    Ok(Command::Put(id, bytes, meta, old)) => {
                        pending.insert(id, (bytes, meta, old));
                    }
                    Ok(Command::Remove(id)) => {
                        pending.remove(&id);
                        for extension in ["pdf", "json"] {
                            let _ = std::fs::remove_file(target.join(format!("{id}.{extension}")));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {}
                }
                if since.elapsed() >= Duration::from_secs(2) || idle {
                    let mut failed = Vec::new();
                    for (id, (bytes, meta, old)) in pending.drain() {
                        if let Err(e) = store(&target, id, &bytes, &meta) {
                            failed.push((id, (bytes, meta, old)));
                            if last_error.as_ref() == Some(&e) {
                                continue;
                            }
                            last_error = Some(e.clone());
                            let _ = tx.send(format!(
                                "La copie de récupération n’a pas pu être écrite : {e}"
                            ));
                        } else {
                            last_error = None;
                            if let Some(old) = old {
                                let _ = std::fs::remove_file(old.with_extension("json"));
                                let _ = std::fs::remove_file(old);
                            }
                        }
                    }
                    pending.extend(failed);
                    since = Instant::now();
                }
                if disconnected {
                    break;
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
        if !dirty {
            self.remove(id);
            return;
        }
        if self
            .keys
            .get(&id)
            .is_some_and(|k| k.0 == revision && k.1 == draft)
        {
            return;
        }
        self.keys.insert(id, (revision, draft.clone()));
        if let Some(tx) = &self.sender {
            let _ = tx.send(Command::Put(
                id,
                bytes,
                Metadata { name, draft },
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
// FNV-1a is a deterministic accidental-corruption checksum, not a security signature.
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn checksum_update(mut checksum: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        checksum = (checksum ^ u64::from(*byte)).wrapping_mul(FNV_PRIME);
    }
    checksum
}

fn checksum_file(path: &Path) -> Result<u64> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut checksum = FNV_OFFSET;
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(checksum);
        }
        checksum = checksum_update(checksum, &buffer[..count]);
    }
}

fn store(directory: &Path, id: u64, bytes: &[u8], meta: &Metadata) -> Result<()> {
    files::atomic_write(&directory.join(format!("{id}.pdf")), bytes)?;
    files::atomic_write(
        &directory.join(format!("{id}.json")),
        &serde_json::to_vec(&StoredMetadata {
            metadata: meta.clone(),
            pdf_length: Some(bytes.len() as u64),
            pdf_checksum: Some(checksum_update(FNV_OFFSET, bytes)),
        })
        .map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let target = new.directory.as_ref().unwrap().join("9.pdf");
        for _ in 0..100 {
            if target.with_extension("json").exists() && !source.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(std::fs::read(&target).unwrap(), b"new checkpoint");
        assert!(!source.exists());
        new.clean_exit();
        drop(new);
        let next = Journal::start(temp.path()).unwrap();
        assert!(next.entries.is_empty());
    }
    #[test]
    fn incomplete_snapshot_is_ignored_but_legacy_metadata_still_recovers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let temp = tempfile::tempdir_in(root).unwrap();
        let first = Journal::start(temp.path()).unwrap();
        let dir = first.directory.clone().unwrap();
        let meta = Metadata {
            name: "Cours de français.pdf".into(),
            draft: "modifications non sauvegardées".into(),
        };
        store(&dir, 4, b"original snapshot", &meta).unwrap();
        // Simulate a torn checkpoint whose metadata still describes the old PDF.
        std::fs::write(dir.join("4.pdf"), b"xxxxxxxxxxxxxxxxx").unwrap();
        assert_eq!(std::fs::metadata(dir.join("4.pdf")).unwrap().len(), 17);
        drop(first);
        let second = Journal::start(temp.path()).unwrap();
        assert!(second.entries.is_empty());
        drop(second);

        // Checkpoints created by v1.0.0 did not carry either integrity field.
        std::fs::write(dir.join("4.json"), serde_json::to_vec(&meta).unwrap()).unwrap();
        let third = Journal::start(temp.path()).unwrap();
        assert_eq!(third.entries.len(), 1);
        assert_eq!(third.entries[0].metadata.name, "Cours de français.pdf");
    }

    #[test]
    fn pending_checkpoint_is_written_when_sender_disconnects() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let temp = tempfile::tempdir_in(root).unwrap();
        let mut journal = Journal::start(temp.path()).unwrap();
        let path = journal.directory.as_ref().unwrap().join("15.pdf");
        journal.observe(
            15,
            1,
            Arc::new(b"saved draft".to_vec()),
            "Cours.pdf".into(),
            "été".into(),
            true,
        );
        drop(journal);
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"saved draft");
    }
}
