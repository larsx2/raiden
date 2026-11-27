use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::search::Catalog;

static INPUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static OUTPUT: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static CATALOG: Mutex<Option<Catalog>> = Mutex::new(None);

fn hold<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

#[unsafe(no_mangle)]
pub extern "C" fn input(length: usize) -> *mut u8 {
    let mut input = hold(&INPUT);
    input.clear();
    input.resize(length, 0);
    input.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn load() -> usize {
    let catalog = Catalog::from_text(&String::from_utf8_lossy(&hold(&INPUT)));
    let length = catalog.len();
    *hold(&CATALOG) = Some(catalog);
    length
}

fn answer(ask: impl FnOnce(&Catalog, &str) -> Vec<u32>) -> usize {
    let query = String::from_utf8_lossy(&hold(&INPUT)).into_owned();
    let found = hold(&CATALOG)
        .as_ref()
        .map(|catalog| ask(catalog, &query))
        .unwrap_or_default();
    let length = found.len();
    *hold(&OUTPUT) = found;
    length
}

#[unsafe(no_mangle)]
pub extern "C" fn rank() -> usize {
    answer(Catalog::rank)
}

#[unsafe(no_mangle)]
pub extern "C" fn positions(index: usize) -> usize {
    answer(|catalog, query| catalog.positions(index, query))
}

#[unsafe(no_mangle)]
pub extern "C" fn suggestions(limit: usize) -> usize {
    answer(|catalog, _| catalog.suggestions(limit))
}

#[unsafe(no_mangle)]
pub extern "C" fn output() -> *const u32 {
    hold(&OUTPUT).as_ptr()
}
