use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use memchr::memmem;

const MAGIC: &[u8; 8] = b"RAIDENCT";
const VERSION: u32 = 3;
const SNIFFED: usize = 8 * 1024;
const SHOWN_CHARACTERS: usize = 200;
const SHARD_BYTES: usize = 4 * 1024 * 1024;
const FEWEST_ROWS_LOG2: u32 = 8;
const MOST_ROWS_LOG2: u32 = 16;

pub const LARGEST: u64 = 1024 * 1024;

const LEFT_OUT: [&str; 55] = [
    "7z", "a", "aac", "ai", "avi", "bin", "bmp", "class", "csv", "dmg", "doc", "docx", "dylib",
    "eot", "exe", "flac", "gif", "gz", "heic", "icns", "ico", "iso", "jar", "jpeg", "jpg", "key",
    "lock", "log", "m4a", "map", "mov", "mp3", "mp4", "numbers", "o", "otf", "pages", "pdf", "png",
    "ppt", "pptx", "psd", "pyc", "rlib", "so", "sqlite", "svg", "tar", "tif", "tiff", "tsv", "ttf",
    "wasm", "woff", "zip",
];
const GENERATED: [&str; 4] = ["package-lock.json", "pnpm-lock.yaml", ".min.js", ".min.css"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub bits_per_trigram: u32,
    pub hashes: u32,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            bits_per_trigram: 4,
            hashes: 2,
        }
    }
}

impl Shape {
    fn rows_log2(self, trigrams: usize) -> u32 {
        let wanted = (trigrams.max(1) as u64).saturating_mul(u64::from(self.bits_per_trigram));
        let log2 = u64::BITS - wanted.saturating_sub(1).leading_zeros();
        log2.clamp(FEWEST_ROWS_LOG2, MOST_ROWS_LOG2)
    }
}

/// ```
/// use raiden_core::contents::readable;
/// assert!(readable("/Users/me/notes.md", 2_000));
/// assert!(!readable("/Users/me/photo.JPG", 2_000));
/// assert!(!readable("/Users/me/app/package-lock.json", 2_000));
/// assert!(!readable("/Users/me/app/dist/app.min.js", 2_000));
/// assert!(!readable("/Users/me/huge.log", 5 * 1024 * 1024));
/// assert!(!readable("/Users/me/empty.txt", 0));
/// ```
pub fn readable(path: &str, size: u64) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    size > 0
        && size <= LARGEST
        && extension.is_none_or(|extension| !LEFT_OUT.contains(&extension))
        && !GENERATED.iter().any(|generated| name.ends_with(generated))
}

fn fold(byte: u8) -> u8 {
    byte.to_ascii_lowercase()
}

fn trigram(window: &[u8]) -> u32 {
    (u32::from(fold(window[0])) << 16)
        | (u32::from(fold(window[1])) << 8)
        | u32::from(fold(window[2]))
}

/// ```
/// use raiden_core::contents::trigrams;
/// assert_eq!(trigrams(b"abcab").len(), 3);
/// assert_eq!(trigrams(b"ABC"), trigrams(b"abc"));
/// assert!(trigrams(b"ab").is_empty());
/// ```
pub fn trigrams(text: &[u8]) -> Vec<u32> {
    thread_local! {
        static SEEN: std::cell::RefCell<Vec<u64>> = std::cell::RefCell::new(vec![0; (1 << 24) / 64]);
    }
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let mut found = Vec::new();
        for window in text.windows(3) {
            let trigram = trigram(window);
            let (word, bit) = ((trigram / 64) as usize, 1_u64 << (trigram % 64));
            if seen[word] & bit == 0 {
                seen[word] |= bit;
                found.push(trigram);
            }
        }
        for trigram in &found {
            seen[(trigram / 64) as usize] = 0;
        }
        found.sort_unstable();
        found
    })
}

fn rows_of(trigram: u32, rows_log2: u32, hashes: u32) -> impl Iterator<Item = usize> {
    let mixed = u64::from(trigram).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mask = (1_u64 << rows_log2) - 1;
    (0..hashes).map(move |at| {
        let spun = mixed.rotate_left(at * 21) ^ (mixed >> 29);
        ((spun.wrapping_mul(0xBF58_476D_1CE4_E5B9) >> 17) & mask) as usize
    })
}

struct Shard {
    rows_log2: u32,
    hashes: u32,
    words: usize,
    documents: Vec<u32>,
    bits: Vec<u64>,
}

impl Shard {
    fn new(rows_log2: u32, hashes: u32, capacity: usize) -> Self {
        let words = capacity.div_ceil(64).max(1);
        Self {
            rows_log2,
            hashes,
            words,
            documents: Vec::with_capacity(capacity),
            bits: vec![0; (1 << rows_log2) * words],
        }
    }

    fn full(&self) -> bool {
        self.documents.len() == self.words * 64
    }

    fn add(&mut self, document: u32, trigrams: &[u32]) {
        let column = self.documents.len();
        self.documents.push(document);
        let (word, bit) = (column / 64, 1_u64 << (column % 64));
        for trigram in trigrams {
            for row in rows_of(*trigram, self.rows_log2, self.hashes) {
                self.bits[row * self.words + word] |= bit;
            }
        }
    }

    fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let used = self.documents.len().div_ceil(64);
        out.write_all(&self.rows_log2.to_le_bytes())?;
        out.write_all(&self.hashes.to_le_bytes())?;
        out.write_all(&(used as u32).to_le_bytes())?;
        out.write_all(&(self.documents.len() as u32).to_le_bytes())?;
        for document in &self.documents {
            out.write_all(&document.to_le_bytes())?;
        }
        for row in self.bits.chunks_exact(self.words) {
            for word in &row[..used] {
                out.write_all(&word.to_le_bytes())?;
            }
        }
        Ok(())
    }
}

pub struct Builder<W: Write> {
    shape: Shape,
    shard_bytes: usize,
    out: W,
    shards: u32,
    open: Vec<Shard>,
    paths: Vec<String>,
    modified: Vec<u64>,
    pub skipped: usize,
    pub read_bytes: u64,
}

impl<W: Write> Builder<W> {
    pub fn new(shape: Shape, mut out: W) -> io::Result<Self> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        Ok(Self {
            shape,
            shard_bytes: SHARD_BYTES,
            out,
            shards: 0,
            open: Vec::new(),
            paths: Vec::new(),
            modified: Vec::new(),
            skipped: 0,
            read_bytes: 0,
        })
    }

    pub fn with_shard_bytes(mut self, shard_bytes: usize) -> Self {
        self.shard_bytes = shard_bytes;
        self
    }

    pub fn add_file(&mut self, path: &str) -> io::Result<()> {
        let Some((modified, text)) = read_text(Path::new(path)) else {
            self.skipped += 1;
            return Ok(());
        };
        self.read_bytes += text.len() as u64;
        self.add_text(path, modified, &text)
    }

    pub fn add_text(&mut self, path: &str, modified: u64, text: &[u8]) -> io::Result<()> {
        let found = trigrams(text);
        let rows_log2 = self.shape.rows_log2(found.len());
        let document = self.paths.len() as u32;
        self.paths.push(path.to_owned());
        self.modified.push(modified);
        let at = match self
            .open
            .iter()
            .position(|shard| shard.rows_log2 == rows_log2)
        {
            Some(at) => at,
            None => {
                let capacity = ((self.shard_bytes * 8) >> rows_log2).max(64);
                self.open
                    .push(Shard::new(rows_log2, self.shape.hashes, capacity));
                self.open.len() - 1
            }
        };
        self.open[at].add(document, &found);
        if self.open[at].full() {
            let shard = self.open.swap_remove(at);
            shard.write(&mut self.out)?;
            self.shards += 1;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        for shard in std::mem::take(&mut self.open) {
            if !shard.documents.is_empty() {
                shard.write(&mut self.out)?;
                self.shards += 1;
            }
        }
        self.out.write_all(&u32::MAX.to_le_bytes())?;
        self.out
            .write_all(&(self.paths.len() as u32).to_le_bytes())?;
        for modified in &self.modified {
            self.out.write_all(&modified.to_le_bytes())?;
        }
        let mut order: Vec<u32> = (0..self.paths.len() as u32).collect();
        order.sort_by(|a, b| self.paths[*a as usize].cmp(&self.paths[*b as usize]));
        for document in &order {
            self.out.write_all(&document.to_le_bytes())?;
        }
        let mut at = 0_u32;
        for path in &self.paths {
            self.out.write_all(&at.to_le_bytes())?;
            at += path.len() as u32;
        }
        self.out.write_all(&at.to_le_bytes())?;
        for path in &self.paths {
            self.out.write_all(path.as_bytes())?;
        }
        self.out.flush()?;
        Ok(self.out)
    }
}

fn nanos(modified: std::time::SystemTime) -> u64 {
    modified.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
    })
}

const DATALESS: u32 = 0x4000_0000;

fn evicted(found: &std::fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::darwin::fs::MetadataExt;
        found.st_flags() & DATALESS != 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (found, DATALESS);
        false
    }
}

fn eligible(path: &str) -> Option<u64> {
    if !readable(path, 1) {
        return None;
    }
    let found = std::fs::symlink_metadata(path).ok()?;
    (found.is_file() && !evicted(&found) && readable(path, found.len()))
        .then(|| found.modified().ok().map(nanos))
        .flatten()
}

fn read_text(path: &Path) -> Option<(u64, Vec<u8>)> {
    use std::os::unix::fs::OpenOptionsExt;
    let before = std::fs::symlink_metadata(path).ok()?;
    if !before.is_file() || evicted(&before) {
        return None;
    }
    let mut file = File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let found = file.metadata().ok()?;
    if !found.is_file() || evicted(&found) {
        return None;
    }
    let modified = nanos(found.modified().ok()?);
    let mut text = Vec::new();
    Read::by_ref(&mut file)
        .take(SNIFFED as u64)
        .read_to_end(&mut text)
        .ok()?;
    if memchr::memchr(0, &text).is_some() {
        return None;
    }
    file.take(LARGEST).read_to_end(&mut text).ok()?;
    Some((modified, text))
}

struct ShardView {
    rows_log2: u32,
    hashes: u32,
    words: usize,
    documents_at: usize,
    count: usize,
    bits_at: usize,
}

pub struct Index<B: AsRef<[u8]>> {
    bytes: B,
    shards: Vec<ShardView>,
    count: usize,
    modified_at: usize,
    order_at: usize,
    offsets_at: usize,
    names_at: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    pub line: usize,
    pub text: String,
}

fn broken(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

fn u32_at(bytes: &[u8], at: usize) -> io::Result<u32> {
    let slice = bytes
        .get(at..at + 4)
        .ok_or_else(|| broken("the index ends early"))?;
    Ok(u32::from_le_bytes(
        slice.try_into().map_err(|_| broken("bad number"))?,
    ))
}

impl<B: AsRef<[u8]>> Index<B> {
    pub fn open(bytes: B) -> io::Result<Self> {
        let all = bytes.as_ref();
        if all.get(..8) != Some(MAGIC.as_slice()) || u32_at(all, 8)? != VERSION {
            return Err(broken("not a Raiden content index"));
        }
        let mut at = 12;
        let mut shards = Vec::new();
        loop {
            let rows_log2 = u32_at(all, at)?;
            if rows_log2 == u32::MAX {
                at += 4;
                break;
            }
            if !(FEWEST_ROWS_LOG2..=MOST_ROWS_LOG2).contains(&rows_log2) {
                return Err(broken("a shard has an impossible size"));
            }
            let hashes = u32_at(all, at + 4)?;
            let words = u32_at(all, at + 8)? as usize;
            let count = u32_at(all, at + 12)? as usize;
            let documents_at = at + 16;
            let bits_at = documents_at + count * 4;
            let end = bits_at + (1_usize << rows_log2) * words * 8;
            if end > all.len() || count > words * 64 {
                return Err(broken("a shard runs past the end"));
            }
            shards.push(ShardView {
                rows_log2,
                hashes,
                words,
                documents_at,
                count,
                bits_at,
            });
            at = end;
        }
        let count = u32_at(all, at)? as usize;
        let modified_at = at + 4;
        let order_at = modified_at + count * 8;
        let offsets_at = order_at + count * 4;
        let names_at = offsets_at + (count + 1) * 4;
        let names_length = u32_at(all, offsets_at + count * 4)? as usize;
        if names_at + names_length != all.len() {
            return Err(broken("the index's paths do not end where it does"));
        }
        let index = Self {
            bytes,
            shards,
            count,
            modified_at,
            order_at,
            offsets_at,
            names_at,
        };
        for document in 0..count {
            if index.path(document as u32).is_none() {
                return Err(broken("a path is out of place"));
            }
        }
        Ok(index)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn number(&self, at: usize) -> u32 {
        u32_at(self.bytes.as_ref(), at).unwrap_or(u32::MAX)
    }

    pub fn modified(&self, document: u32) -> u64 {
        let at = self.modified_at + document as usize * 8;
        self.bytes
            .as_ref()
            .get(at..at + 8)
            .and_then(|bytes| bytes.try_into().ok())
            .map_or(0, u64::from_le_bytes)
    }

    fn sorted(&self, rank: usize) -> u32 {
        self.number(self.order_at + rank * 4)
    }

    pub fn find(&self, path: &str) -> Option<u32> {
        let rank = self.first_at_or_after(path);
        (rank < self.count)
            .then(|| self.sorted(rank))
            .filter(|document| self.path(*document) == Some(path))
    }

    fn first_at_or_after(&self, path: &str) -> usize {
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let middle = (low + high) / 2;
            if self.path(self.sorted(middle)).unwrap_or_default() < path {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    pub fn under(&self, folder: &str) -> Vec<u32> {
        let prefix = format!("{}/", folder.trim_end_matches('/'));
        let mut found = Vec::new();
        for rank in self.first_at_or_after(&prefix)..self.count {
            let document = self.sorted(rank);
            if !self
                .path(document)
                .is_some_and(|path| path.starts_with(&prefix))
            {
                break;
            }
            found.push(document);
        }
        found
    }

    pub fn inside(&self, folder: &str) -> Vec<u32> {
        let prefix = format!("{}/", folder.trim_end_matches('/'));
        let mut found = Vec::new();
        for rank in self.first_at_or_after(&prefix)..self.count {
            let document = self.sorted(rank);
            let Some(rest) = self
                .path(document)
                .and_then(|path| path.strip_prefix(&prefix))
            else {
                break;
            };
            if !rest.contains('/') {
                found.push(document);
            }
        }
        found
    }

    fn word(&self, at: usize) -> u64 {
        let all = self.bytes.as_ref();
        u64::from_le_bytes(all[at..at + 8].try_into().unwrap_or_default())
    }

    pub fn candidates(&self, query: &[u8]) -> Vec<u32> {
        self.candidates_unless(query, &|| false)
    }

    fn candidates_unless(&self, query: &[u8], stop: &(dyn Fn() -> bool + Sync)) -> Vec<u32> {
        let wanted = trigrams(query);
        let all = self.bytes.as_ref();
        let mut found = Vec::new();
        for shard in &self.shards {
            if stop() {
                return Vec::new();
            }
            let mut rows: Vec<usize> = wanted
                .iter()
                .flat_map(|trigram| rows_of(*trigram, shard.rows_log2, shard.hashes))
                .collect();
            rows.sort_unstable();
            rows.dedup();
            for word in 0..shard.words {
                let mut hits = if word == shard.words - 1 && shard.count % 64 != 0 {
                    (1_u64 << (shard.count % 64)) - 1
                } else {
                    u64::MAX
                };
                for row in &rows {
                    if hits == 0 {
                        break;
                    }
                    hits &= self.word(shard.bits_at + (row * shard.words + word) * 8);
                }
                while hits != 0 {
                    let column = word * 64 + hits.trailing_zeros() as usize;
                    hits &= hits - 1;
                    let at = shard.documents_at + column * 4;
                    found.push(u32::from_le_bytes(
                        all[at..at + 4].try_into().unwrap_or_default(),
                    ));
                }
            }
        }
        found
    }

    pub fn path(&self, document: u32) -> Option<&str> {
        let document = document as usize;
        if document >= self.count {
            return None;
        }
        let start = self.number(self.offsets_at + document * 4) as usize;
        let end = self.number(self.offsets_at + document * 4 + 4) as usize;
        let bytes = self
            .bytes
            .as_ref()
            .get(self.names_at + start..self.names_at + end)?;
        std::str::from_utf8(bytes).ok()
    }

    pub fn search(&self, query: &str, most: usize) -> Vec<Found> {
        let needle = query.as_bytes().to_ascii_lowercase();
        if needle.len() < 3 || most == 0 {
            return Vec::new();
        }
        let candidates = self.candidates(&needle);
        let paths: Vec<&str> = candidates
            .iter()
            .filter_map(|document| self.path(*document))
            .collect();
        confirm(&paths, &needle, most, &|| false)
    }
}

fn confirm(
    paths: &[&str],
    needle: &[u8],
    most: usize,
    stop: &(dyn Fn() -> bool + Sync),
) -> Vec<Found> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let workers = std::thread::available_parallelism().map_or(4, |count| count.get().min(8));
    let next = AtomicUsize::new(0);
    let enough = AtomicBool::new(false);
    let limit = AtomicUsize::new(usize::MAX);
    let found = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let finder = memmem::Finder::new(needle);
                while !enough.load(Ordering::Relaxed) {
                    if stop() {
                        enough.store(true, Ordering::Relaxed);
                        return;
                    }
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    if at >= limit.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some(path) = paths.get(at) else {
                        return;
                    };
                    let Some((_, text)) = read_text(Path::new(path)) else {
                        continue;
                    };
                    if let Some(hit) = finder.find(&text.to_ascii_lowercase()) {
                        let mut found = found
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        found.push((at, line_at(path, &text, hit)));
                        if found.len() >= most {
                            let mut places: Vec<usize> = found.iter().map(|(at, _)| *at).collect();
                            places.sort_unstable();
                            limit.store(places[most - 1], Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });
    let mut found = found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().take(most).map(|(_, hit)| hit).collect()
}

fn line_at(path: &str, text: &[u8], at: usize) -> Found {
    let start = memchr::memrchr(b'\n', &text[..at]).map_or(0, |newline| newline + 1);
    let end = memchr::memchr(b'\n', &text[at..]).map_or(text.len(), |newline| at + newline);
    let line = memchr::memchr_iter(b'\n', &text[..at]).count() + 1;
    let whole = String::from_utf8_lossy(&text[start..end]);
    let hit = String::from_utf8_lossy(&text[start..at]).chars().count();
    let length = whole.chars().count();
    let from = if length <= SHOWN_CHARACTERS {
        0
    } else {
        hit.saturating_sub(SHOWN_CHARACTERS / 3)
            .min(length - SHOWN_CHARACTERS)
    };
    let shown = whole
        .chars()
        .skip(from)
        .take(SHOWN_CHARACTERS)
        .collect::<String>()
        .trim()
        .to_owned();
    Found {
        path: path.to_owned(),
        line,
        text: shown,
    }
}

pub struct Mapped {
    at: *mut libc::c_void,
    length: usize,
}

unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

impl Mapped {
    pub fn open(path: &Path) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let file = File::open(path)?;
        let length = usize::try_from(file.metadata()?.len()).map_err(|_| broken("too large"))?;
        if length == 0 {
            return Err(broken("the index is empty"));
        }
        let at = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        if at == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { at, length })
    }
}

impl AsRef<[u8]> for Mapped {
    fn as_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.at.cast::<u8>(), self.length) }
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.at, self.length) };
    }
}

pub fn build_into(
    shape: Shape,
    files: &[(String, u64)],
    to: &Path,
) -> io::Result<(usize, usize, u64)> {
    let out = BufWriter::new(File::create(to)?);
    let mut builder = Builder::new(shape, out)?;
    for (path, _) in files {
        builder.add_file(path)?;
    }
    let (indexed, skipped, read) = (builder.paths.len(), builder.skipped, builder.read_bytes);
    builder.finish()?;
    Ok((indexed, skipped, read))
}

pub struct Contents<B: AsRef<[u8]>> {
    base: Option<Index<B>>,
    gone: HashSet<u32>,
    fresh: HashMap<String, (u64, Vec<u32>)>,
    skipped: HashMap<String, u64>,
}

#[derive(Debug, Default)]
pub struct Update {
    gone: Vec<u32>,
    vanished: Vec<String>,
    fresh: Vec<(String, u64, Option<Vec<u32>>)>,
}

impl<B: AsRef<[u8]>> Contents<B> {
    pub fn new(base: Option<Index<B>>) -> Self {
        Self {
            base,
            gone: HashSet::new(),
            fresh: HashMap::new(),
            skipped: HashMap::new(),
        }
    }

    pub fn changed_since_built(&self) -> usize {
        self.gone.len() + self.fresh.len()
    }

    pub fn reconcile(&mut self, folder: &Path, below: bool, listed: &[String]) -> bool {
        let update = self.prepare(folder, below, listed);
        self.apply(update)
    }

    pub fn prepare(&self, folder: &Path, below: bool, listed: &[String]) -> Update {
        let mut update = Update::default();
        let Some(folder_text) = folder.to_str() else {
            return update;
        };
        let prefix = format!("{}/", folder_text.trim_end_matches('/'));
        let in_scope = |path: &str| {
            path.strip_prefix(&prefix)
                .is_some_and(|rest| below || !rest.contains('/'))
        };
        let on_disk: HashMap<&str, u64> = listed
            .iter()
            .filter(|path| in_scope(path))
            .filter_map(|path| eligible(path).map(|modified| (path.as_str(), modified)))
            .collect();
        if let Some(base) = &self.base {
            let known = if below {
                base.under(folder_text)
            } else {
                base.inside(folder_text)
            };
            update.gone.extend(known.into_iter().filter(|document| {
                !self.gone.contains(document)
                    && !base
                        .path(*document)
                        .is_some_and(|path| on_disk.contains_key(path))
            }));
        }
        update.vanished = self
            .fresh
            .keys()
            .chain(self.skipped.keys())
            .filter(|path| in_scope(path) && !on_disk.contains_key(path.as_str()))
            .cloned()
            .collect();
        for (path, modified) in on_disk {
            let known = self.base.as_ref().and_then(|base| base.find(path));
            let current_in_base = known.is_some_and(|document| {
                !self.gone.contains(&document)
                    && self
                        .base
                        .as_ref()
                        .is_some_and(|base| base.modified(document) == modified)
            });
            let current = current_in_base
                || self.fresh.get(path).is_some_and(|(at, _)| *at == modified)
                || self.skipped.get(path) == Some(&modified);
            if current {
                continue;
            }
            if let Some(document) = known
                && !self.gone.contains(&document)
            {
                update.gone.push(document);
            }
            let held = read_text(Path::new(path)).map(|(_, text)| trigrams(&text));
            update.fresh.push((path.to_owned(), modified, held));
        }
        update
    }

    pub fn apply(&mut self, update: Update) -> bool {
        let changed =
            !update.gone.is_empty() || !update.vanished.is_empty() || !update.fresh.is_empty();
        self.gone.extend(update.gone);
        for path in update.vanished {
            self.fresh.remove(&path);
            self.skipped.remove(&path);
        }
        for (path, modified, held) in update.fresh {
            match held {
                Some(held) => {
                    self.skipped.remove(&path);
                    self.fresh.insert(path, (modified, held));
                }
                None => {
                    self.fresh.remove(&path);
                    self.skipped.insert(path, modified);
                }
            }
        }
        changed
    }

    pub fn search(&self, query: &str, most: usize, stop: &(dyn Fn() -> bool + Sync)) -> Vec<Found> {
        let needle = query.as_bytes().to_ascii_lowercase();
        if needle.len() < 3 || most == 0 || stop() {
            return Vec::new();
        }
        let wanted = trigrams(&needle);
        let mut paths: Vec<&str> = self
            .fresh
            .iter()
            .filter(|(_, (_, held))| {
                wanted
                    .iter()
                    .all(|trigram| held.binary_search(trigram).is_ok())
            })
            .map(|(path, _)| path.as_str())
            .collect();
        paths.sort_unstable();
        if let Some(base) = &self.base {
            paths.extend(
                base.candidates_unless(&needle, stop)
                    .into_iter()
                    .filter(|document| !self.gone.contains(document))
                    .filter_map(|document| base.path(document)),
            );
        }
        confirm(&paths, &needle, most, stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(shape: Shape, texts: &[(&str, &str)]) -> Index<Vec<u8>> {
        index_in_shards(shape, texts, SHARD_BYTES)
    }

    fn index_in_shards(shape: Shape, texts: &[(&str, &str)], shard_bytes: usize) -> Index<Vec<u8>> {
        let mut builder = Builder::new(shape, Vec::new())
            .unwrap()
            .with_shard_bytes(shard_bytes);
        for (path, text) in texts {
            builder.add_text(path, 1, text.as_bytes()).unwrap();
        }
        Index::open(builder.finish().unwrap()).unwrap()
    }

    fn candidate_paths<B: AsRef<[u8]>>(index: &Index<B>, query: &str) -> Vec<String> {
        let mut paths: Vec<String> = index
            .candidates(query.to_ascii_lowercase().as_bytes())
            .into_iter()
            .filter_map(|document| index.path(document).map(str::to_owned))
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn a_file_holding_the_query_is_always_a_candidate() {
        let texts: Vec<(String, String)> = (0..500)
            .map(|at| {
                (
                    format!("/f/{at}.txt"),
                    format!("line {at}\nneedle-{at} sits here\n"),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str)> = texts
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        for shape in [
            Shape {
                bits_per_trigram: 1,
                hashes: 1,
            },
            Shape::default(),
            Shape {
                bits_per_trigram: 16,
                hashes: 3,
            },
        ] {
            let index = index_of(shape, &borrowed);
            for at in [0, 63, 64, 199, 499] {
                let paths = candidate_paths(&index, &format!("needle-{at} SITS"));
                assert!(paths.contains(&format!("/f/{at}.txt")), "{shape:?} {at}");
            }
        }
    }

    #[test]
    fn unrelated_files_are_mostly_filtered_out() {
        let texts: Vec<(String, String)> = (0..2000)
            .map(|at| {
                (
                    format!("/f/{at}.rs"),
                    format!("fn helper_{at}() {{ let value = {at}; }}\n"),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str)> = texts
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        let index = index_of(Shape::default(), &borrowed);
        let paths = candidate_paths(&index, "quarterly revenue forecast");
        assert!(paths.len() < 20, "{} false candidates", paths.len());
    }

    #[test]
    fn search_confirms_matches_and_reports_the_line() {
        let folder = std::env::temp_dir().join(format!("raiden-contents-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let notes = folder.join("notes.md");
        let other = folder.join("other.md");
        std::fs::write(
            &notes,
            "# Notes\nfirst line\nThe Quarterly Plan is due\nlast\n",
        )
        .unwrap();
        std::fs::write(&other, "nothing about plans here\n").unwrap();
        let files = vec![
            (notes.to_str().unwrap().to_owned(), 60),
            (other.to_str().unwrap().to_owned(), 30),
        ];
        let saved = folder.join("contents.idx");
        build_into(Shape::default(), &files, &saved).unwrap();
        let index = Index::open(std::fs::read(&saved).unwrap()).unwrap();
        let found = index.search("quarterly plan", 10);
        let none = index.search("missing words", 10);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(
            found,
            [Found {
                path: notes.to_str().unwrap().to_owned(),
                line: 3,
                text: "The Quarterly Plan is due".to_owned(),
            }]
        );
        assert!(none.is_empty());
    }

    #[test]
    fn binary_files_and_short_queries_are_skipped() {
        let folder =
            std::env::temp_dir().join(format!("raiden-contents-bin-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let blob = folder.join("blob.dat");
        std::fs::write(&blob, b"needle\0\0\0binary").unwrap();
        let saved = folder.join("contents.idx");
        let (indexed, skipped, _) = build_into(
            Shape::default(),
            &[(blob.to_str().unwrap().to_owned(), 16)],
            &saved,
        )
        .unwrap();
        let index = Index::open(std::fs::read(&saved).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!((indexed, skipped), (0, 1));
        assert!(index.search("needle", 10).is_empty());
        assert!(index.search("ne", 10).is_empty());
    }

    #[test]
    fn many_files_fill_several_shards_and_all_stay_findable() {
        let texts: Vec<(String, String)> = (0..70_000)
            .map(|at| (format!("/f/{at}"), format!("unique-token-{at:06}")))
            .collect();
        let borrowed: Vec<(&str, &str)> = texts
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        let index = index_in_shards(Shape::default(), &borrowed, 2048);
        assert!(index.shards.len() >= 10, "{} shards", index.shards.len());
        for at in [0, 32_767, 32_768, 65_535, 69_999] {
            let paths = candidate_paths(&index, &format!("unique-token-{at:06}"));
            assert!(paths.contains(&format!("/f/{at}")), "{at}");
        }
    }

    #[test]
    fn a_damaged_index_is_refused() {
        let index = index_of(Shape::default(), &[("/a", "hello world")]);
        let bytes = index.bytes.clone();
        assert!(Index::open(bytes[..bytes.len() - 3].to_vec()).is_err());
        assert!(Index::open(b"RAIDENNM".to_vec()).is_err());
        let mut wrong = bytes.clone();
        wrong[12] = 40;
        assert!(Index::open(wrong).is_err());
    }

    #[derive(Clone, Copy)]
    struct Usage {
        cpu_ns: u64,
        energy_nj: u64,
        read: u64,
        written: u64,
        peak: u64,
        wakeups: u64,
    }

    fn cpu_ns() -> u64 {
        let mut used: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut used) };
        let micros = |time: libc::timeval| time.tv_sec as u64 * 1_000_000 + time.tv_usec as u64;
        (micros(used.ru_utime) + micros(used.ru_stime)) * 1000
    }

    fn usage() -> Usage {
        let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
        unsafe {
            libc::proc_pid_rusage(libc::getpid(), libc::RUSAGE_INFO_V4, (&raw mut info).cast())
        };
        Usage {
            cpu_ns: cpu_ns(),
            energy_nj: info.ri_billed_energy,
            read: info.ri_diskio_bytesread,
            written: info.ri_diskio_byteswritten,
            peak: info.ri_lifetime_max_phys_footprint,
            wakeups: info.ri_pkg_idle_wkups + info.ri_interrupt_wkups,
        }
    }

    fn mb(bytes: u64) -> f64 {
        bytes as f64 / 1e6
    }

    const QUERIES: [&str; 12] = [
        "use",
        "TODO",
        "fn main",
        "useEffect",
        "import React",
        "SELECT",
        "budget",
        "presupuesto",
        "Raiden",
        "quarterly report",
        "N5276V",
        "zqxjkvbwp",
    ];

    #[test]
    #[ignore = "reads the whole home folder; run with --ignored --nocapture to measure"]
    fn measures_content_search_on_the_real_home_folder() {
        use std::os::darwin::fs::MetadataExt as _;
        let home = std::env::var("HOME").unwrap();
        let sample: usize = std::env::var("RAIDEN_BENCH_SAMPLE")
            .ok()
            .and_then(|every| every.parse().ok())
            .unwrap_or(1);
        let shapes: Vec<Shape> = std::env::var("RAIDEN_BENCH_SHAPES")
            .unwrap_or_else(|_| "2x2".to_owned())
            .split(',')
            .filter_map(|shape| {
                let (bits, hashes) = shape.split_once('x')?;
                Some(Shape {
                    bits_per_trigram: bits.parse().ok()?,
                    hashes: hashes.parse().ok()?,
                })
            })
            .collect();
        if std::env::var("RAIDEN_BENCH_QOS").as_deref() == Ok("background") {
            unsafe {
                libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
            }
        }
        let kept = std::env::var_os("RAIDEN_BENCH_DIR").map(std::path::PathBuf::from);
        let folder = kept.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("raiden-bench-{}", std::process::id()))
        });
        std::fs::create_dir_all(&folder).unwrap();
        let names = crate::names::Names::build(&home);
        let started = std::time::Instant::now();
        let mut dataless = 0;
        let files: Vec<(String, u64)> = names
            .files()
            .into_iter()
            .step_by(sample)
            .filter_map(|path| {
                let found = std::fs::symlink_metadata(&path).ok()?;
                if found.st_flags() & 0x4000_0000 != 0 {
                    dataless += 1;
                    return None;
                }
                (found.is_file() && readable(&path, found.len())).then_some((path, found.len()))
            })
            .collect();
        let total: u64 = files.iter().map(|(_, size)| size).sum();
        eprintln!(
            "footprint before building: {:.0} MB (name index and file list)",
            mb(usage().peak)
        );
        eprintln!(
            "candidates to read: {} files, {:.0} MB (sampled 1 in {sample}); {dataless} evicted iCloud files skipped; listed in {:?}",
            files.len(),
            mb(total),
            started.elapsed()
        );
        for shape in shapes {
            let saved = folder.join(format!(
                "contents-{}x{}.idx",
                shape.bits_per_trigram, shape.hashes
            ));
            let before = usage();
            let started = std::time::Instant::now();
            let (indexed, skipped, read) = build_into(shape, &files, &saved).unwrap();
            let wall = started.elapsed();
            let after = usage();
            let size = std::fs::metadata(&saved).unwrap().len();
            eprintln!(
                "\nshape {}x{}: indexed {indexed} text files ({:.0} MB of text), skipped {skipped} binary",
                shape.bits_per_trigram,
                shape.hashes,
                mb(read)
            );
            eprintln!(
                "  build: {:.1} s wall, {:.1} s CPU, {:.1} J, {} wakeups, read {:.0} MB, wrote {:.0} MB, peak footprint {:.0} MB, index {:.0} MB ({:.1}% of the text)",
                wall.as_secs_f64(),
                (after.cpu_ns - before.cpu_ns) as f64 / 1e9,
                (after.energy_nj - before.energy_nj) as f64 / 1e9,
                after.wakeups - before.wakeups,
                mb(after.read - before.read),
                mb(after.written - before.written),
                mb(after.peak),
                mb(size),
                size as f64 * 100.0 / read.max(1) as f64
            );
            let mapped = Mapped::open(&saved).unwrap();
            let index = Index::open(mapped).unwrap();
            for query in QUERIES {
                let needle = query.to_ascii_lowercase();
                let before = usage();
                let started = std::time::Instant::now();
                let candidates = index.candidates(needle.as_bytes());
                let filtered = started.elapsed();
                let started = std::time::Instant::now();
                let shown = index.search(query, 20);
                let first_page = started.elapsed();
                let started = std::time::Instant::now();
                let finder = memmem::Finder::new(needle.as_bytes());
                let real = candidates
                    .iter()
                    .filter_map(|document| index.path(*document))
                    .filter(|path| {
                        std::fs::read(path)
                            .is_ok_and(|text| finder.find(&text.to_ascii_lowercase()).is_some())
                    })
                    .count();
                let all = started.elapsed();
                let after = usage();
                eprintln!(
                    "  {query:>16}: {:>7} candidates in {:>9.2?}, {real:>6} real ({:>5.1}% false), first {} shown in {:>9.2?}, all checked in {:>9.2?}, {:.2} J",
                    candidates.len(),
                    filtered,
                    if candidates.is_empty() {
                        0.0
                    } else {
                        (candidates.len() - real) as f64 * 100.0 / candidates.len() as f64
                    },
                    shown.len(),
                    first_page,
                    all,
                    (after.energy_nj - before.energy_nj) as f64 / 1e9
                );
            }
            let footprint = usage().peak;
            eprintln!("  peak footprint after queries {:.0} MB", mb(footprint));
        }
        if kept.is_none() {
            let _ = std::fs::remove_dir_all(&folder);
        }
    }

    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("raiden-overlay-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn write(&self, name: &str, text: &str) -> String {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path.to_str().unwrap().to_owned()
        }

        fn contents(&self, files: &[String]) -> Contents<Vec<u8>> {
            let listed: Vec<(String, u64)> = files.iter().map(|path| (path.clone(), 1)).collect();
            let saved = self.0.join("contents.idx");
            build_into(Shape::default(), &listed, &saved).unwrap();
            Contents::new(Some(Index::open(std::fs::read(&saved).unwrap()).unwrap()))
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn listed_below(folder: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut waiting = vec![folder.to_path_buf()];
        while let Some(at) = waiting.pop() {
            for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    waiting.push(path);
                } else {
                    found.push(path.to_str().unwrap().to_owned());
                }
            }
        }
        found
    }

    fn changed<B: AsRef<[u8]>>(contents: &mut Contents<B>, folder: &Path) -> bool {
        contents.reconcile(folder, false, &listed_below(folder))
    }

    fn found_paths<B: AsRef<[u8]>>(contents: &Contents<B>, query: &str) -> Vec<String> {
        let mut paths: Vec<String> = contents
            .search(query, 100, &|| false)
            .into_iter()
            .map(|found| found.path.rsplit('/').next().unwrap().to_owned())
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn edits_additions_and_deletions_show_up_after_their_folder_changes() {
        let folder = Folder::new("edits");
        let kept = folder.write("notes/kept.md", "the budget for march");
        let edited = folder.write("notes/edited.md", "old wording about the budget");
        let deleted = folder.write("notes/deleted.md", "budget that will vanish");
        let mut contents = folder.contents(&[kept, edited.clone(), deleted.clone()]);
        assert_eq!(
            found_paths(&contents, "budget"),
            ["deleted.md", "edited.md", "kept.md"]
        );
        std::fs::write(&edited, "new wording about the forecast").unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        File::options()
            .write(true)
            .open(&edited)
            .unwrap()
            .set_modified(later)
            .unwrap();
        std::fs::remove_file(&deleted).unwrap();
        folder.write("notes/added.md", "a brand new budget");
        assert!(changed(&mut contents, &folder.0.join("notes")));
        assert_eq!(found_paths(&contents, "budget"), ["added.md", "kept.md"]);
        assert_eq!(found_paths(&contents, "forecast"), ["edited.md"]);
        assert_eq!(contents.changed_since_built(), 4);
    }

    #[test]
    fn a_folder_with_nothing_new_reads_no_file() {
        let folder = Folder::new("unchanged");
        let one = folder.write("docs/one.txt", "alpha beta gamma");
        let two = folder.write("docs/two.txt", "delta epsilon");
        let mut contents = folder.contents(&[one, two]);
        assert!(!changed(&mut contents, &folder.0.join("docs")));
        assert_eq!(contents.changed_since_built(), 0);
    }

    #[test]
    fn an_edit_within_the_same_second_as_the_build_is_still_seen() {
        let folder = Folder::new("same-second");
        let file = folder.write("a/quick.txt", "first draft");
        let mut contents = folder.contents(std::slice::from_ref(&file));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "second draft").unwrap();
        changed(&mut contents, &folder.0.join("a"));
        assert_eq!(found_paths(&contents, "second draft"), ["quick.txt"]);
        assert!(found_paths(&contents, "first draft").is_empty());
    }

    #[test]
    fn paths_are_found_by_name_and_by_folder_without_loading_them() {
        let folder = Folder::new("lookup");
        let paths: Vec<String> = ["b/2.txt", "a/1.txt", "a/sub/3.txt", "a/4.txt", "ab/5.txt"]
            .iter()
            .map(|name| folder.write(name, "some words here"))
            .collect();
        let contents = folder.contents(&paths);
        let base = contents.base.as_ref().unwrap();
        for path in &paths {
            let document = base.find(path).unwrap();
            assert_eq!(base.path(document), Some(path.as_str()));
        }
        assert_eq!(
            base.find(&format!("{}/a/missing.txt", folder.0.display())),
            None
        );
        let mut inside: Vec<&str> = base
            .inside(folder.0.join("a").to_str().unwrap())
            .into_iter()
            .filter_map(|document| base.path(document))
            .map(|path| path.rsplit('/').next().unwrap())
            .collect();
        inside.sort_unstable();
        assert_eq!(inside, ["1.txt", "4.txt"]);
    }

    #[test]
    #[ignore = "writes 2000 files to a temporary folder; run with --ignored --nocapture"]
    fn measures_keeping_up_with_edits() {
        let folder = Folder::new("churn");
        let sample =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/src/contents.rs")).unwrap();
        let files: Vec<String> = (0..2000)
            .map(|at| {
                let mut text = sample.clone();
                text.extend_from_slice(format!("\nmarker-{at}\n").as_bytes());
                let path = folder.0.join(format!("project/src/file{at}.rs"));
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, &text).unwrap();
                path.to_str().unwrap().to_owned()
            })
            .collect();
        let mut contents = folder.contents(&files);
        let src = folder.0.join("project/src");
        for edited in [10_usize, 100, 1000] {
            let later =
                std::time::SystemTime::now() + std::time::Duration::from_secs(edited as u64);
            for path in files.iter().take(edited) {
                std::fs::write(path, format!("rewritten {edited} quarterly-{edited}")).unwrap();
                File::options()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .set_modified(later)
                    .unwrap();
            }
            let before = usage();
            let started = std::time::Instant::now();
            changed(&mut contents, &src);
            let took = started.elapsed();
            let after = usage();
            let found = contents
                .search(&format!("quarterly-{edited}"), 2000, &|| false)
                .len();
            let started = std::time::Instant::now();
            let unchanged = changed(&mut contents, &src);
            eprintln!(
                "{edited:>5} edited of 2000 in one folder: caught up in {took:?} ({:.0} µs CPU per edited file), {found} found; looking again with nothing new took {:?} (changed: {unchanged})",
                (after.cpu_ns - before.cpu_ns) as f64 / 1e3 / edited as f64,
                started.elapsed()
            );
        }
    }

    #[test]
    fn a_folder_moved_in_is_read_whole_when_its_subtree_is_reconciled() {
        let folder = Folder::new("moved");
        let kept = folder.write("Code/kept.rs", "fn kept() {}");
        let mut contents = folder.contents(&[kept]);
        folder.write(
            "Code/project/src/deep/plan.md",
            "the relocated quarterly plan",
        );
        let project = folder.0.join("Code/project");
        assert!(contents.reconcile(&project, true, &listed_below(&project)));
        assert_eq!(found_paths(&contents, "relocated quarterly"), ["plan.md"]);
    }

    #[test]
    fn files_the_name_index_leaves_out_are_never_read() {
        let folder = Folder::new("left-out");
        let kept = folder.write("app/kept.js", "useEffect in my code");
        let mut contents = folder.contents(std::slice::from_ref(&kept));
        folder.write(
            "app/node_modules/react/index.js",
            "useEffect in a dependency",
        );
        let app = folder.0.join("app");
        contents.reconcile(&app, true, std::slice::from_ref(&kept));
        assert_eq!(found_paths(&contents, "useEffect"), ["kept.js"]);
    }

    #[test]
    fn an_unchanged_binary_is_not_sniffed_again() {
        let folder = Folder::new("binary-again");
        let blob = folder.write("data/blob.dat", "x");
        std::fs::write(&blob, b"needle\0\0binary").unwrap();
        let mut contents = folder.contents(&[]);
        let data = folder.0.join("data");
        assert!(changed(&mut contents, &data));
        assert!(!changed(&mut contents, &data));
        assert!(found_paths(&contents, "needle").is_empty());
    }

    #[test]
    fn a_file_grown_past_the_limit_drops_out() {
        let folder = Folder::new("grown");
        let log = folder.write("notes/journal.txt", "a quiet journal entry");
        let mut contents = folder.contents(std::slice::from_ref(&log));
        let mut huge = b"a quiet journal entry\n".to_vec();
        huge.resize(3 * 1024 * 1024, b'x');
        std::fs::write(&log, huge).unwrap();
        changed(&mut contents, &folder.0.join("notes"));
        assert!(found_paths(&contents, "quiet journal").is_empty());
    }

    #[test]
    fn a_match_deep_in_a_long_line_is_shown_around_the_match() {
        let mut text = "word ".repeat(400).into_bytes();
        let at = text.len();
        text.extend_from_slice(b"the needle is here and the line goes on ");
        text.extend_from_slice("tail ".repeat(100).as_bytes());
        let found = line_at("/f", &text, at + 4);
        assert!(found.text.contains("needle"), "{}", found.text);
        assert!(found.text.chars().count() <= SHOWN_CHARACTERS);
    }

    #[test]
    fn skipped_binaries_never_count_toward_a_rebuild() {
        let folder = Folder::new("binaries-count");
        for at in 0..20 {
            let blob = folder.write(&format!("data/blob{at}.dat"), "x");
            std::fs::write(&blob, b"\0\0\0").unwrap();
        }
        let mut contents = folder.contents(&[]);
        changed(&mut contents, &folder.0.join("data"));
        assert_eq!(contents.changed_since_built(), 0);
    }

    #[test]
    fn a_file_replaced_by_a_fifo_or_a_link_is_not_read() {
        let folder = Folder::new("fifo");
        let notes = folder.write("a/notes.md", "secret plans");
        let elsewhere = folder.write("b/target.md", "secret plans");
        let contents = folder.contents(std::slice::from_ref(&notes));
        std::fs::remove_file(&notes).unwrap();
        let made = std::process::Command::new("/usr/bin/mkfifo")
            .arg(&notes)
            .status()
            .unwrap();
        assert!(made.success());
        let started = std::time::Instant::now();
        assert!(found_paths(&contents, "secret plans").is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        std::fs::remove_file(&notes).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &notes).unwrap();
        assert!(found_paths(&contents, "secret plans").is_empty());
    }

    #[test]
    fn a_removed_folder_takes_its_files_out_of_text_search() {
        let folder = Folder::new("removed-folder");
        let deep = folder.write("Code/old/src/deep.rs", "removed words here");
        let kept = folder.write("Code/kept.rs", "removed words kept");
        let mut contents = folder.contents(&[deep, kept]);
        contents.reconcile(&folder.0.join("Code/old"), true, &[]);
        assert_eq!(found_paths(&contents, "removed words"), ["kept.rs"]);
    }

    #[test]
    fn the_same_search_shows_the_same_files_every_time() {
        let folder = Folder::new("stable");
        let files: Vec<String> = (0..120)
            .map(|at| folder.write(&format!("notes/n{at:03}.md"), "a common phrase here"))
            .collect();
        let contents = folder.contents(&files);
        let first: Vec<String> = contents
            .search("common phrase", 10, &|| false)
            .into_iter()
            .map(|found| found.path)
            .collect();
        for _ in 0..5 {
            let again: Vec<String> = contents
                .search("common phrase", 10, &|| false)
                .into_iter()
                .map(|found| found.path)
                .collect();
            assert_eq!(again, first);
        }
        assert_eq!(first.len(), 10);
    }

    #[test]
    fn a_stopped_search_reads_nothing() {
        let folder = Folder::new("stopped");
        let file = folder.write("a/notes.md", "stop words here");
        let contents = folder.contents(&[file]);
        assert!(contents.search("stop words", 10, &|| true).is_empty());
        assert_eq!(found_paths(&contents, "stop words"), ["notes.md"]);
    }
}
