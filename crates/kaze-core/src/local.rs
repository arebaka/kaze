use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;

use crate::listing::{Cancellation, ListingSummary, UnreadableEntry};
use crate::resource::{
    ErrorKind, Link, Location, Place, Resource, ResourceError, ResourceKind, ResourceRef,
};

const LARGEST_BATCH: usize = 1024;

pub fn home() -> Result<Location, ResourceError> {
    let path = platform::home_directory().ok_or(ResourceError::new(ErrorKind::NotFound))?;
    if !fs::metadata(&path)?.is_dir() {
        return Err(ResourceError::new(ErrorKind::NotAContainer));
    }
    Ok(Location::new(ResourceRef::Local(path)).with_place(Place::Home))
}

pub(crate) fn list(
    directory: &Path,
    batch_size: usize,
    cancellation: &Cancellation,
    mut deliver: impl FnMut(Vec<Resource>) -> bool,
) -> Result<ListingSummary, ResourceError> {
    if !fs::metadata(directory)?.is_dir() {
        return Err(ResourceError::new(ErrorKind::NotAContainer));
    }
    let mut batch_size = batch_size.max(1);
    let accounts = platform::Accounts::new();
    let mut batch = Vec::new();
    let mut unreadable = Vec::new();
    for entry in fs::read_dir(directory)? {
        if cancellation.is_cancelled() {
            return Err(ResourceError::new(ErrorKind::Cancelled));
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                unreadable.push(UnreadableEntry {
                    resource: ResourceRef::Local(directory.to_path_buf()),
                    error: error.into(),
                });
                break;
            }
        };
        match read_entry(&entry, &accounts) {
            Ok(Some(resource)) => batch.push(resource),
            Ok(None) => {}
            Err(error) => unreadable.push(UnreadableEntry {
                resource: ResourceRef::Local(entry.path()),
                error: error.into(),
            }),
        }
        if batch.len() >= batch_size {
            if !deliver(std::mem::take(&mut batch)) {
                return Err(ResourceError::new(ErrorKind::Cancelled));
            }
            let grown = batch_size.saturating_mul(2).min(LARGEST_BATCH);
            batch_size = grown.max(batch_size);
        }
    }
    if !batch.is_empty() && !deliver(batch) {
        return Err(ResourceError::new(ErrorKind::Cancelled));
    }
    Ok(ListingSummary { unreadable })
}

fn read_entry(entry: &fs::DirEntry, accounts: &platform::Accounts) -> io::Result<Option<Resource>> {
    let name = entry.file_name();
    let hidden = platform::is_hidden(&name);
    if hidden {
        return Ok(None);
    }
    let path = entry.path();
    let own = entry.metadata()?;
    let (metadata, kind, link) = if own.file_type().is_symlink() {
        match fs::metadata(&path) {
            Ok(target) => {
                let kind = platform::kind(target.file_type());
                (target, kind, Some(Link::Resolved))
            }
            Err(_) => (own, ResourceKind::Other, Some(Link::Broken)),
        }
    } else {
        let kind = platform::kind(own.file_type());
        (own, kind, None)
    };
    Ok(Some(Resource {
        reference: ResourceRef::Local(path),
        name: name.to_string_lossy().into_owned(),
        kind,
        link,
        hidden,
        metadata: accounts.describe(&metadata, kind),
    }))
}
