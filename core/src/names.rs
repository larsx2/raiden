use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirEntryExt, MetadataExt};
use std::path::{Component, Path};
use std::time::SystemTime;

use memchr::memmem;

use crate::files;

const NONE: u32 = u32::MAX;
const MAGIC: &[u8; 8] = b"RAIDENNM";
const VERSION: u32 = 4;
const SEPARATOR: u8 = b'/';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Root,
    Folder,
    File,
    Other,
}

impl Kind {
    fn code(self) -> u8 {
        match self {
            Self::Root => 0,
            Self::Folder => 1,
            Self::File => 2,
            Self::Other => 3,
        }
    }

    fn from_code(code: u8) -> io::Result<Self> {
        match code {
            0 => Ok(Self::Root),
            1 => Ok(Self::Folder),
            2 => Ok(Self::File),
            _ => Ok(Self::Other),
        }
    }
}

const KIND_BITS: u8 = 0b11;
const ALIVE: u8 = 0b100;
const BLOCKED: u8 = 0b1000;
const FOREIGN: u8 = 0b1_0000;
const KEPT_FLAGS: u8 = KIND_BITS | BLOCKED | FOREIGN;

#[derive(Debug, Default)]
pub struct Names {
    home: String,
    parent: Vec<u32>,
    first_child: Vec<u32>,
    next_sibling: Vec<u32>,
    flags: Vec<u8>,
    name_at: Vec<u32>,
    folded_at: Vec<u32>,
    inode: Vec<u64>,
    device: u64,
    names: Vec<u8>,
    folded: Vec<u8>,
    dead: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stamp {
    pub home: u64,
    pub drive: u64,
    pub at: u64,
    pub history: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub is_file: bool,
    pub closeness: u8,
}

#[derive(Debug, Default)]
pub struct Changes(Vec<Compared>);

#[derive(Debug)]
struct Compared {
    folder: u32,
    on_disk: Option<Vec<Tree>>,
    anew: bool,
    foreign: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Listed {
    name: String,
    kind: Kind,
    inode: u64,
    foreign: bool,
}

#[derive(Debug)]
struct Tree {
    listed: Listed,
    inside: Option<Vec<Tree>>,
}

pub use crate::files::fold;

fn broken(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

fn offset(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(NONE)
}

type Scored = (u8, usize, u32);

fn best_first(a: &Scored, b: &Scored) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
}

impl Names {
    pub fn new(home: &str) -> Self {
        let home = home.trim_end_matches('/').to_owned();
        let mut names = Self {
            device: device_of(&home),
            home: home.clone(),
            ..Self::default()
        };
        names.push(NONE, Kind::Root, &home, 0);
        names.push(0, Kind::Root, &Self::drive_name(&home), 0);
        names
    }

    pub fn build(home: &str) -> Self {
        let mut names = Self::new(home);
        names.scan(0);
        names.scan(1);
        names.shrink();
        names
    }

    fn drive_name(home: &str) -> String {
        files::icloud_drive(home)[home.len() + 1..].to_owned()
    }

    pub fn len(&self) -> usize {
        self.parent.len() - self.dead
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= 2
    }

    pub fn home(&self) -> &str {
        &self.home
    }

    pub fn bytes(&self) -> usize {
        let words = self.parent.capacity()
            + self.first_child.capacity()
            + self.next_sibling.capacity()
            + self.name_at.capacity()
            + self.folded_at.capacity();
        words * size_of::<u32>()
            + self.inode.capacity() * size_of::<u64>()
            + self.flags.capacity()
            + self.names.capacity()
            + self.folded.capacity()
    }

    fn shrink(&mut self) {
        fn fit<T>(list: &mut Vec<T>) {
            list.shrink_to_fit();
            list.reserve_exact(list.len() / 32);
        }
        fit(&mut self.parent);
        fit(&mut self.first_child);
        fit(&mut self.next_sibling);
        fit(&mut self.flags);
        fit(&mut self.name_at);
        fit(&mut self.folded_at);
        fit(&mut self.inode);
        fit(&mut self.names);
        fit(&mut self.folded);
    }

    fn push(&mut self, parent: u32, kind: Kind, name: &str, inode: u64) -> u32 {
        let id = offset(self.parent.len());
        self.parent.push(parent);
        self.inode.push(inode);
        self.first_child.push(NONE);
        self.flags.push(kind.code() | ALIVE);
        self.name_at.push(offset(self.names.len()));
        self.folded_at.push(offset(self.folded.len()));
        self.names.extend_from_slice(name.as_bytes());
        if kind != Kind::Root {
            self.folded.extend_from_slice(fold(name).as_bytes());
        }
        self.folded.push(SEPARATOR);
        if parent == NONE {
            self.next_sibling.push(NONE);
        } else {
            self.next_sibling.push(self.first_child[parent as usize]);
            self.first_child[parent as usize] = id;
        }
        id
    }

    fn kind(&self, id: u32) -> Kind {
        match self.flags[id as usize] & KIND_BITS {
            0 => Kind::Root,
            1 => Kind::Folder,
            2 => Kind::File,
            _ => Kind::Other,
        }
    }

    fn alive(&self, id: u32) -> bool {
        self.flags[id as usize] & ALIVE != 0
    }

    fn span(starts: &[u32], end: usize, id: u32) -> std::ops::Range<usize> {
        let at = id as usize;
        let to = starts.get(at + 1).map_or(end, |next| *next as usize);
        starts[at] as usize..to
    }

    fn name(&self, id: u32) -> &str {
        let span = Self::span(&self.name_at, self.names.len(), id);
        std::str::from_utf8(&self.names[span]).unwrap_or_default()
    }

    fn folded_name(&self, id: u32) -> &[u8] {
        let span = Self::span(&self.folded_at, self.folded.len(), id);
        &self.folded[span.start..span.end - 1]
    }

    fn children(&self, folder: u32) -> impl Iterator<Item = u32> + '_ {
        std::iter::successors(
            Some(self.first_child[folder as usize]).filter(|first| *first != NONE),
            |at| Some(self.next_sibling[*at as usize]).filter(|next| *next != NONE),
        )
    }

    pub fn path(&self, id: u32) -> String {
        let mut parts = Vec::new();
        let mut at = id;
        while at != NONE {
            parts.push(self.name(at));
            at = self.parent[at as usize];
        }
        parts.reverse();
        parts.join("/")
    }

    fn depth(&self, id: u32) -> usize {
        std::iter::successors(Some(id), |at| {
            Some(self.parent[*at as usize]).filter(|parent| *parent != NONE)
        })
        .count()
    }

    fn opens(&self, id: u32) -> bool {
        self.alive(id)
            && self.flags[id as usize] & FOREIGN == 0
            && match self.kind(id) {
                Kind::Root => true,
                Kind::Folder => files::opened(self.name(id)),
                Kind::File | Kind::Other => false,
            }
    }

    fn scan(&mut self, folder: u32) {
        let path = self.path(folder);
        match read_tree(Path::new(&path), folder == 0, self.device) {
            Some(trees) => self.graft(folder, trees),
            None => self.flags[folder as usize] |= BLOCKED,
        }
    }

    fn graft(&mut self, folder: u32, trees: Vec<Tree>) {
        let mut waiting = vec![(folder, trees)];
        while let Some((folder, trees)) = waiting.pop() {
            for tree in trees {
                let Listed {
                    name,
                    kind,
                    inode,
                    foreign,
                } = tree.listed;
                let child = self.push(folder, kind, &name, inode);
                if foreign {
                    self.flags[child as usize] |= FOREIGN;
                }
                match tree.inside {
                    Some(inside) => waiting.push((child, inside)),
                    None => self.flags[child as usize] |= BLOCKED,
                }
            }
        }
    }

    fn kill_below(&mut self, id: u32) {
        let mut waiting = vec![id];
        while let Some(at) = waiting.pop() {
            if self.alive(at) {
                self.flags[at as usize] &= !ALIVE;
                self.dead += 1;
            }
            waiting.extend(self.children(at));
        }
    }

    fn child(&self, folder: u32, name: &str) -> Option<u32> {
        self.children(folder)
            .find(|child| self.name(*child) == name)
    }

    fn locate(&self, path: &Path) -> u32 {
        let drive = files::icloud_drive(&self.home);
        let (mut at, rest) = match path.strip_prefix(&drive) {
            Ok(rest) => (1, rest),
            Err(_) => match path.strip_prefix(&self.home) {
                Ok(rest) => (0, rest),
                Err(_) => return NONE,
            },
        };
        for part in rest.components() {
            let Component::Normal(name) = part else {
                return NONE;
            };
            let Some(name) = name.to_str() else {
                return NONE;
            };
            match self.child(at, name) {
                Some(child) => at = child,
                None if self.opens(at) && files::listed(name, at == 0) => break,
                None => return NONE,
            }
        }
        at
    }

    fn on_disk(&self, id: u32) -> u32 {
        let mut at = id;
        while self.kind(at) != Kind::Root && !still_a_folder(&self.path(at)) {
            at = self.parent[at as usize];
        }
        at
    }

    fn now_foreign(&self, folder: u32) -> bool {
        self.kind(folder) != Kind::Root
            && std::fs::symlink_metadata(self.path(folder))
                .is_ok_and(|found| found.dev() != self.device)
    }

    pub fn refresh(&mut self, path: &Path, deep: Option<SystemTime>) -> bool {
        let changes = self.changes(path, deep);
        self.apply(changes)
    }

    pub fn changes(&self, path: &Path, deep: Option<SystemTime>) -> Changes {
        let found = self.locate(path);
        if found == NONE {
            return Changes::default();
        }
        let first = self.on_disk(found);
        let mut compared = Vec::new();
        let mut waiting = vec![first];
        while let Some(folder) = waiting.pop() {
            if !self.opens(folder) {
                continue;
            }
            if self.now_foreign(folder) {
                compared.push(Compared {
                    folder,
                    on_disk: Some(Vec::new()),
                    anew: true,
                    foreign: true,
                });
                continue;
            }
            if folder == first || self.blocked_at(folder) || self.modified_since(folder, deep) {
                compared.push(self.compare(folder));
            }
            if deep.is_some() {
                waiting.extend(
                    self.children(folder)
                        .filter(|child| self.kind(*child) == Kind::Folder),
                );
            }
        }
        Changes(compared)
    }

    fn blocked_at(&self, id: u32) -> bool {
        self.flags[id as usize] & BLOCKED != 0
    }

    fn modified_since(&self, folder: u32, since: Option<SystemTime>) -> bool {
        let Some(since) = since else {
            return false;
        };
        std::fs::metadata(self.path(folder))
            .and_then(|found| found.modified())
            .ok()
            .is_none_or(|modified| modified >= since)
    }

    fn compare(&self, folder: u32) -> Compared {
        let path = self.path(folder);
        let anew = self.blocked_at(folder);
        let Some(listed) = listing(Path::new(&path), folder == 0, self.device) else {
            return Compared {
                folder,
                on_disk: None,
                anew: false,
                foreign: false,
            };
        };
        let known: HashSet<Listed> = if anew {
            HashSet::new()
        } else {
            self.children(folder)
                .map(|child| self.listed(child))
                .collect()
        };
        let on_disk = listed
            .into_iter()
            .map(|listed| {
                let inside = if descends(&listed) && !known.contains(&listed) {
                    read_tree(&Path::new(&path).join(&listed.name), false, self.device)
                } else {
                    Some(Vec::new())
                };
                Tree { listed, inside }
            })
            .collect();
        Compared {
            folder,
            on_disk: Some(on_disk),
            anew,
            foreign: false,
        }
    }

    fn listed(&self, id: u32) -> Listed {
        Listed {
            name: self.name(id).to_owned(),
            kind: self.kind(id),
            inode: self.inode[id as usize],
            foreign: self.flags[id as usize] & FOREIGN != 0,
        }
    }

    pub fn apply(&mut self, changes: Changes) -> bool {
        let mut changed = false;
        for Compared {
            folder,
            on_disk,
            anew,
            foreign,
        } in changes.0
        {
            if !self.alive(folder) {
                continue;
            }
            if foreign {
                changed |= self.flags[folder as usize] & FOREIGN == 0;
                self.flags[folder as usize] |= FOREIGN;
            }
            match on_disk {
                Some(on_disk) => {
                    changed |= self.blocked_at(folder);
                    self.flags[folder as usize] &= !BLOCKED;
                    changed |= self.replace_children(folder, on_disk, anew);
                }
                None => {
                    changed |= !self.blocked_at(folder);
                    self.flags[folder as usize] |= BLOCKED;
                }
            }
        }
        if self.dead > self.parent.len() / 2 {
            *self = self.compacted();
        }
        changed
    }

    fn replace_children(&mut self, folder: u32, on_disk: Vec<Tree>, anew: bool) -> bool {
        let present: HashSet<&Listed> = on_disk.iter().map(|tree| &tree.listed).collect();
        let (kept, gone): (Vec<u32>, Vec<u32>) = self.children(folder).partition(|child| {
            self.kind(*child) == Kind::Root || (!anew && present.contains(&self.listed(*child)))
        });
        let known: HashSet<Listed> = kept.iter().map(|child| self.listed(*child)).collect();
        let added: Vec<Tree> = on_disk
            .into_iter()
            .filter(|tree| !known.contains(&tree.listed))
            .collect();
        let changed = !gone.is_empty() || !added.is_empty();
        for child in gone {
            self.kill_below(child);
        }
        self.first_child[folder as usize] = NONE;
        for child in kept.into_iter().rev() {
            self.next_sibling[child as usize] = self.first_child[folder as usize];
            self.first_child[folder as usize] = child;
        }
        self.graft(folder, added);
        changed
    }

    pub fn block(&mut self, path: &Path) {
        let id = self.locate(path);
        if id != NONE && Path::new(&self.path(id)) == path {
            self.flags[id as usize] |= BLOCKED;
        }
    }

    pub fn blocked(&self) -> Vec<String> {
        (0..offset(self.parent.len()))
            .filter(|id| self.alive(*id) && self.flags[*id as usize] & BLOCKED != 0)
            .map(|id| self.path(id))
            .collect()
    }

    pub fn search(&self, query: &str, most: usize, leave_out: &[String]) -> Vec<Hit> {
        let left_out: HashSet<u32> = leave_out
            .iter()
            .map(|path| self.locate(Path::new(path)))
            .filter(|id| *id != NONE)
            .collect();
        let words: Vec<String> = query
            .split_whitespace()
            .map(fold)
            .filter(|word| !word.is_empty())
            .collect();
        let Some(longest) = words.iter().max_by_key(|word| word.len()) else {
            return Vec::new();
        };
        let mut found: Vec<u32> = Vec::new();
        for at in memmem::find_iter(&self.folded, longest.as_bytes()) {
            let id = self.owner(at);
            if found.last() == Some(&id) {
                continue;
            }
            if self.alive(id)
                && self.kind(id) != Kind::Root
                && self.holds_all(id, &words)
                && !self.inside_any(id, &left_out)
            {
                found.push(id);
            }
        }
        let query = fold(query);
        let mut scored: Vec<Scored> = found
            .into_iter()
            .map(|id| {
                let name = std::str::from_utf8(self.folded_name(id)).unwrap_or_default();
                (files::closeness_of_folded(name, &query), self.depth(id), id)
            })
            .collect();
        if most > 0 && scored.len() > most {
            scored.select_nth_unstable_by(most - 1, best_first);
            scored.truncate(most);
        }
        scored.sort_unstable_by(best_first);
        scored
            .into_iter()
            .map(|(closeness, _, id)| Hit {
                path: self.path(id),
                is_file: self.kind(id) == Kind::File,
                closeness,
            })
            .collect()
    }

    fn inside_any(&self, id: u32, folders: &HashSet<u32>) -> bool {
        if folders.is_empty() {
            return false;
        }
        std::iter::successors(Some(self.parent[id as usize]), |at| {
            Some(self.parent[*at as usize]).filter(|parent| *parent != NONE)
        })
        .any(|folder| folders.contains(&folder))
    }

    fn owner(&self, at: usize) -> u32 {
        let at = offset(at);
        offset(
            self.folded_at
                .partition_point(|start| *start <= at)
                .saturating_sub(1),
        )
    }

    fn holds_all(&self, id: u32, words: &[String]) -> bool {
        let name = self.folded_name(id);
        words
            .iter()
            .all(|word| memmem::find(name, word.as_bytes()).is_some())
    }

    fn live_in_order(&self) -> (Vec<u32>, Vec<u32>) {
        let mut order = Vec::with_capacity(self.len());
        let mut renumbered = vec![NONE; self.parent.len()];
        let mut waiting = vec![0];
        while let Some(old) = waiting.pop() {
            renumbered[old as usize] = offset(order.len());
            order.push(old);
            waiting.extend(self.children(old));
        }
        (order, renumbered)
    }

    fn compacted(&self) -> Self {
        let mut fresh = Self {
            home: self.home.clone(),
            device: self.device,
            ..Self::default()
        };
        let (order, renumbered) = self.live_in_order();
        for old in order {
            let parent = self.parent[old as usize];
            let parent = if parent == NONE {
                NONE
            } else {
                renumbered[parent as usize]
            };
            let new = fresh.push(
                parent,
                self.kind(old),
                self.name(old),
                self.inode[old as usize],
            );
            fresh.flags[new as usize] |= self.flags[old as usize] & (BLOCKED | FOREIGN);
        }
        fresh.shrink();
        fresh
    }

    pub fn write(&self, stamp: &Stamp, mut out: impl Write) -> io::Result<()> {
        let (order, renumbered) = self.live_in_order();
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&stamp.home.to_le_bytes())?;
        out.write_all(&stamp.drive.to_le_bytes())?;
        out.write_all(&stamp.at.to_le_bytes())?;
        out.write_all(&offset(stamp.history.len()).to_le_bytes())?;
        out.write_all(stamp.history.as_bytes())?;
        out.write_all(&offset(order.len()).to_le_bytes())?;
        for old in order {
            let parent = self.parent[old as usize];
            let parent = if parent == NONE {
                NONE
            } else {
                renumbered[parent as usize]
            };
            let name = self.name(old).as_bytes();
            out.write_all(&parent.to_le_bytes())?;
            out.write_all(&[self.flags[old as usize] & KEPT_FLAGS])?;
            out.write_all(&self.inode[old as usize].to_le_bytes())?;
            out.write_all(&offset(name.len()).to_le_bytes())?;
            out.write_all(name)?;
        }
        out.flush()
    }

    pub fn read(mut from: impl Read) -> io::Result<(Self, Stamp)> {
        let mut magic = [0; 8];
        from.read_exact(&mut magic)?;
        if &magic != MAGIC || read_u32(&mut from)? != VERSION {
            return Err(broken("not a Raiden name index"));
        }
        let home = read_u64(&mut from)?;
        let drive = read_u64(&mut from)?;
        let at = read_u64(&mut from)?;
        let history = read_string(&mut from)?;
        let stamp = Stamp {
            home,
            drive,
            at,
            history,
        };
        let count = read_u32(&mut from)?;
        let mut names = Self::default();
        for id in 0..count {
            let parent = read_u32(&mut from)?;
            let mut flags = [0];
            from.read_exact(&mut flags)?;
            let kind = Kind::from_code(flags[0] & KIND_BITS)?;
            let inode = read_u64(&mut from)?;
            let name = read_string(&mut from)?;
            let in_order = if id == 0 { parent == NONE } else { parent < id };
            if !in_order {
                return Err(broken("an entry comes before its folder"));
            }
            if id == 0 {
                names.home.clone_from(&name);
                names.device = device_of(&name);
            }
            let roots_first = match id {
                0 => kind == Kind::Root,
                1 => kind == Kind::Root && name == Self::drive_name(&names.home),
                _ => kind != Kind::Root,
            };
            if !roots_first {
                return Err(broken("the index does not start with its two roots"));
            }
            let new = names.push(parent, kind, &name, inode);
            names.flags[new as usize] |= flags[0] & (BLOCKED | FOREIGN);
        }
        if names.parent.len() < 2 {
            return Err(broken("the index has no roots"));
        }
        names.shrink();
        Ok((names, stamp))
    }
}

fn read_u32(from: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0; 4];
    from.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_string(from: &mut impl Read) -> io::Result<String> {
    let length = read_u32(from)? as usize;
    if length > u16::MAX as usize {
        return Err(broken("a name is too long"));
    }
    let mut bytes = vec![0; length];
    from.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| broken("a name is not UTF-8"))
}

fn read_u64(from: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0; 8];
    from.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn device_of(path: &str) -> u64 {
    std::fs::metadata(path).map_or(0, |found| found.dev())
}

fn listing(folder: &Path, at_home: bool, device: u64) -> Option<Vec<Listed>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if gone(&error) => return Some(Vec::new()),
        Err(_) => return None,
    };
    let mut listed = Vec::new();
    for entry in entries {
        let entry = entry.ok()?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !files::listed(&name, at_home) {
            continue;
        }
        let file_type = entry.file_type().ok()?;
        let kind = if file_type.is_dir() {
            Kind::Folder
        } else if file_type.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let (inode, foreign) = if kind == Kind::Folder && files::opened(&name) {
            let device_here = entry.metadata().map_or(device, |found| found.dev());
            (entry.ino(), device_here != device)
        } else if kind == Kind::Folder {
            (entry.ino(), false)
        } else {
            (0, false)
        };
        listed.push(Listed {
            name,
            kind,
            inode,
            foreign,
        });
    }
    Some(listed)
}

fn still_a_folder(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.is_dir())
}

fn descends(listed: &Listed) -> bool {
    listed.kind == Kind::Folder && !listed.foreign && files::opened(&listed.name)
}

fn gone(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

fn read_tree(folder: &Path, at_home: bool, device: u64) -> Option<Vec<Tree>> {
    let trees = listing(folder, at_home, device)?
        .into_iter()
        .map(|listed| {
            let inside = if descends(&listed) {
                read_tree(&folder.join(&listed.name), false, device)
            } else {
                Some(Vec::new())
            };
            Tree { listed, inside }
        })
        .collect();
    Some(trees)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct Home(PathBuf);

    impl Home {
        fn new(label: &str) -> Self {
            let unique = format!(
                "raiden-names-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            );
            let folder = std::env::temp_dir().join(unique);
            let _ = fs::remove_dir_all(&folder);
            fs::create_dir_all(&folder).unwrap();
            Self(folder.canonicalize().unwrap())
        }

        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, relative).unwrap();
            path
        }

        fn folder(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn found(names: &Names, query: &str) -> Vec<String> {
        let home = format!("{}/", names.home());
        names
            .search(query, 5000, &[])
            .into_iter()
            .map(|hit| hit.path.trim_start_matches(&home).to_owned())
            .collect()
    }

    #[test]
    fn finds_files_and_folders_by_any_part_of_their_name() {
        let home = Home::new("parts");
        home.file("Code/lantern/AGENTS.md");
        home.file("Documents/Release notes.pdf");
        let names = Names::build(home.path());
        assert_eq!(found(&names, "agents"), ["Code/lantern/AGENTS.md"]);
        assert_eq!(found(&names, "lease no"), ["Documents/Release notes.pdf"]);
        assert_eq!(found(&names, "antern"), ["Code/lantern"]);
        assert!(found(&names, "nowhere").is_empty());
    }

    #[test]
    fn every_word_must_be_in_the_same_name() {
        let home = Home::new("words");
        home.file("release/notes.md");
        home.file("release notes.md");
        let names = Names::build(home.path());
        assert_eq!(found(&names, "notes release"), ["release notes.md"]);
    }

    #[test]
    fn case_and_accents_do_not_matter() {
        let home = Home::new("accents");
        home.file("Música/Canción Final.mp3");
        let names = Names::build(home.path());
        assert_eq!(found(&names, "cancion"), ["Música/Canción Final.mp3"]);
        assert_eq!(found(&names, "MUSICA"), ["Música"]);
        assert_eq!(found(&names, "canción"), ["Música/Canción Final.mp3"]);
    }

    #[test]
    fn dotted_capitals_fold_like_their_plain_letters() {
        let home = Home::new("dotted");
        home.file("İstanbul.pdf");
        let names = Names::build(home.path());
        assert_eq!(found(&names, "istanbul"), ["İstanbul.pdf"]);
    }

    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_folder_macos_will_not_open_keeps_what_was_known_and_says_so() {
        let home = Home::new("blocked");
        home.file("Documents/plan.txt");
        let documents = home.0.join("Documents");
        let mut names = Names::build(home.path());
        assert!(names.blocked().is_empty());
        set_mode(&documents, 0o000);
        names.refresh(&documents, None);
        let blocked = names.blocked();
        set_mode(&documents, 0o755);
        assert_eq!(blocked, [documents.to_str().unwrap()]);
        assert_eq!(found(&names, "plan"), ["Documents/plan.txt"]);
        home.file("Documents/later.txt");
        names.refresh(&documents, None);
        assert!(names.blocked().is_empty());
        assert_eq!(found(&names, "later"), ["Documents/later.txt"]);
    }

    #[test]
    fn a_folder_blocked_from_the_start_is_reported() {
        let home = Home::new("blocked-start");
        home.file("Desktop/x.txt");
        let desktop = home.0.join("Desktop");
        set_mode(&desktop, 0o000);
        let names = Names::build(home.path());
        let mut saved = Vec::new();
        names.write(&Stamp::default(), &mut saved).unwrap();
        set_mode(&desktop, 0o755);
        assert_eq!(names.blocked(), [desktop.to_str().unwrap()]);
        let (loaded, _) = Names::read(saved.as_slice()).unwrap();
        assert_eq!(loaded.blocked(), [desktop.to_str().unwrap()]);
    }

    #[test]
    fn reading_changes_alters_nothing_until_they_are_applied() {
        let home = Home::new("two-steps");
        home.file("Docs/old.txt");
        let mut names = Names::build(home.path());
        fs::remove_file(home.0.join("Docs/old.txt")).unwrap();
        home.file("Docs/Sub/new.txt");
        let changes = names.changes(&home.0.join("Docs"), None);
        assert_eq!(found(&names, "old"), ["Docs/old.txt"]);
        assert!(found(&names, "new").is_empty());
        names.apply(changes);
        assert!(found(&names, "old").is_empty());
        assert_eq!(found(&names, "new"), ["Docs/Sub/new.txt"]);
    }

    #[test]
    fn half_of_a_big_folder_deleted_leaves_exactly_the_other_half() {
        let home = Home::new("half");
        for at in 0..2000 {
            home.file(&format!("Photos/img-{at:04}.jpg"));
        }
        let mut names = Names::build(home.path());
        for at in (0..2000).step_by(2) {
            fs::remove_file(home.0.join(format!("Photos/img-{at:04}.jpg"))).unwrap();
        }
        names.refresh(&home.0.join("Photos"), None);
        let mut left: Vec<String> = found(&names, "img-");
        left.sort();
        let wanted: Vec<String> = (1..2000)
            .step_by(2)
            .map(|at| format!("Photos/img-{at:04}.jpg"))
            .collect();
        assert_eq!(left.len(), 1000);
        assert_eq!(left, wanted);
    }

    fn age(folder: &Path) {
        let long_ago = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs::File::open(folder)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
    }

    #[test]
    fn a_deep_check_reads_again_only_folders_changed_since_events_were_lost() {
        let home = Home::new("pruned");
        home.file("Code/old/a.rs");
        home.file("Code/new/b.rs");
        let mut names = Names::build(home.path());
        let lost_at = SystemTime::now() - std::time::Duration::from_secs(5);
        home.file("Code/old/unseen.rs");
        home.file("Code/new/seen.rs");
        age(&home.0.join("Code/old"));
        names.refresh(&home.0.join("Code"), Some(lost_at));
        assert_eq!(found(&names, "seen.rs"), ["Code/new/seen.rs"]);
        names.refresh(&home.0.join("Code"), Some(SystemTime::UNIX_EPOCH));
        let mut both = found(&names, "seen.rs");
        both.sort();
        assert_eq!(both, ["Code/new/seen.rs", "Code/old/unseen.rs"]);
    }

    #[test]
    fn a_folder_swapped_in_under_the_same_name_is_read_whole() {
        let home = Home::new("swapped");
        home.file("Project/build/stale.o");
        home.file("Project/build.new/fresh.o");
        let mut names = Names::build(home.path());
        let project = home.0.join("Project");
        fs::rename(project.join("build"), project.join("build.old")).unwrap();
        fs::rename(project.join("build.new"), project.join("build")).unwrap();
        fs::remove_dir_all(project.join("build.old")).unwrap();
        names.refresh(&project, None);
        assert!(found(&names, "stale").is_empty());
        assert_eq!(found(&names, "fresh"), ["Project/build/fresh.o"]);
    }

    #[test]
    fn a_folder_opened_up_after_being_blocked_is_read_whole() {
        let home = Home::new("opened-up");
        let documents = home.folder("Documents");
        set_mode(&documents, 0o000);
        let mut names = Names::build(home.path());
        set_mode(&documents, 0o755);
        home.file("Documents/Taxes/2026/return.pdf");
        names.refresh(&documents, None);
        assert!(names.blocked().is_empty());
        assert_eq!(found(&names, "return"), ["Documents/Taxes/2026/return.pdf"]);
    }

    #[test]
    fn only_a_missing_folder_counts_as_empty() {
        assert!(gone(&io::Error::from(io::ErrorKind::NotFound)));
        assert!(gone(&io::Error::from(io::ErrorKind::NotADirectory)));
        for passing in [
            io::ErrorKind::TimedOut,
            io::ErrorKind::Interrupted,
            io::ErrorKind::OutOfMemory,
            io::ErrorKind::Other,
        ] {
            assert!(!gone(&io::Error::from(passing)), "{passing:?}");
        }
        assert!(
            listing(Path::new("/no/such/folder"), false, 0).is_some_and(|listed| listed.is_empty())
        );
    }

    #[test]
    fn refreshing_says_whether_anything_changed() {
        let home = Home::new("changed");
        home.file("Docs/a.txt");
        let mut names = Names::build(home.path());
        assert!(!names.refresh(&home.0.join("Docs"), None));
        home.file("Docs/b.txt");
        assert!(names.refresh(&home.0.join("Docs"), None));
        fs::remove_file(home.0.join("Docs/a.txt")).unwrap();
        assert!(names.refresh(&home.0.join("Docs"), None));
    }

    #[test]
    fn a_link_to_a_folder_is_listed_but_neither_followed_nor_called_a_file() {
        let home = Home::new("links");
        home.file("Real/inside.txt");
        std::os::unix::fs::symlink(home.0.join("Real"), home.0.join("Shortcut")).unwrap();
        let names = Names::build(home.path());
        let hits = names.search("shortcut", 10, &[]);
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].is_file);
        assert_eq!(found(&names, "inside"), ["Real/inside.txt"]);
    }

    struct Mounted(PathBuf);

    impl Drop for Mounted {
        fn drop(&mut self) {
            let _ = std::process::Command::new("/usr/bin/hdiutil")
                .args(["detach", "-force"])
                .arg(&self.0)
                .output();
        }
    }

    fn mount_volume(at: &Path, image: &Path) -> Option<Mounted> {
        let made = std::process::Command::new("/usr/bin/hdiutil")
            .args([
                "create",
                "-size",
                "4m",
                "-fs",
                "HFS+",
                "-volname",
                "RaidenTest",
            ])
            .arg(image)
            .output()
            .ok()?;
        fs::create_dir_all(at).ok()?;
        let attached = std::process::Command::new("/usr/bin/hdiutil")
            .args(["attach", "-nobrowse", "-mountpoint"])
            .arg(at)
            .arg(image.with_extension("dmg"))
            .output()
            .ok()?;
        (made.status.success() && attached.status.success()).then(|| Mounted(at.to_owned()))
    }

    #[test]
    fn a_volume_mounted_inside_home_is_listed_but_never_read() {
        let home = Home::new("mounted");
        let scratch = std::env::temp_dir().join(format!("raiden-volume-{}", std::process::id()));
        let Some(volume) = mount_volume(&home.0.join("Shares/team"), &scratch) else {
            eprintln!("hdiutil could not mount a test volume here, so this check cannot run.");
            return;
        };
        fs::write(volume.0.join("remote-report.txt"), "").unwrap();
        home.file("Shares/local-note.txt");
        let mut names = Names::build(home.path());
        assert_eq!(found(&names, "team"), ["Shares/team"]);
        assert!(found(&names, "remote-report").is_empty());
        names.refresh(&volume.0, Some(SystemTime::UNIX_EPOCH));
        assert!(found(&names, "remote-report").is_empty());
        assert_eq!(found(&names, "local-note"), ["Shares/local-note.txt"]);
        drop(volume);
        let _ = fs::remove_file(scratch.with_extension("dmg"));
    }

    #[test]
    fn a_volume_mounted_over_a_known_folder_is_not_read_and_its_old_names_go() {
        let home = Home::new("mounted-over");
        home.file("Shares/team/old-local.txt");
        let mut names = Names::build(home.path());
        let team = home.0.join("Shares/team");
        fs::remove_file(team.join("old-local.txt")).unwrap();
        let scratch = std::env::temp_dir().join(format!("raiden-over-{}", std::process::id()));
        let Some(volume) = mount_volume(&team, &scratch) else {
            eprintln!("hdiutil could not mount a test volume here, so this check cannot run.");
            return;
        };
        fs::write(volume.0.join("remote.txt"), "").unwrap();
        names.refresh(&team, None);
        assert!(found(&names, "remote").is_empty());
        assert!(found(&names, "old-local").is_empty());
        assert_eq!(found(&names, "team"), ["Shares/team"]);
        drop(volume);
        let _ = fs::remove_file(scratch.with_extension("dmg"));
    }

    #[test]
    fn a_folder_replaced_by_a_link_is_not_followed() {
        let home = Home::new("replaced-by-link");
        home.file("Work/kept.txt");
        home.file("Elsewhere/secret.txt");
        let mut names = Names::build(home.path());
        fs::remove_dir_all(home.0.join("Work")).unwrap();
        std::os::unix::fs::symlink(home.0.join("Elsewhere"), home.0.join("Work")).unwrap();
        names.refresh(&home.0.join("Work"), None);
        assert!(found(&names, "kept").is_empty());
        assert_eq!(found(&names, "secret"), ["Elsewhere/secret.txt"]);
        let work = names.search("work", 10, &[]);
        assert_eq!(work.len(), 1);
        assert!(!work[0].is_file);
    }

    #[test]
    fn hits_inside_left_out_folders_never_crowd_out_the_rest() {
        let home = Home::new("left-out");
        for at in 0..20 {
            home.file(&format!("Documents/report {at}.txt"));
        }
        home.file("Code/deep/er/still/report.txt");
        let names = Names::build(home.path());
        let documents = home.0.join("Documents").to_str().unwrap().to_owned();
        let hits = names.search("report", 5, std::slice::from_ref(&documents));
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("Code/deep/er/still/report.txt"));
        assert_eq!(names.search("report", 5, &[]).len(), 5);
    }

    #[test]
    fn a_folder_can_be_marked_blocked_by_its_watcher() {
        let home = Home::new("mark-blocked");
        let documents = home.folder("Documents");
        let mut names = Names::build(home.path());
        names.block(&documents);
        names.block(&home.0.join("Documents/not-there"));
        assert_eq!(names.blocked(), [documents.to_str().unwrap()]);
    }

    #[test]
    fn something_new_at_the_top_of_home_counts_as_a_change() {
        let home = Home::new("top-level");
        let mut names = Names::build(home.path());
        home.file("new-at-home.txt");
        assert!(names.refresh(&home.0, None));
        assert_eq!(found(&names, "new-at-home"), ["new-at-home.txt"]);
    }

    #[test]
    fn a_query_of_only_accent_marks_finds_nothing() {
        let home = Home::new("marks");
        home.file("abc");
        assert!(
            Names::build(home.path())
                .search("\u{301}", 10, &[])
                .is_empty()
        );
    }

    #[test]
    fn a_word_never_matches_across_two_names() {
        let home = Home::new("across");
        home.file("abc");
        home.file("def");
        let names = Names::build(home.path());
        assert!(found(&names, "cd").is_empty());
        assert!(found(&names, "c/d").is_empty());
    }

    #[test]
    fn noise_hidden_files_and_package_insides_are_not_indexed() {
        let home = Home::new("noise");
        for noise in [
            ".config/needle.md",
            "Library/Caches/needle.md",
            "Applications/needle.app/x",
            "Code/web/node_modules/needle.js",
            "Code/rust/target/needle.d",
            "Downloads/Needle.app/Contents/Info.plist",
        ] {
            home.file(noise);
        }
        home.file("Code/needle.rs");
        let names = Names::build(home.path());
        let mut needles = found(&names, "needle");
        needles.sort();
        assert_eq!(needles, ["Code/needle.rs", "Downloads/Needle.app"]);
    }

    #[test]
    fn icloud_drive_is_indexed_even_though_library_is_not() {
        let home = Home::new("icloud");
        home.file("Library/Mobile Documents/com~apple~CloudDocs/Work/plan.key");
        home.file("Library/Mobile Documents/com~apple~Pages/plan.pages");
        let names = Names::build(home.path());
        assert_eq!(
            found(&names, "plan"),
            ["Library/Mobile Documents/com~apple~CloudDocs/Work/plan.key"]
        );
    }

    #[test]
    fn exact_names_come_before_longer_ones_then_shallow_before_deep() {
        let home = Home::new("rank");
        home.file("a/b/c/notes");
        home.file("my notes.md");
        home.file("notes");
        home.file("x/notes");
        let names = Names::build(home.path());
        assert_eq!(
            found(&names, "notes"),
            ["notes", "x/notes", "a/b/c/notes", "my notes.md"]
        );
    }

    #[test]
    fn only_the_best_are_kept_when_too_many_match() {
        let home = Home::new("most");
        for at in 0..50 {
            home.file(&format!("deep/er/report {at}.txt"));
        }
        home.file("report");
        let names = Names::build(home.path());
        let hits = names.search("report", 5, &[]);
        assert_eq!(hits.len(), 5);
        assert!(hits[0].path.ends_with("/report"));
        assert!(hits[0].is_file);
    }

    #[test]
    fn a_new_file_is_found_after_its_folder_is_refreshed() {
        let home = Home::new("added");
        let folder = home.folder("Documents");
        let mut names = Names::build(home.path());
        home.file("Documents/minutes.txt");
        assert!(found(&names, "minutes").is_empty());
        names.refresh(&folder, None);
        assert_eq!(found(&names, "minutes"), ["Documents/minutes.txt"]);
    }

    #[test]
    fn a_new_folder_is_read_whole() {
        let home = Home::new("new-folder");
        let mut names = Names::build(home.path());
        home.file("Projects/raiden/src/main.rs");
        names.refresh(&home.0.join("Projects/raiden/src"), None);
        assert_eq!(found(&names, "main.rs"), ["Projects/raiden/src/main.rs"]);
        assert_eq!(found(&names, "raiden"), ["Projects/raiden"]);
    }

    #[test]
    fn a_deleted_folder_takes_everything_inside_with_it() {
        let home = Home::new("deleted");
        home.file("Old/a/one.txt");
        home.file("Old/b/two.txt");
        let mut names = Names::build(home.path());
        fs::remove_dir_all(home.0.join("Old")).unwrap();
        names.refresh(&home.0.join("Old"), None);
        assert!(found(&names, "one").is_empty());
        assert!(found(&names, "two").is_empty());
        assert!(found(&names, "old").is_empty());
    }

    #[test]
    fn a_rename_shows_the_new_name_only() {
        let home = Home::new("renamed");
        home.file("Docs/draft.txt");
        let mut names = Names::build(home.path());
        fs::rename(home.0.join("Docs/draft.txt"), home.0.join("Docs/final.txt")).unwrap();
        names.refresh(&home.0.join("Docs"), None);
        assert!(found(&names, "draft").is_empty());
        assert_eq!(found(&names, "final"), ["Docs/final.txt"]);
    }

    #[test]
    fn a_deep_refresh_catches_changes_below_the_folder() {
        let home = Home::new("deep");
        home.file("Code/app/src/old.rs");
        let mut names = Names::build(home.path());
        fs::remove_file(home.0.join("Code/app/src/old.rs")).unwrap();
        home.file("Code/app/src/new.rs");
        names.refresh(&home.0.join("Code"), None);
        assert_eq!(found(&names, "old.rs"), ["Code/app/src/old.rs"]);
        names.refresh(&home.0.join("Code"), Some(SystemTime::UNIX_EPOCH));
        assert!(found(&names, "old.rs").is_empty());
        assert_eq!(found(&names, "new.rs"), ["Code/app/src/new.rs"]);
    }

    #[test]
    fn a_file_that_becomes_a_folder_is_replaced() {
        let home = Home::new("kind");
        home.file("thing");
        let mut names = Names::build(home.path());
        fs::remove_file(home.0.join("thing")).unwrap();
        home.file("thing/inside.txt");
        names.refresh(&home.0, None);
        let hits = names.search("thing", 10, &[]);
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].is_file);
        assert_eq!(found(&names, "inside"), ["thing/inside.txt"]);
    }

    #[test]
    fn refreshing_outside_home_or_inside_noise_changes_nothing() {
        let home = Home::new("outside");
        home.file("Code/web/node_modules/pkg/index.js");
        let mut names = Names::build(home.path());
        let before = names.len();
        names.refresh(Path::new("/private/var"), Some(SystemTime::UNIX_EPOCH));
        names.refresh(
            &home.0.join("Code/web/node_modules/pkg"),
            Some(SystemTime::UNIX_EPOCH),
        );
        assert_eq!(names.len(), before);
        assert!(found(&names, "index").is_empty());
    }

    #[test]
    fn changes_in_hidden_folders_and_library_never_relist_home() {
        let home = Home::new("hidden");
        home.file(".cache/old");
        let mut names = Names::build(home.path());
        home.file("unseen.txt");
        home.file(".cache/new/blob");
        home.file("Library/Caches/blob");
        names.refresh(&home.0.join(".cache/new"), None);
        names.refresh(&home.0.join("Library/Caches"), Some(SystemTime::UNIX_EPOCH));
        assert!(found(&names, "unseen").is_empty());
    }

    #[test]
    fn many_deletions_compact_the_index_without_losing_anything() {
        let home = Home::new("compact");
        for at in 0..40 {
            home.file(&format!("Temp/file {at}.txt"));
        }
        home.file("Keep/kept.txt");
        let mut names = Names::build(home.path());
        fs::remove_dir_all(home.0.join("Temp")).unwrap();
        names.refresh(&home.0, None);
        assert_eq!(names.dead, 0);
        assert_eq!(found(&names, "kept"), ["Keep/kept.txt"]);
        assert!(found(&names, "file").is_empty());
    }

    #[test]
    fn the_index_survives_a_round_trip_to_disk() {
        let home = Home::new("round-trip");
        home.file("Code/lantern/AGENTS.md");
        home.file("Library/Mobile Documents/com~apple~CloudDocs/plan.key");
        home.file("Gone/x.txt");
        let mut names = Names::build(home.path());
        fs::remove_dir_all(home.0.join("Gone")).unwrap();
        names.refresh(&home.0, None);
        let stamp = Stamp {
            home: 123_456_789,
            drive: 123_000_000,
            at: 1_790_000_000,
            history: "6F9C2A1E".to_owned(),
        };
        let mut saved = Vec::new();
        names.write(&stamp, &mut saved).unwrap();
        let (loaded, read_stamp) = Names::read(saved.as_slice()).unwrap();
        assert_eq!(read_stamp, stamp);
        assert_eq!(loaded.home(), home.path());
        assert_eq!(loaded.len(), names.len());
        assert_eq!(found(&loaded, "agents"), ["Code/lantern/AGENTS.md"]);
        assert_eq!(found(&loaded, "plan").len(), 1);
        assert!(found(&loaded, "x.txt").is_empty());
        let mut loaded = loaded;
        home.file("Code/added.txt");
        loaded.refresh(&home.0.join("Code"), None);
        assert_eq!(found(&loaded, "added"), ["Code/added.txt"]);
    }

    #[test]
    fn a_damaged_index_is_refused_rather_than_misread() {
        let home = Home::new("damaged");
        home.file("a.txt");
        let mut saved = Vec::new();
        Names::build(home.path())
            .write(&Stamp::default(), &mut saved)
            .unwrap();
        assert!(Names::read(&saved[..saved.len() - 1]).is_err());
        assert!(Names::read(&b"NOTRAIDEN"[..]).is_err());
        let mut wrong_version = saved.clone();
        wrong_version[8] = 9;
        assert!(Names::read(wrong_version.as_slice()).is_err());
    }

    #[test]
    #[ignore = "reads the whole home folder; run with --ignored --nocapture to measure"]
    fn measures_the_real_home_folder() {
        let home = std::env::var("HOME").unwrap();
        let started = std::time::Instant::now();
        let names = Names::build(&home);
        let built = started.elapsed();
        let mut saved = Vec::new();
        names.write(&Stamp::default(), &mut saved).unwrap();
        let started = std::time::Instant::now();
        let (loaded, _) = Names::read(saved.as_slice()).unwrap();
        let read = started.elapsed();
        eprintln!(
            "{} entries, built in {built:?}, {:.1} MB in memory, {:.1} MB on disk, read back in {read:?}",
            names.len(),
            names.bytes() as f64 / 1e6,
            saved.len() as f64 / 1e6,
        );
        for query in ["e", "ent", "agents", "readme md", "canción", "zzzqqq"] {
            let started = std::time::Instant::now();
            let hits = loaded.search(query, 5000, &[]);
            eprintln!(
                "{query:>10}: {:>6} hits in {:?}",
                hits.len(),
                started.elapsed()
            );
        }
    }
}
