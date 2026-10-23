use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::PathBuf;

use users::os::unix::UserExt;
use users::{Groups, Users, UsersCache};

use crate::resource::{Access, Metadata, ResourceKind, UnixAttributes};

pub(super) fn home_directory() -> Option<PathBuf> {
    let user = users::get_user_by_uid(users::get_current_uid())?;
    Some(user.home_dir().to_path_buf())
}

pub(super) fn is_hidden(name: &OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

pub(super) fn kind(file_type: fs::FileType) -> ResourceKind {
    if file_type.is_dir() {
        ResourceKind::Directory
    } else if file_type.is_file() {
        ResourceKind::Regular
    } else if file_type.is_fifo() {
        ResourceKind::Pipe
    } else if file_type.is_block_device() {
        ResourceKind::BlockDevice
    } else if file_type.is_char_device() {
        ResourceKind::CharacterDevice
    } else if file_type.is_socket() {
        ResourceKind::Socket
    } else {
        ResourceKind::Other
    }
}

pub(super) struct Accounts {
    cache: UsersCache,
    user_id: u32,
    group_id: u32,
}

impl Accounts {
    pub(super) fn new() -> Self {
        let cache = UsersCache::new();
        let user_id = cache.get_effective_uid();
        let group_id = cache.get_effective_gid();
        Self {
            cache,
            user_id,
            group_id,
        }
    }

    pub(super) fn describe(&self, metadata: &fs::Metadata, kind: ResourceKind) -> Metadata {
        let mode = metadata.mode();
        let user_id = metadata.uid();
        let group_id = metadata.gid();
        let allows = |owner_bit: u32, group_bit: u32, other_bit: u32| {
            (user_id == self.user_id && mode & owner_bit != 0)
                || (group_id == self.group_id && mode & group_bit != 0)
                || mode & other_bit != 0
        };
        let access = Access {
            read: allows(0o400, 0o040, 0o004),
            write: allows(0o200, 0o020, 0o002),
            execute: allows(0o100, 0o010, 0o001),
        };
        let user_name = self
            .cache
            .get_user_by_uid(user_id)
            .map(|user| user.name().to_string_lossy().into_owned());
        let group_name = self
            .cache
            .get_group_by_gid(group_id)
            .map(|group| group.name().to_string_lossy().into_owned());
        Metadata {
            size: (kind == ResourceKind::Regular).then_some(metadata.len()),
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            access: Some(access),
            unix: Some(UnixAttributes {
                mode,
                user_id,
                user_name,
                group_id,
                group_name,
                inode: metadata.ino(),
                hard_links: metadata.nlink(),
            }),
        }
    }
}
